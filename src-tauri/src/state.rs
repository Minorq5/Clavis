//! Общее состояние приложения и фоновые обработчики.

use crate::clip::{self, Content};
use crate::license::{License, State as LicState};
use crate::settings::Settings;
use crate::vault::{self, ClipItem, Vault};
use crate::{device, expander, input, ui};
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicIsize, AtomicU32, Ordering};
use tauri::{AppHandle, Emitter, Manager};
use zeroize::Zeroizing;

pub struct AppState {
    pub dir: PathBuf,
    pub vault: Mutex<Option<Vault>>,
    pub device_secret: Zeroizing<Vec<u8>>,
    pub license: Mutex<License>,
    pub settings: RwLock<Settings>,
    pub prev_window: AtomicIsize,
    pub paused: AtomicBool,
    /// Номер изменения буфера после нашей собственной записи: его не нужно сохранять.
    pub own_seq: AtomicU32,
    pub last_activity: AtomicI64,
    pub picker_shown_at: AtomicI64,
    pub shared: crate::shared::Shared,
    /// Ключи открытых секретных заметок. Живут только в памяти и стираются при блокировке.
    pub secret_keys: Mutex<HashMap<String, crate::crypto::Key>>,
}

impl AppState {
    pub fn init(dir: PathBuf, shared: crate::shared::Shared) -> Result<AppState, String> {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let secret_path = dir.join("device.key");
        let device_secret = match std::fs::read(&secret_path).ok().and_then(|b| device::unprotect(&b).ok()) {
            Some(s) if s.len() == 32 => s,
            _ => {
                // Секрета нет или он защищён на другом компьютере: прежнее хранилище здесь не открыть.
                if secret_path.exists() {
                    let _ = std::fs::rename(&secret_path, dir.join(format!("device.key.old-{}", vault::now())));
                }
                let s = crate::crypto::random::<32>();
                let blob = device::protect(&s).map_err(|e| e.to_string())?;
                std::fs::write(&secret_path, blob).map_err(|e| e.to_string())?;
                Zeroizing::new(s.to_vec())
            }
        };
        let license = License::load(&dir).map_err(|e| e.to_string())?;
        let settings = Settings::load(&dir);
        Ok(AppState {
            dir,
            vault: Mutex::new(None),
            device_secret,
            license: Mutex::new(license),
            settings: RwLock::new(settings),
            prev_window: AtomicIsize::new(0),
            paused: AtomicBool::new(false),
            own_seq: AtomicU32::new(0),
            last_activity: AtomicI64::new(vault::now()),
            picker_shown_at: AtomicI64::new(0),
            shared,
            secret_keys: Mutex::new(HashMap::new()),
        })
    }

    pub fn touch(&self) {
        self.last_activity.store(vault::now(), Ordering::Relaxed);
    }

    pub fn stage(&self) -> &'static str {
        match self.license.lock().state() {
            LicState::Blocked | LicState::Revoked => return "blocked",
            LicState::NotActivated => return "activate",
            LicState::Active { .. } => {}
        }
        match vault::read_header(&self.dir) {
            None => "setup",
            Some(h) if !h.recovery_confirmed => "setup",
            Some(_) if self.vault.lock().is_none() => "locked",
            Some(_) => "ready",
        }
    }

    pub fn is_owner(&self) -> bool {
        matches!(self.license.lock().state(), LicState::Active { owner: true })
    }

    pub fn configure_expander(&self) {
        let (enabled, prefix) = {
            let s = self.settings.read();
            (s.expander.enabled, s.expander.prefix.clone())
        };
        let mut map: HashMap<String, String> = match self.vault.lock().as_ref() {
            Some(v) if enabled => v
                .data
                .snippets
                .iter()
                .filter(|sn| !sn.abbr.trim().is_empty())
                .map(|sn| (sn.abbr.trim().to_string(), sn.id.clone()))
                .collect(),
            _ => {
                expander::disable();
                return;
            }
        };
        // Командные сниппеты тоже работают как сокращения; свои важнее при совпадении.
        for t in crate::shared::team_snippets(self) {
            if !t.abbr.trim().is_empty() {
                map.entry(t.abbr.trim().to_string()).or_insert(format!("team:{}:{}", t.set_id, t.id));
            }
        }
        expander::configure(true, &prefix, map);
    }

    /// Текст сниппета по id (свой или командный вида team:<набор>:<id>).
    /// Отмечает, что сниппет вставили: для статистики в библиотеке.
    pub fn count_use(&self, id: &str) {
        let mut g = self.vault.lock();
        if let Some(v) = g.as_mut() {
            let u = v.data.snippet_uses.entry(id.to_string()).or_default();
            u.count += 1;
            u.last = vault::now();
            let _ = v.save();
        }
    }

    pub fn snippet_body(&self, id: &str) -> Option<String> {
        if let Some(rest) = id.strip_prefix("team:") {
            let (set, sid) = rest.split_once(':')?;
            return crate::shared::team_snippets(self).into_iter().find(|t| t.set_id == set && t.id == sid).map(|t| t.body);
        }
        let v = self.vault.lock();
        v.as_ref()?.data.snippets.iter().find(|s| s.id == id).map(|s| s.body.clone())
    }

    pub fn lock(&self, app: &AppHandle) {
        self.lock_with(app, "lock");
    }

    pub fn lock_with(&self, app: &AppHandle, reason: &str) {
        {
            let mut g = self.vault.lock();
            if let Some(v) = g.as_mut() {
                v.log_now(reason, "");
                let _ = v.save();
            }
            *g = None;
        }
        self.secret_keys.lock().clear();
        expander::disable();
        ui::hide_picker(app, false);
        let _ = app.emit("status:changed", ());
    }
}

