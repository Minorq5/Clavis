//! Работа с окнами и клавиатурой: где стоит курсор, куда вернуть фокус, как вставить.

#[derive(Clone, Copy, Debug, Default)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[cfg(windows)]
pub use imp::*;

#[cfg(windows)]
mod imp {
    use super::Point;
    use std::time::Duration;
    use windows::Win32::Foundation::{HWND, POINT};
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    use windows::Win32::UI::WindowsAndMessaging::*;

    pub fn foreground() -> isize {
        unsafe { GetForegroundWindow().0 as isize }
    }

    /// Положение текстовой каретки в активном окне, если программа его сообщает,
    /// иначе положение указателя мыши.
    pub fn anchor() -> Point {
        unsafe {
            let fg = GetForegroundWindow();
            let tid = GetWindowThreadProcessId(fg, None);
            let mut gui = GUITHREADINFO { cbSize: std::mem::size_of::<GUITHREADINFO>() as u32, ..Default::default() };
            if tid != 0 && GetGUIThreadInfo(tid, &mut gui).is_ok() && !gui.hwndCaret.0.is_null() {
                let mut p = POINT { x: gui.rcCaret.left, y: gui.rcCaret.bottom };
                if ClientToScreen(gui.hwndCaret, &mut p).as_bool() && (p.x != 0 || p.y != 0) {
                    return Point { x: p.x, y: p.y + 4 };
                }
            }
            let mut p = POINT::default();
            let _ = GetCursorPos(&mut p);
            Point { x: p.x, y: p.y + 12 }
        }
    }

    /// Возвращает фокус окну. Windows не даёт фоновым программам забирать фокус,
    /// поэтому на время присоединяемся к потоку ввода текущего окна.
    pub fn focus(hwnd: isize) {
        if hwnd == 0 {
            return;
        }
        unsafe {
            let target = HWND(hwnd as *mut _);
            if !IsWindow(Some(target)).as_bool() {
                return;
            }
            let fg = GetForegroundWindow();
            let fg_tid = GetWindowThreadProcessId(fg, None);
            let me = GetCurrentThreadId();
            let attached = fg_tid != 0 && fg_tid != me && AttachThreadInput(me, fg_tid, true).as_bool();
            if IsIconic(target).as_bool() {
                let _ = ShowWindow(target, SW_RESTORE);
            }
            let _ = SetForegroundWindow(target);
            if attached {
                let _ = AttachThreadInput(me, fg_tid, false);
            }
        }
    }

    fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, time: 0, dwExtraInfo: 0 },
            },
        }
    }

    fn send(inputs: &[INPUT]) {
        unsafe {
            SendInput(inputs, std::mem::size_of::<INPUT>() as i32);
        }
    }

    /// Отпускает модификаторы, которые человек мог ещё держать (Shift после Shift+Enter и т.п.).
    fn release_modifiers() {
        let mods = [VK_SHIFT, VK_LSHIFT, VK_RSHIFT, VK_CONTROL, VK_LCONTROL, VK_RCONTROL, VK_MENU, VK_LMENU, VK_RMENU, VK_LWIN, VK_RWIN];
        let ups: Vec<INPUT> = mods
            .iter()
            .filter(|vk| unsafe { GetAsyncKeyState(vk.0 as i32) } as u16 & 0x8000 != 0)
            .map(|vk| key(*vk, true))
            .collect();
        if !ups.is_empty() {
            send(&ups);
        }
    }

    pub fn press_paste() {
        release_modifiers();
        std::thread::sleep(Duration::from_millis(30));
        send(&[key(VK_CONTROL, false), key(VK_V, false), key(VK_V, true), key(VK_CONTROL, true)]);
    }

    pub fn backspaces(n: usize) {
        let mut v = Vec::with_capacity(n * 2);
        for _ in 0..n {
            v.push(key(VK_BACK, false));
            v.push(key(VK_BACK, true));
        }
        send(&v);
    }
}

#[cfg(not(windows))]
mod stub {
    use super::Point;
    pub fn foreground() -> isize { 0 }
    pub fn anchor() -> Point { Point::default() }
    pub fn focus(_: isize) {}
    pub fn press_paste() {}
    pub fn backspaces(_: usize) {}
}
#[cfg(not(windows))]
pub use stub::*;
