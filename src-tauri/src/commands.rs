//! Команды, которые вызывает интерфейс.

use crate::clip;
use crate::license::LicenseError;
use crate::settings::Settings;
use crate::state::AppState;
use crate::vault::{self, ClipItem, Folder, NoteMeta, Snippet, Vault, VaultError};
use crate::{input, ui};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

type R<T> = Result<T, String>;

fn locked<T>() -> R<T> {
    Err("locked".into())
}

fn net(e: LicenseError) -> String {
    match e {
        LicenseError::Network => "network".into(),
        LicenseError::Clock => "clock".into(),
        LicenseError::Server(s) => s,
        other => other.to_string(),
    }
}

/// Выполняет действие с открытым хранилищем и сохраняет его.
fn with_vault<T>(state: &AppState, f: impl FnOnce(&mut Vault) -> R<T>) -> R<T> {
    state.touch();
    let mut g = state.vault.lock();
    let Some(v) = g.as_mut() else { return locked() };
    let out = f(v)?;
    v.save().map_err(|e| e.to_string())?;
    Ok(out)
}

fn read_vault<T>(state: &AppState, f: impl FnOnce(&Vault) -> T) -> R<T> {
    state.touch();
    let g = state.vault.lock();
    match g.as_ref() {
        Some(v) => Ok(f(v)),
        None => locked(),
    }
}

// ——— состояние и доступ ———

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    stage: &'static str,
    owner: bool,
    attempts_left: u32,
}