/// Вызывается при каждом изменении буфера обмена.
pub fn on_clipboard_change(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.paused.load(Ordering::Relaxed) {
        return;
    }
    if !matches!(state.license.lock().state(), LicState::Active { .. }) {
        return;
    }
    let seq = clip::seq();
    if seq != 0 && seq == state.own_seq.load(Ordering::SeqCst) {
        return;
    }
    let (ignore_private, excluded, limit) = {
        let s = state.settings.read();
        (s.ignore_password_managers, s.excluded_apps.clone(), s.history_limit)
    };
    let Some(cap) = clip::read(ignore_private) else { return };
    if let Some(src) = &cap.source {
        if excluded.iter().any(|e| e.trim().eq_ignore_ascii_case(src)) {
            return;
        }
        // То, что скопировано внутри самого Clavis, в историю не попадает.
        if src.eq_ignore_ascii_case("clavis.exe") {
            return;
        }
    }

    let id = uuid::Uuid::new_v4().to_string();
    let mut item = ClipItem {
        id: id.clone(),
        kind: String::new(),
        text: None,
        html: None,
        rich: false,
        image: None,
        image_size: None,
        files: None,
        source: cap.source.map(|s| s.trim_end_matches(".exe").trim_end_matches(".EXE").to_string()),
        at: vault::now(),
        pinned: false,
    };
    let mut blob: Option<Vec<u8>> = None;
    match cap.content {
        Content::Text { text, html } => {
            item.kind = "text".into();
            item.rich = html.is_some();
            item.text = Some(text);
            item.html = html;
        }
        Content::Image { png, width, height, thumb } => {
            item.kind = "image".into();
            item.image = Some(thumb);
            item.image_size = Some([width, height]);
            blob = Some(png);
        }
        Content::Files(files) => {
            item.kind = "files".into();
            item.files = Some(files);
        }
    }

    let mut guard = state.vault.lock();
    if let Some(v) = guard.as_mut() {
        if let Some(b) = &blob {
            if v.put_blob(&id, b).is_err() {
                return;
            }
        }
        v.push_history(item, limit);
        let _ = v.save();
    } else if vault::read_header(&state.dir).is_some() {
        let _ = vault::write_inbox(&state.dir, item, blob.as_deref());
    }
    drop(guard);
    let _ = app.emit("history:changed", ());
}

/// Сработало сокращение: стираем его и вставляем текст сниппета.
pub fn on_expansion(app: &AppHandle, m: expander::Match) {
    let state = app.state::<AppState>();
    let Some(body) = state.snippet_body(&m.snippet_id) else { return };
    state.count_use(&m.snippet_id);
    std::thread::sleep(std::time::Duration::from_millis(20));
    input::backspaces(m.typed);

    if crate::template::asked(&body).is_empty() {
        let clipboard = clip::read_text().unwrap_or_default();
        let text = crate::template::fill(&body, &HashMap::new(), &clipboard);
        paste_preserving_clipboard(app, &text);
    } else {
        // Есть поля для заполнения: открываем форму, а вставим после неё.
        let handle = app.clone();
        let id = m.snippet_id.clone();
        let _ = app.run_on_main_thread(move || ui::show_picker_for(&handle, Some(id)));
    }
}

/// Вставляет текст через буфер и возвращает прежнее текстовое содержимое буфера.
fn paste_preserving_clipboard(app: &AppHandle, text: &str) {
    let state = app.state::<AppState>();
    let previous = clip::read_text();
    if !clip::write_text(text, None) {
        return;
    }
    state.own_seq.store(clip::seq(), Ordering::SeqCst);
    input::press_paste();
    std::thread::sleep(std::time::Duration::from_millis(250));
    if let Some(prev) = previous {
        if clip::write_text(&prev, None) {
            state.own_seq.store(clip::seq(), Ordering::SeqCst);
        }
    }
}

/// Следит за бездействием и блокирует хранилище.
pub fn start_idle_watch(app: AppHandle) {
    std::thread::Builder::new()
        .name("idle-lock".into())
        .spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(20));
            let state = app.state::<AppState>();
            let minutes = state.settings.read().lock_after_minutes;
            if minutes == 0 || state.vault.lock().is_none() {
                continue;
            }
            let idle = vault::now() - state.last_activity.load(Ordering::Relaxed);
            if idle > minutes as i64 * 60_000 {
                let handle = app.clone();
                let _ = app.run_on_main_thread(move || {
                    let s = handle.state::<AppState>();
                    s.lock_with(&handle, "lock_idle");
                });
            }
        })
        .ok();
}
