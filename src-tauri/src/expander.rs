//! Сокращения: «;подп» и пробел в любой программе заменяются текстом сниппета.
//!
//! Низкоуровневый перехватчик клавиатуры видит только нажатия, а не текст программ.
//! Он помнит символы, набранные с последнего пробела, и больше ничего: буфер
//! никуда не пишется и сбрасывается при каждом пробеле, Enter, стрелке или сочетании с Ctrl.

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, OnceLock};

pub struct Match {
    pub typed: usize,
    pub snippet_id: String,
}

struct Shared {
    enabled: AtomicBool,
    prefix: RwLock<String>,
    /// сокращение (в нижнем регистре) → id сниппета
    abbrs: RwLock<HashMap<String, String>>,
    buffer: RwLock<String>,
    tx: mpsc::Sender<Match>,
}

static SHARED: OnceLock<Shared> = OnceLock::new();

pub fn configure(enabled: bool, prefix: &str, abbrs: HashMap<String, String>) {
    if let Some(s) = SHARED.get() {
        s.enabled.store(enabled && !prefix.is_empty() && !abbrs.is_empty(), Ordering::SeqCst);
        *s.prefix.write() = prefix.to_string();
        *s.abbrs.write() = abbrs.into_iter().map(|(k, v)| (k.to_lowercase(), v)).collect();
        s.buffer.write().clear();
    }
}

pub fn disable() {
    if let Some(s) = SHARED.get() {
        s.enabled.store(false, Ordering::SeqCst);
        s.abbrs.write().clear();
        s.buffer.write().clear();
    }
}

/// Запускает перехватчик. `on_match` вызывается в отдельном потоке.
pub fn start(on_match: impl Fn(Match) + Send + 'static) {
    let (tx, rx) = mpsc::channel::<Match>();
    let _ = SHARED.set(Shared {
        enabled: AtomicBool::new(false),
        prefix: RwLock::new(";".into()),
        abbrs: RwLock::new(HashMap::new()),
        buffer: RwLock::new(String::new()),
        tx,
    });
    std::thread::Builder::new()
        .name("expander-worker".into())
        .spawn(move || {
            while let Ok(m) = rx.recv() {
                on_match(m);
            }
        })
        .ok();
    imp::install();
}

/// Возвращает true, если нажатие нужно поглотить (сработало сокращение).
fn on_key(ch: Key) -> bool {
    let Some(s) = SHARED.get() else { return false };
    if !s.enabled.load(Ordering::Relaxed) {
        return false;
    }
    let mut buf = s.buffer.write();
    match ch {
        Key::Char(c) => {
            buf.push(c);
            if buf.chars().count() > 48 {
                let cut: String = buf.chars().skip(buf.chars().count() - 48).collect();
                *buf = cut;
            }
            false
        }
        Key::Back => {
            buf.pop();
            false
        }
        Key::Reset => {
            buf.clear();
            false
        }
        Key::Delimiter => {
            let typed = std::mem::take(&mut *buf);
            let prefix = s.prefix.read();
            let Some(abbr) = typed.strip_prefix(prefix.as_str()) else { return false };
            let Some(id) = s.abbrs.read().get(&abbr.to_lowercase()).cloned() else { return false };
            let _ = s.tx.send(Match { typed: typed.chars().count(), snippet_id: id });
            true
        }
    }
}

enum Key {
    Char(char),
    Back,
    Reset,
    Delimiter,
}

#[cfg(windows)]
mod imp {
    use super::{on_key, Key};
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    use windows::Win32::UI::WindowsAndMessaging::*;

    fn down(vk: VIRTUAL_KEY) -> bool {
        unsafe { GetAsyncKeyState(vk.0 as i32) as u16 & 0x8000 != 0 }
    }

    unsafe extern "system" fn hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 && (wparam.0 == WM_KEYDOWN as usize || wparam.0 == WM_SYSKEYDOWN as usize) {
            let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
            if kb.flags.0 & LLKHF_INJECTED.0 == 0 {
                let vk = VIRTUAL_KEY(kb.vkCode as u16);
                let ctrl = down(VK_CONTROL);
                let alt = down(VK_MENU);
                let win = down(VK_LWIN) || down(VK_RWIN);
                // AltGr приходит как Ctrl+Alt: это ввод символа, а не сочетание.
                let altgr = ctrl && alt && down(VK_RMENU);
                let key = if (ctrl || alt || win) && !altgr {
                    Some(Key::Reset)
                } else {
                    match vk {
                        VK_SPACE | VK_TAB | VK_RETURN => Some(Key::Delimiter),
                        VK_BACK => Some(Key::Back),
                        VK_SHIFT | VK_LSHIFT | VK_RSHIFT | VK_CAPITAL | VK_CONTROL | VK_LCONTROL | VK_RCONTROL | VK_MENU | VK_LMENU | VK_RMENU => None,
                        VK_ESCAPE | VK_LEFT | VK_RIGHT | VK_UP | VK_DOWN | VK_HOME | VK_END | VK_PRIOR | VK_NEXT | VK_DELETE | VK_INSERT => Some(Key::Reset),
                        _ => translate(kb.vkCode, kb.scanCode).map(Key::Char),
                    }
                };
                if let Some(k) = key {
                    if on_key(k) {
                        return LRESULT(1);
                    }
                }
            }
        }
        CallNextHookEx(None, code, wparam, lparam)
    }

    fn translate(vk: u32, scan: u32) -> Option<char> {
        unsafe {
            let mut state = [0u8; 256];
            if down(VK_SHIFT) {
                state[VK_SHIFT.0 as usize] = 0x80;
            }
            if GetKeyState(VK_CAPITAL.0 as i32) & 1 != 0 {
                state[VK_CAPITAL.0 as usize] = 0x01;
            }
            if down(VK_RMENU) {
                state[VK_CONTROL.0 as usize] = 0x80;
                state[VK_MENU.0 as usize] = 0x80;
            }
            let fg = GetForegroundWindow();
            let tid = GetWindowThreadProcessId(fg, None);
            let layout = GetKeyboardLayout(tid);
            let mut buf = [0u16; 4];
            // Флаг 0x4: не менять состояние «мёртвых» клавиш у активной программы.
            let n = ToUnicodeEx(vk, scan, &state, &mut buf, 0x4, Some(layout));
            if n == 1 {
                char::from_u32(buf[0] as u32).filter(|c| !c.is_control())
            } else {
                None
            }
        }
    }

    pub fn install() {
        std::thread::Builder::new()
            .name("keyboard-hook".into())
            .spawn(|| unsafe {
                let Ok(_h) = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook), None, 0) else { return };
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            })
            .ok();
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn install() {}
    #[allow(dead_code)]
    fn _use() {
        let _ = super::on_key(super::Key::Reset);
    }
}