#[tauri::command]
pub fn status(state: State<AppState>) -> Status {
    Status { stage: state.stage(), owner: state.is_owner(), attempts_left: state.license.lock().attempts_left() }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivateResult {
    ok: bool,
    attempts_left: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[tauri::command]
pub async fn activate(app: AppHandle, code: String) -> R<ActivateResult> {
    let code: String = code.chars().filter(|c| c.is_ascii_alphanumeric()).take(24).collect::<String>().to_uppercase();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mut lic = state.license.lock();
        let res = match lic.activate(&code) {
            Ok((ok, left)) => ActivateResult { ok, attempts_left: left, error: None },
            Err(e) => ActivateResult { ok: false, attempts_left: lic.attempts_left(), error: Some(net(e)) },
        };
        drop(lic);
        let _ = app.emit("status:changed", ());
        Ok(res)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn request_access(app: AppHandle, name: String, message: String) -> R<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let env = state.license.lock().access_request(name.trim(), message.trim()).map_err(net)?;
        crate::license::post(&env).map(|_| ()).map_err(net)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn valid_pin(pin: &str) -> bool {
    (4..=12).contains(&pin.len()) && pin.chars().all(|c| c.is_ascii_digit())
}

#[tauri::command]
pub async fn set_pin(app: AppHandle, pin: String) -> R<Vec<String>> {
    if !valid_pin(&pin) {
        return Err("pin".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        if state.stage() != "setup" {
            return Err("stage".into());
        }
        let (mut v, words) = Vault::create(&state.dir, &pin, &state.device_secret).map_err(|e| e.to_string())?;
        v.log_now("setup", &crate::device::machine_name());
        let _ = v.save();
        *state.vault.lock() = Some(v);
        Ok(words)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn confirm_recovery(app: AppHandle, state: State<AppState>) -> R<()> {
    {
        let g = state.vault.lock();
        let Some(v) = g.as_ref() else { return locked() };
        v.confirm_recovery().map_err(|e| e.to_string())?;
    }
    // Статус не рассылаем: окно настройки ещё покажет последний шаг и само сообщит о завершении.
    let _ = app;
    state.configure_expander();
    state.touch();
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockResult {
    ok: bool,
    attempts_left: u32,
}

#[tauri::command]
pub async fn unlock(app: AppHandle, pin: String) -> R<UnlockResult> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        match Vault::unlock(&state.dir, &pin, &state.device_secret) {
            Ok(mut v) => {
                v.log_now("unlock_pin", &crate::device::machine_name());
                let _ = v.save();
                *state.vault.lock() = Some(v);
                state.configure_expander();
                state.touch();
                let _ = app.emit("status:changed", ());
                let _ = app.emit("history:changed", ());
                Ok(UnlockResult { ok: true, attempts_left: vault::MAX_PIN_FAILURES })
            }
            Err((VaultError::WrongPin, left)) => {
                let _ = vault::write_journal_locked(&state.dir, "unlock_fail", "PIN");
                Ok(UnlockResult { ok: false, attempts_left: left })
            }
            Err((VaultError::PinDisabled, _)) => Ok(UnlockResult { ok: false, attempts_left: 0 }),
            Err((e, _)) => Err(e.to_string()),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn recover(app: AppHandle, words: Vec<String>, new_pin: String) -> R<Value> {
    if !valid_pin(&new_pin) {
        return Err("pin".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let words: Vec<String> = words.iter().map(|w| w.trim().to_lowercase()).filter(|w| !w.is_empty()).collect();
        match Vault::recover(&state.dir, &words, &new_pin, &state.device_secret) {
            Ok(mut v) => {
                v.log_now("recovery", &crate::device::machine_name());
                let _ = v.save();
                *state.vault.lock() = Some(v);
                state.configure_expander();
                state.touch();
                let _ = app.emit("status:changed", ());
                Ok(json!({ "ok": true }))
            }
            Err(VaultError::WrongPhrase) => Ok(json!({ "ok": false })),
            Err(e) => Err(e.to_string()),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn lock(app: AppHandle, state: State<AppState>) {
    state.lock(&app);
}

// ——— история ———

#[tauri::command]
pub fn history_list(state: State<AppState>) -> R<Vec<ClipItem>> {
    read_vault(&state, |v| {
        v.data
            .history
            .iter()
            .map(|h| ClipItem { html: None, ..h.clone() })
            .collect()
    })
}

#[tauri::command]
pub fn history_pin(state: State<AppState>, id: String, pinned: bool) -> R<()> {
    with_vault(&state, |v| {
        if let Some(h) = v.data.history.iter_mut().find(|h| h.id == id) {
            h.pinned = pinned;
        }
        Ok(())
    })
}

#[tauri::command]
pub fn history_delete(state: State<AppState>, id: String) -> R<()> {
    with_vault(&state, |v| {
        v.data.history.retain(|h| h.id != id);
        let _ = v.remove_blob(&id);
        Ok(())
    })
}

#[tauri::command]
pub fn history_clear(state: State<AppState>) -> R<()> {
    with_vault(&state, |v| {
        let ids: Vec<String> = v.data.history.iter().filter(|h| !h.pinned).map(|h| h.id.clone()).collect();
        v.data.history.retain(|h| h.pinned);
        for id in ids {
            let _ = v.remove_blob(&id);
        }
        Ok(())
    })
}

#[tauri::command]
pub fn history_pause(state: State<AppState>, paused: bool) {
    state.paused.store(paused, Ordering::Relaxed);
}

#[tauri::command]
pub fn history_paused(state: State<AppState>) -> bool {
    state.paused.load(Ordering::Relaxed)
}

// ——— вставка ———

fn finish_paste(app: &AppHandle, state: &AppState, paste: bool) {
    state.own_seq.store(clip::seq(), Ordering::SeqCst);
    ui::hide_picker(app, true);
    if paste {
        std::thread::sleep(std::time::Duration::from_millis(60));
        input::press_paste();
    }
}

#[tauri::command]
pub async fn paste_text(app: AppHandle, text: String, paste: bool) -> R<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        if !clip::write_text(&text, None) {
            return Err("clipboard".into());
        }
        finish_paste(&app, &state, paste);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Копирует текст в буфер, не закрывая окно и не вставляя (кнопки «Скопировать»).
#[tauri::command]
pub fn copy_text(state: State<AppState>, text: String) -> R<()> {
    if !clip::write_text(&text, None) {
        return Err("clipboard".into());
    }
    state.own_seq.store(clip::seq(), Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub async fn paste_item(app: AppHandle, id: String, plain: bool, paste: bool) -> R<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let (item, blob) = {
            let mut g = state.vault.lock();
            let Some(v) = g.as_mut() else { return locked() };
            let Some(pos) = v.data.history.iter().position(|h| h.id == id) else { return Err("missing".into()) };
            let mut item = v.data.history.remove(pos);
            item.at = vault::now();
            let blob = if item.kind == "image" { v.get_blob(&item.id).ok().map(|b| b.to_vec()) } else { None };
            v.data.history.insert(0, item.clone());
            let _ = v.save();
            (item, blob)
        };
        let ok = match item.kind.as_str() {
            "text" => clip::write_text(item.text.as_deref().unwrap_or(""), if plain { None } else { item.html.as_deref() }),
            "image" => blob.map(|b| clip::write_image(&b)).unwrap_or(false),
            "files" => clip::write_files(item.files.as_deref().unwrap_or(&[])),
            _ => false,
        };
        if !ok {
            return Err("clipboard".into());
        }
        finish_paste(&app, &state, paste);
        let _ = app.emit("history:changed", ());
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn hide_picker(app: AppHandle) {
    ui::hide_picker(&app, true);
}

#[tauri::command]
pub fn open_main(app: AppHandle, section: Option<String>) {
    ui::show_main(&app, section.as_deref());
}

// ——— сниппеты ———

#[tauri::command]
pub fn folders_list(state: State<AppState>) -> R<Vec<Folder>> {
    read_vault(&state, |v| v.data.folders.clone())
}

#[tauri::command]
pub fn folder_save(state: State<AppState>, folder: Folder) -> R<Folder> {
    let name: String = folder.name.trim().chars().take(60).collect();
    if name.is_empty() {
        return Err("name".into());
    }
    with_vault(&state, |v| {
        let f = Folder { id: if folder.id.is_empty() { uuid::Uuid::new_v4().to_string() } else { folder.id.clone() }, name };
        match v.data.folders.iter_mut().find(|x| x.id == f.id) {
            Some(x) => *x = f.clone(),
            None => v.data.folders.push(f.clone()),
        }
        Ok(f)
    })
}

#[tauri::command]
pub fn folder_delete(state: State<AppState>, id: String) -> R<()> {
    with_vault(&state, |v| {
        v.data.folders.retain(|f| f.id != id);
        for s in v.data.snippets.iter_mut().filter(|s| s.folder.as_deref() == Some(&id)) {
            s.folder = None;
        }
        Ok(())
    })
}

#[tauri::command]
pub fn snippets_list(state: State<AppState>) -> R<Vec<Snippet>> {
    read_vault(&state, |v| {
        let mut s = v.data.snippets.clone();
        s.sort_by(|a, b| b.updated.cmp(&a.updated));
        s
    })
}

fn normalize_snippet(mut s: Snippet) -> Snippet {
    s.title = s.title.chars().take(200).collect();
    s.abbr = s.abbr.trim().chars().filter(|c| !c.is_whitespace()).take(32).collect();
    s.tags = s.tags.into_iter().map(|t| t.trim().chars().take(40).collect::<String>()).filter(|t| !t.is_empty()).take(20).collect();
    s
}

#[tauri::command]
pub fn snippet_save(state: State<AppState>, snippet: Snippet) -> R<Snippet> {
    let mut s = normalize_snippet(snippet);
    let out = with_vault(&state, |v| {
        if s.id.is_empty() {
            s.id = uuid::Uuid::new_v4().to_string();
        }
        s.updated = vault::now();
        if let Some(f) = &s.folder {
            if !v.data.folders.iter().any(|x| &x.id == f) {
                s.folder = None;
            }
        }
        match v.data.snippets.iter_mut().find(|x| x.id == s.id) {
            Some(x) => *x = s.clone(),
            None => v.data.snippets.push(s.clone()),
        }
        Ok(s)
    })?;
    state.configure_expander();
    Ok(out)
}

#[tauri::command]
pub fn snippet_delete(state: State<AppState>, id: String) -> R<()> {
    with_vault(&state, |v| {
        v.data.snippets.retain(|s| s.id != id);
        Ok(())
    })?;
    state.configure_expander();
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct SnippetExport {
    format: String,
    folders: Vec<Folder>,
    snippets: Vec<Snippet>,
}

#[tauri::command]
pub async fn snippets_export(app: AppHandle) -> R<Option<String>> {
    let data = {
        let state = app.state::<AppState>();
        read_vault(&state, |v| SnippetExport { format: "clavis-snippets/1".into(), folders: v.data.folders.clone(), snippets: v.data.snippets.clone() })?
    };
    let Some(path) = app.dialog().file().add_filter("JSON", &["json"]).set_file_name("clavis-snippets.json").blocking_save_file() else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    std::fs::write(&path, serde_json::to_vec_pretty(&data).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(Some(path.display().to_string()))
}

#[tauri::command]
pub async fn snippets_import(app: AppHandle) -> R<Option<usize>> {
    let Some(path) = app.dialog().file().add_filter("JSON", &["json"]).blocking_pick_file() else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    let raw = std::fs::read(&path).map_err(|e| e.to_string())?;
    if raw.len() > 20 * 1024 * 1024 {
        return Err("too_large".into());
    }
    let data: SnippetExport = serde_json::from_slice(&raw).map_err(|_| "format".to_string())?;
    if !data.format.starts_with("clavis-snippets/") {
        return Err("format".into());
    }
    let state = app.state::<AppState>();
    let n = with_vault(&state, |v| {
        let mut map = std::collections::HashMap::new();
        for f in data.folders {
            let existing = v.data.folders.iter().find(|x| x.name == f.name).map(|x| x.id.clone());
            let id = existing.unwrap_or_else(|| {
                let nf = Folder { id: uuid::Uuid::new_v4().to_string(), name: f.name.chars().take(60).collect() };
                v.data.folders.push(nf.clone());
                nf.id
            });
            map.insert(f.id, id);
        }
        let mut n = 0;
        for s in data.snippets.into_iter().take(10_000) {
            if v.data.snippets.iter().any(|x| x.title == s.title && x.body == s.body) {
                continue;
            }
            let mut s = normalize_snippet(s);
            s.id = uuid::Uuid::new_v4().to_string();
            s.folder = s.folder.and_then(|f| map.get(&f).cloned());
            s.updated = vault::now();
            v.data.snippets.push(s);
            n += 1;
        }
        Ok(n)
    })?;
    state.configure_expander();
    Ok(Some(n))
}

// ——— заметки ———

#[derive(Serialize, Deserialize)]
pub struct NoteDto {
    id: String,
    title: String,
    #[serde(default)]
    doc: Value,
    #[serde(default)]
    updated: i64,
}

pub fn plain_text_pub(v: &Value, out: &mut String) {
    plain_text(v, out)
}

fn plain_text(v: &Value, out: &mut String) {
    if out.len() > 8000 {
        return;
    }
    if let Some(t) = v.get("text").and_then(Value::as_str) {
        out.push_str(t);
        out.push(' ');
    }
    if let Some(c) = v.get("content").and_then(Value::as_array) {
        for x in c {
            plain_text(x, out);
        }
    }
}

#[tauri::command]
pub fn notes_list(state: State<AppState>) -> R<Vec<NoteMeta>> {
    // Заметки, пролежавшие в корзине дольше 30 дней, удаляются окончательно.
    let expired: Vec<String> = read_vault(&state, |v| {
        let now = vault::now();
        v.data.notes.iter().filter(|n| n.trashed > 0 && now - n.trashed > TRASH_MS).map(|n| n.id.clone()).collect()
    })?;
    if !expired.is_empty() {
        with_vault(&state, |v| {
            for id in &expired {
                purge_note(v, id);
            }
            Ok(())
        })?;
    }
    read_vault(&state, |v| {
        let mut n = v.data.notes.clone();
        n.sort_by(|a, b| b.updated.cmp(&a.updated));
        n
    })
}

const TRASH_MS: i64 = 30 * 86_400_000;

fn purge_note(v: &mut Vault, id: &str) {
    v.data.notes.retain(|n| n.id != id);
    let _ = v.remove_note(id);
    v.drop_versions(id);
    v.data.reminders.retain(|r| r.note != id);
}

/// Содержимое заметки. Для секретной без открытого ключа возвращает только признак { secret: true }.
#[tauri::command]
pub fn note_get(state: State<AppState>, id: String) -> R<Value> {
    state.touch();
    let g = state.vault.lock();
    let Some(v) = g.as_ref() else { return locked() };
    let doc = v.get_note(&id).map_err(|e| e.to_string())?;
    if doc.get("secret").and_then(Value::as_bool) == Some(true) {
        return Ok(json!({ "secret": true }));
    }
    Ok(doc)
}

#[tauri::command]
pub fn note_save(state: State<AppState>, note: NoteDto, folder: Option<String>) -> R<NoteMeta> {
    let st: &AppState = &state;
    with_vault(st, |v| {
        let id = if note.id.is_empty() { uuid::Uuid::new_v4().to_string() } else { note.id.clone() };
        let prev = v.data.notes.iter().find(|x| x.id == id).cloned();
        let secret = prev.as_ref().map(|p| p.secret).unwrap_or(false);
        let mut text = String::new();
        if !secret {
            plain_text(&note.doc, &mut text);
        }
        let meta = NoteMeta {
            id: id.clone(),
            title: note.title.chars().take(200).collect(),
            updated: vault::now(),
            text: text.chars().take(4000).collect(),
            folder: prev.as_ref().and_then(|p| p.folder.clone()).or_else(|| folder.filter(|f| v.data.note_folders.iter().any(|x| &x.id == f))),
            tags: prev.as_ref().map(|p| p.tags.clone()).unwrap_or_default(),
            pinned: prev.as_ref().map(|p| p.pinned).unwrap_or(false),
            secret,
            trashed: prev.as_ref().map(|p| p.trashed).unwrap_or(0),
        };
        if secret {
            crate::extras::save_secret(st, v, &id, &note.doc)?;
        } else {
            v.put_note(&id, &note.doc).map_err(|e| e.to_string())?;
            // Версия не чаще раза в 10 минут.
            let last = v.data.versions.iter().filter(|x| x.note == id).map(|x| x.at).max().unwrap_or(0);
            if vault::now() - last >= 600_000 {
                if let Ok(bytes) = serde_json::to_vec(&note.doc) {
                    let _ = v.put_version(&id, &bytes);
                }
            }
        }
        match v.data.notes.iter_mut().find(|x| x.id == id) {
            Some(x) => *x = meta.clone(),
            None => v.data.notes.push(meta.clone()),
        }
        Ok(meta)
    })
}

#[tauri::command]
/// Переносит заметку в корзину. Оттуда её можно вернуть в течение 30 дней.
pub fn note_delete(state: State<AppState>, id: String) -> R<()> {
    with_vault(&state, |v| {
        let n = v.data.notes.iter_mut().find(|n| n.id == id).ok_or("missing")?;
        n.trashed = vault::now();
        n.pinned = false;
        v.data.reminders.retain(|r| r.note != id);
        Ok(())
    })
}

#[tauri::command]
pub fn note_restore(state: State<AppState>, id: String) -> R<NoteMeta> {
    with_vault(&state, |v| {
        let n = v.data.notes.iter_mut().find(|n| n.id == id).ok_or("missing")?;
        n.trashed = 0;
        Ok(n.clone())
    })
}

/// Удаляет заметку окончательно, вместе с её версиями и дедлайнами.
#[tauri::command]
pub fn note_purge(state: State<AppState>, id: String) -> R<()> {
    with_vault(&state, |v| {
        purge_note(v, &id);
        Ok(())
    })
}

#[tauri::command]
pub fn trash_empty(state: State<AppState>) -> R<()> {
    with_vault(&state, |v| {
        let ids: Vec<String> = v.data.notes.iter().filter(|n| n.trashed > 0).map(|n| n.id.clone()).collect();
        for id in ids {
            purge_note(v, &id);
        }
        Ok(())
    })
}

#[tauri::command]
pub async fn note_pick_image(app: AppHandle) -> R<Option<String>> {
    let Some(path) = app.dialog().file().add_filter("Изображения", &["png", "jpg", "jpeg", "gif", "webp", "bmp"]).blocking_pick_file() else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    if bytes.len() > 15 * 1024 * 1024 {
        return Err("too_large".into());
    }
    let img = image::load_from_memory(&bytes).map_err(|_| "format".to_string())?;
    // Крупные снимки уменьшаем: в заметке больше 2400 пикселей по ширине не нужно.
    let img = if img.width() > 2400 { img.resize(2400, 2400 * img.height() / img.width().max(1), image::imageops::FilterType::Lanczos3) } else { img };
    let png = clip::png_from_image(&img).ok_or("format")?;
    Ok(Some(format!("data:image/png;base64,{}", B64.encode(png))))
}

#[tauri::command]
pub async fn note_export_docx(app: AppHandle, name: String, base64: String) -> R<Option<String>> {
    let bytes = B64.decode(base64).map_err(|_| "format".to_string())?;
    let safe: String = name.chars().filter(|c| !"\\/:*?\"<>|".contains(*c)).take(80).collect();
    let file = format!("{}.docx", if safe.trim().is_empty() { "Заметка" } else { safe.trim() });
    let Some(path) = app.dialog().file().add_filter("Word", &["docx"]).set_file_name(&file).blocking_save_file() else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(Some(path.display().to_string()))
}

// ——— настройки ———

#[tauri::command]
pub fn settings_get(state: State<AppState>) -> Settings {
    state.settings.read().clone()
}

#[tauri::command]
pub fn settings_set(app: AppHandle, state: State<AppState>, settings: Settings) -> R<()> {
    let new = settings.sanitized();
    let old = state.settings.read().clone();
    if new.hotkey != old.hotkey || new.quick_hotkey != old.quick_hotkey {
        if let Err(e) = crate::register_hotkeys(&app, &new.hotkey, &new.quick_hotkey) {
            let _ = crate::register_hotkeys(&app, &old.hotkey, &old.quick_hotkey);
            return Err(e);
        }
    }
    if new.autostart != old.autostart {
        crate::set_autostart(&app, new.autostart);
    }
    new.save(&state.dir);
    let limit = new.history_limit;
    *state.settings.write() = new;
    {
        let mut g = state.vault.lock();
        if let Some(v) = g.as_mut() {
            v.trim_history(limit);
            let _ = v.save();
        }
    }
    state.configure_expander();
    let _ = app.emit("settings:changed", ());
    Ok(())
}

// ——— владелец ———

fn owner_call(app: &AppHandle, action: &str, args: Value) -> R<Value> {
    let state = app.state::<AppState>();
    let env = state.license.lock().owner_request(action, args).map_err(net)?;
    crate::license::post(&env).map_err(net)
}

macro_rules! owner_cmd {
    ($name:ident, $action:literal $(, $arg:ident : $ty:ty)*) => {
        #[tauri::command]
        pub async fn $name(app: AppHandle $(, $arg: $ty)*) -> R<Value> {
            tauri::async_runtime::spawn_blocking(move || owner_call(&app, $action, json!({ $(stringify!($arg): $arg),* })))
                .await
                .map_err(|e| e.to_string())?
        }
    };
}

owner_cmd!(owner_requests, "requests");
owner_cmd!(owner_codes, "codes");
owner_cmd!(owner_devices, "devices");
owner_cmd!(owner_decline, "decline", id: String);
owner_cmd!(owner_revoke_code, "revoke_code", id: String);
owner_cmd!(owner_revoke_device, "revoke_device", id: String);
owner_cmd!(owner_unblock, "unblock", id: String);

#[tauri::command]
pub async fn owner_issue_code(app: AppHandle, label: String, request_id: Option<String>) -> R<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let v = owner_call(&app, "issue", json!({ "label": label, "requestId": request_id }))?;
        v.get("code").and_then(Value::as_str).map(String::from).ok_or_else(|| "server".to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

// ——— общие заметки ———

macro_rules! blocking {
    ($app:ident, $body:expr) => {
        tauri::async_runtime::spawn_blocking(move || {
            let $app = $app;
            $body
        })
        .await
        .map_err(|e| e.to_string())?
    };
}

#[tauri::command]
pub async fn shared_list(app: AppHandle) -> R<Vec<crate::shared::SharedMeta>> {
    blocking!(app, crate::shared::list(&app))
}

#[tauri::command]
pub async fn shared_create(app: AppHandle, title: String, kind: Option<String>) -> R<crate::shared::SharedMeta> {
    blocking!(app, crate::shared::create(&app, &title, kind.as_deref().unwrap_or("note")))
}

#[tauri::command]
pub async fn shared_open(app: AppHandle, id: String) -> R<crate::shared::Opened> {
    blocking!(app, crate::shared::open(&app, &id))
}

#[tauri::command]
pub fn shared_close(app: AppHandle, id: String) {
    crate::shared::close(&app, &id);
}

#[tauri::command]
pub fn shared_push(app: AppHandle, state: State<AppState>, id: String, update: String) -> R<()> {
    state.touch();
    crate::shared::push(&app, &id, &update)
}

#[tauri::command]
pub fn shared_awareness(app: AppHandle, id: String, data: String) -> R<()> {
    crate::shared::awareness(&app, &id, &data)
}

#[tauri::command]
pub async fn shared_title(app: AppHandle, id: String, title: String) -> R<()> {
    blocking!(app, crate::shared::set_title(&app, &id, &title))
}

#[tauri::command]
pub async fn shared_invite(app: AppHandle, id: String) -> R<String> {
    blocking!(app, crate::shared::invite(&app, &id))
}

#[tauri::command]
pub async fn shared_join(app: AppHandle, link: String) -> R<String> {
    blocking!(app, crate::shared::join(&app, &link))
}

#[tauri::command]
pub async fn shared_members(app: AppHandle, id: String) -> R<Vec<crate::shared::Member>> {
    blocking!(app, crate::shared::members(&app, &id))
}

#[tauri::command]
pub async fn shared_remove(app: AppHandle, id: String, device: String) -> R<()> {
    blocking!(app, crate::shared::remove_member(&app, &id, &device))
}

#[tauri::command]
pub async fn shared_leave(app: AppHandle, id: String) -> R<()> {
    blocking!(app, crate::shared::leave(&app, &id))
}

#[tauri::command]
pub async fn shared_image_put(app: AppHandle, id: String, data_url: String) -> R<String> {
    blocking!(app, crate::shared::image_put(&app, &id, &data_url))
}

#[tauri::command]
pub async fn shared_image_get(app: AppHandle, id: String, blob: String) -> R<String> {
    blocking!(app, crate::shared::image_get(&app, &id, &blob))
}

#[tauri::command]
pub fn shared_online(app: AppHandle) -> bool {
    crate::shared::online(&app)
}
