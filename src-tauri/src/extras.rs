//! Команды версии 0.3: журнал, Windows Hello, распознавание текста, внешние службы,
//! история версий, напоминания, секретные заметки, папки и теги заметок, шаблоны,
//! командные сниппеты, экспорт в PDF и Markdown.

use crate::clip;
use crate::state::AppState;
use crate::vault::{self, Folder, JournalEntry, NoteMeta, Reminder, Template, Vault, VersionMeta};
use crate::{input, ui, winrt};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

type R<T> = Result<T, String>;

fn with_vault<T>(state: &AppState, f: impl FnOnce(&mut Vault) -> R<T>) -> R<T> {
    state.touch();
    let mut g = state.vault.lock();
    let Some(v) = g.as_mut() else { return Err("locked".into()) };
    let out = f(v)?;
    v.save().map_err(|e| e.to_string())?;
    Ok(out)
}

fn read<T>(state: &AppState, f: impl FnOnce(&Vault) -> T) -> R<T> {
    let g = state.vault.lock();
    g.as_ref().map(f).ok_or_else(|| "locked".into())
}

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

// ——— журнал ———

#[tauri::command]
pub fn journal_list(state: State<AppState>) -> R<Vec<JournalEntry>> {
    read(&state, |v| {
        let mut j = v.data.journal.clone();
        j.reverse();
        j
    })
}

// ——— Windows Hello ———

fn hello_challenge(state: &AppState) -> Vec<u8> {
    let pk = vault::read_header(&state.dir).map(|h| h.inbox_pk).unwrap_or_default();
    format!("clavis/hello/v1/{pk}").into_bytes()
}

#[derive(Serialize)]
pub struct HelloStatus {
    available: bool,
    enabled: bool,
}

#[tauri::command]
pub async fn hello_status(app: AppHandle) -> R<HelloStatus> {
    blocking!(app, {
        let state = app.state::<AppState>();
        let enabled = vault::read_header(&state.dir).map(|h| h.hello_wrap.is_some()).unwrap_or(false);
        Ok(HelloStatus { available: winrt::hello_available(), enabled })
    })
}

#[tauri::command]
pub async fn hello_enable(app: AppHandle) -> R<()> {
    blocking!(app, {
        let state = app.state::<AppState>();
        let challenge = hello_challenge(&state);
        let sig = winrt::hello_sign(&challenge, true).map_err(|e| e.to_string())?;
        // Проверяем, что подпись воспроизводится: иначе по ней нельзя будет войти.
        let again = winrt::hello_sign(&challenge, false).map_err(|e| e.to_string())?;
        if sig != again {
            winrt::hello_remove();
            return Err("hello_unstable".into());
        }
        with_vault(&state, |v| {
            v.enable_hello(&sig, &state.device_secret).map_err(|e| e.to_string())?;
            v.log_now("hello_on", "");
            Ok(())
        })
    })
}

#[tauri::command]
pub fn hello_disable(state: State<AppState>) -> R<()> {
    with_vault(&state, |v| {
        v.disable_hello().map_err(|e| e.to_string())?;
        v.log_now("hello_off", "");
        Ok(())
    })?;
    winrt::hello_remove();
    Ok(())
}

#[tauri::command]
pub async fn unlock_hello(app: AppHandle) -> R<bool> {
    blocking!(app, {
        let state = app.state::<AppState>();
        let sig = match winrt::hello_sign(&hello_challenge(&state), false) {
            Ok(s) => s,
            Err(winrt::WinRtError::Cancelled) => return Ok(false),
            Err(e) => return Err(e.to_string()),
        };
        match Vault::unlock_hello(&state.dir, &sig, &state.device_secret) {
            Ok(mut v) => {
                v.log_now("unlock_hello", &crate::device::machine_name());
                let _ = v.save();
                *state.vault.lock() = Some(v);
                state.configure_expander();
                state.touch();
                let _ = app.emit("status:changed", ());
                let _ = app.emit("history:changed", ());
                Ok(true)
            }
            Err(_) => {
                let _ = vault::write_journal_locked(&state.dir, "unlock_fail", "Windows Hello");
                Ok(false)
            }
        }
    })
}

// ——— распознавание текста и вставка изображений ———

#[tauri::command]
pub async fn ocr_item(app: AppHandle, id: String) -> R<String> {
    blocking!(app, {
        let state = app.state::<AppState>();
        let png = read(&state, |v| v.get_blob(&id).map(|b| b.to_vec()))?.map_err(|_| "missing".to_string())?;
        winrt::ocr(&png).map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub async fn paste_image(app: AppHandle, data_url: String, paste: bool) -> R<()> {
    blocking!(app, {
        let state = app.state::<AppState>();
        let (_, b64) = data_url.split_once(";base64,").ok_or("format")?;
        let bytes = B64.decode(b64).map_err(|_| "format".to_string())?;
        let img = image::load_from_memory(&bytes).map_err(|_| "format".to_string())?;
        let png = clip::png_from_image(&img).ok_or("format")?;
        if !clip::write_image(&png) {
            return Err("clipboard".into());
        }
        state.own_seq.store(clip::seq(), Ordering::SeqCst);
        ui::hide_picker(&app, true);
        if paste {
            std::thread::sleep(Duration::from_millis(60));
            input::press_paste();
        }
        Ok(())
    })
}

// ——— внешние службы (с разрешения) ———

#[tauri::command]
pub fn consent_give(state: State<AppState>, service: String) -> R<()> {
    if !matches!(service.as_str(), "translate" | "shorten") {
        return Err("unknown".into());
    }
    with_vault(&state, |v| {
        if !v.data.consents.contains(&service) {
            v.data.consents.push(service);
        }
        Ok(())
    })
}

#[tauri::command]
pub fn consent_revoke(state: State<AppState>, service: String) -> R<()> {
    with_vault(&state, |v| {
        v.data.consents.retain(|s| s != &service);
        Ok(())
    })
}

#[tauri::command]
pub fn consents(state: State<AppState>) -> R<Vec<String>> {
    read(&state, |v| v.data.consents.clone())
}

fn require_consent(state: &AppState, service: &str) -> R<()> {
    if read(state, |v| v.data.consents.iter().any(|s| s == service))? {
        Ok(())
    } else {
        Err(format!("consent:{service}"))
    }
}

fn http() -> ureq::Agent {
    let mut b = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(5)).timeout(Duration::from_secs(20)).user_agent("Clavis");
    if let Ok(t) = native_tls::TlsConnector::new() {
        b = b.tls_connector(std::sync::Arc::new(t));
    }
    b.build()
}

/// Делит текст на части не длиннее `max` байт по границам предложений и строк.
fn chunks(text: &str, max: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for piece in text.split_inclusive(|c: char| matches!(c, '.' | '!' | '?' | '\n')) {
        if cur.len() + piece.len() > max && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        if piece.len() > max {
            // Очень длинное предложение режем по словам.
            for w in piece.split_inclusive(' ') {
                if cur.len() + w.len() > max && !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                cur.push_str(w);
            }
        } else {
            cur.push_str(piece);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn detect_lang(text: &str) -> &'static str {
    let cyr = text.chars().filter(|c| ('а'..='я').contains(&c.to_lowercase().next().unwrap_or(*c)) || *c == 'ё' || *c == 'Ё').count();
    let lat = text.chars().filter(|c| c.is_ascii_alphabetic()).count();
    if cyr > lat {
        // Болгарский отличается от русского буквами и частыми словами.
        let lower = text.to_lowercase();
        let bg = ["ъ ", " и ", "щ", " на ", " се ", " не "].iter().filter(|w| lower.contains(*w)).count();
        let ru = ["ы", "э", "ё", " не ", " это "].iter().filter(|w| lower.contains(*w)).count();
        if bg > ru && !lower.contains('ы') && !lower.contains('э') {
            "bg"
        } else {
            "ru"
        }
    } else {
        "en"
    }
}

#[tauri::command]
pub async fn translate(app: AppHandle, text: String, to: String) -> R<String> {
    blocking!(app, {
        let state = app.state::<AppState>();
        require_consent(&state, "translate")?;
        if !matches!(to.as_str(), "ru" | "en" | "bg") {
            return Err("lang".into());
        }
        let from = detect_lang(&text);
        if from == to {
            return Ok(text);
        }
        if text.len() > 20_000 {
            return Err("too_large".into());
        }
        let agent = http();
        let mut out = String::new();
        for part in chunks(&text, 450) {
            let r = agent
                .get("https://api.mymemory.translated.net/get")
                .query("q", &part)
                .query("langpair", &format!("{from}|{to}"))
                .call()
                .map_err(|_| "network".to_string())?;
            let v: Value = r.into_json().map_err(|_| "network".to_string())?;
            let t = v.pointer("/responseData/translatedText").and_then(Value::as_str).ok_or("network")?;
            if v.get("responseStatus").and_then(Value::as_i64).unwrap_or(200) != 200 {
                return Err("limit".into());
            }
            out.push_str(t);
            if part.ends_with('\n') && !t.ends_with('\n') {
                out.push('\n');
            } else if !out.ends_with(char::is_whitespace) {
                out.push(' ');
            }
        }
        Ok(out.trim_end().to_string())
    })
}

#[tauri::command]
pub async fn shorten(app: AppHandle, url: String) -> R<String> {
    blocking!(app, {
        let state = app.state::<AppState>();
        require_consent(&state, "shorten")?;
        let url = url.trim().to_string();
        if !(url.starts_with("http://") || url.starts_with("https://")) || url.len() > 4000 {
            return Err("format".into());
        }
        let r = http().get("https://clck.ru/--").query("url", &url).call().map_err(|_| "network".to_string())?;
        let short = r.into_string().map_err(|_| "network".to_string())?.trim().to_string();
        if short.starts_with("https://clck.ru/") {
            Ok(short)
        } else {
            Err("network".into())
        }
    })
}

// ——— заметки: папки, теги, закрепление, секретность ———

#[tauri::command]
pub fn note_folders(state: State<AppState>) -> R<Vec<Folder>> {
    read(&state, |v| v.data.note_folders.clone())
}

#[tauri::command]
pub fn note_folder_save(state: State<AppState>, folder: Folder) -> R<Folder> {
    let name: String = folder.name.trim().chars().take(60).collect();
    if name.is_empty() {
        return Err("name".into());
    }
    with_vault(&state, |v| {
        let f = Folder { id: if folder.id.is_empty() { uuid::Uuid::new_v4().to_string() } else { folder.id }, name };
        match v.data.note_folders.iter_mut().find(|x| x.id == f.id) {
            Some(x) => *x = f.clone(),
            None => v.data.note_folders.push(f.clone()),
        }
        Ok(f)
    })
}

#[tauri::command]
pub fn note_folder_delete(state: State<AppState>, id: String) -> R<()> {
    with_vault(&state, |v| {
        v.data.note_folders.retain(|f| f.id != id);
        for n in v.data.notes.iter_mut().filter(|n| n.folder.as_deref() == Some(&id)) {
            n.folder = None;
        }
        Ok(())
    })
}

#[tauri::command]
pub fn note_meta_set(state: State<AppState>, id: String, folder: Option<String>, tags: Vec<String>, pinned: bool) -> R<NoteMeta> {
    with_vault(&state, |v| {
        let folder = folder.filter(|f| v.data.note_folders.iter().any(|x| &x.id == f));
        let n = v.data.notes.iter_mut().find(|n| n.id == id).ok_or("missing")?;
        n.folder = folder;
        n.tags = tags.into_iter().map(|t| t.trim().trim_start_matches('#').chars().take(40).collect::<String>()).filter(|t| !t.is_empty()).take(20).collect();
        n.pinned = pinned;
        Ok(n.clone())
    })
}

#[tauri::command]
pub fn shared_pin(app: AppHandle, id: String, pinned: bool) -> R<()> {
    crate::shared::set_pinned(&app, &id, pinned)
}

#[derive(Serialize, Deserialize)]
struct SecretEnvelope {
    secret: bool,
    salt: String,
    ct: String,
}

fn seal_secret(key: &[u8; 32], id: &str, doc: &Value) -> R<String> {
    let plain = zeroize::Zeroizing::new(serde_json::to_vec(doc).map_err(|e| e.to_string())?);
    Ok(B64.encode(crate::crypto::seal(key, format!("secret:{id}").as_bytes(), &plain)))
}

/// Открывает секретную заметку паролем. Ключ остаётся в памяти до блокировки.
#[tauri::command]
pub async fn note_unlock(app: AppHandle, id: String, password: String) -> R<Value> {
    blocking!(app, {
        let state = app.state::<AppState>();
        let stored = read(&state, |v| v.get_note(&id))?.map_err(|e| e.to_string())?;
        let env: SecretEnvelope = serde_json::from_value(stored).map_err(|_| "not_secret".to_string())?;
        let salt = B64.decode(&env.salt).map_err(|_| "format".to_string())?;
        let key = vault::secret_key(&password, &salt).map_err(|e| e.to_string())?;
        let ct = B64.decode(&env.ct).map_err(|_| "format".to_string())?;
        let plain = crate::crypto::open(&key, format!("secret:{id}").as_bytes(), &ct).map_err(|_| "wrong_password".to_string())?;
        let doc: Value = serde_json::from_slice(&plain).map_err(|_| "format".to_string())?;
        state.secret_keys.lock().insert(id, key);
        Ok(doc)
    })
}

/// Сохраняет содержимое секретной заметки, зашифровав его её ключом.
pub fn save_secret(state: &AppState, v: &Vault, id: &str, doc: &Value) -> R<()> {
    let keys = state.secret_keys.lock();
    let key = keys.get(id).ok_or("secret_locked")?;
    let stored = v.get_note(id).map_err(|e| e.to_string())?;
    let salt = stored.get("salt").and_then(Value::as_str).ok_or("format")?.to_string();
    let ct = seal_secret(key, id, doc)?;
    v.put_note(id, &json!({ "secret": true, "salt": salt, "ct": ct })).map_err(|e| e.to_string())
}

/// Делает заметку секретной (пароль задан) или обычной (пароль пустой).
#[tauri::command]
pub async fn note_set_secret(app: AppHandle, id: String, password: Option<String>, doc: Value) -> R<NoteMeta> {
    blocking!(app, {
        let state = app.state::<AppState>();
        match password.filter(|p| !p.is_empty()) {
            Some(pw) => {
                if pw.chars().count() < 4 {
                    return Err("password_short".into());
                }
                let salt = crate::crypto::random::<16>();
                let key = vault::secret_key(&pw, &salt).map_err(|e| e.to_string())?;
                let ct = seal_secret(&key, &id, &doc)?;
                let meta = with_vault(&state, |v| {
                    v.put_note(&id, &json!({ "secret": true, "salt": B64.encode(salt), "ct": ct })).map_err(|e| e.to_string())?;
                    // Прежние версии хранили открытый текст: удаляем их.
                    v.drop_versions(&id);
                    let n = v.data.notes.iter_mut().find(|n| n.id == id).ok_or("missing")?;
                    n.secret = true;
                    n.text.clear();
                    Ok(n.clone())
                })?;
                state.secret_keys.lock().insert(id, key);
                Ok(meta)
            }
            None => {
                if !state.secret_keys.lock().contains_key(&id) {
                    return Err("secret_locked".into());
                }
                let meta = with_vault(&state, |v| {
                    v.put_note(&id, &doc).map_err(|e| e.to_string())?;
                    let mut text = String::new();
                    crate::commands::plain_text_pub(&doc, &mut text);
                    let n = v.data.notes.iter_mut().find(|n| n.id == id).ok_or("missing")?;
                    n.secret = false;
                    n.text = text.chars().take(4000).collect();
                    Ok(n.clone())
                })?;
                state.secret_keys.lock().remove(&id);
                Ok(meta)
            }
        }
    })
}

#[tauri::command]
pub fn note_lock_secret(state: State<AppState>, id: String) {
    state.secret_keys.lock().remove(&id);
}

// ——— история версий ———

#[tauri::command]
pub fn versions_list(state: State<AppState>, note: String) -> R<Vec<VersionMeta>> {
    read(&state, |v| {
        let mut l: Vec<VersionMeta> = v.data.versions.iter().filter(|x| x.note == note).cloned().collect();
        l.sort_by(|a, b| b.at.cmp(&a.at));
        l
    })
}

/// Содержимое версии: JSON документа для личной заметки, состояние Yjs (base64) для общей.
#[tauri::command]
pub fn version_get(state: State<AppState>, id: String) -> R<Value> {
    let bytes = read(&state, |v| v.get_blob(&id).map(|b| b.to_vec()))?.map_err(|e| e.to_string())?;
    match serde_json::from_slice::<Value>(&bytes) {
        Ok(doc) if doc.get("type").is_some() => Ok(json!({ "kind": "json", "doc": doc })),
        _ => Ok(json!({ "kind": "yjs", "state": B64.encode(bytes) })),
    }
}

// ——— напоминания ———

#[derive(Deserialize)]
pub struct ReminderIn {
    text: String,
    at: i64,
}

#[tauri::command]
pub fn reminders_set(app: AppHandle, state: State<AppState>, note: String, items: Vec<ReminderIn>) -> R<()> {
    with_vault(&state, |v| {
        let old: Vec<Reminder> = v.data.reminders.iter().filter(|r| r.note == note).cloned().collect();
        v.data.reminders.retain(|r| r.note != note);
        for it in items.into_iter().take(200) {
            let text: String = it.text.trim().chars().take(200).collect();
            let fired = old.iter().any(|o| o.at == it.at && o.text == text && o.fired);
            v.data.reminders.push(Reminder { id: uuid::Uuid::new_v4().to_string(), note: note.clone(), text, at: it.at, fired });
        }
        Ok(())
    })?;
    let _ = app.emit("reminders:changed", ());
    Ok(())
}

#[tauri::command]
pub fn reminders_list(state: State<AppState>) -> R<Vec<Reminder>> {
    read(&state, |v| {
        let mut l = v.data.reminders.clone();
        l.sort_by_key(|r| r.at);
        l
    })
}

/// Раз в 20 секунд проверяет дедлайны и показывает уведомление Windows.
pub fn start_reminders(app: AppHandle) {
    std::thread::Builder::new()
        .name("reminders".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_secs(20));
            let Some(state) = app.try_state::<AppState>() else { continue };
            let due: Vec<Reminder> = {
                let mut g = state.vault.lock();
                let Some(v) = g.as_mut() else { continue };
                let now = vault::now();
                let mut out = Vec::new();
                for r in v.data.reminders.iter_mut().filter(|r| !r.fired && r.at <= now) {
                    r.fired = true;
                    // Совсем старые дедлайны (больше суток) молча отмечаем, не беспокоя.
                    if now - r.at < 86_400_000 {
                        out.push(r.clone());
                    }
                }
                if !out.is_empty() || v.data.reminders.iter().any(|r| r.fired) {
                    let _ = v.save();
                }
                out
            };
            if due.is_empty() {
                continue;
            }
            let ru = state.settings.read().lang == "ru";
            for r in &due {
                let title = if ru { "Дедлайн" } else { "Deadline" };
                notify(&app, title, &r.text, Some(r.note.clone()));
            }
            let _ = app.emit("reminders:changed", ());
        })
        .ok();
}

// ——— шаблоны ———

#[tauri::command]
pub fn templates_list(state: State<AppState>) -> R<Vec<Template>> {
    read(&state, |v| v.data.templates.clone())
}

#[tauri::command]
pub fn template_save(state: State<AppState>, title: String, doc: Value) -> R<Template> {
    let title: String = title.trim().chars().take(80).collect();
    if title.is_empty() {
        return Err("name".into());
    }
    with_vault(&state, |v| {
        let t = Template { id: uuid::Uuid::new_v4().to_string(), title, doc };
        v.data.templates.push(t.clone());
        Ok(t)
    })
}

#[tauri::command]
pub fn template_delete(state: State<AppState>, id: String) -> R<()> {
    with_vault(&state, |v| {
        v.data.templates.retain(|t| t.id != id);
        Ok(())
    })
}

// ——— командные сниппеты ———

#[tauri::command]
pub fn team_snippets(state: State<AppState>) -> Vec<crate::shared::TeamSnippet> {
    crate::shared::team_snippets(&state)
}

// ——— экспорт ———

fn safe_name(name: &str, ext: &str, fallback: &str) -> String {
    let n: String = name.chars().filter(|c| !"\\/:*?\"<>|".contains(*c)).take(80).collect();
    format!("{}.{ext}", if n.trim().is_empty() { fallback } else { n.trim() })
}

#[tauri::command]
pub async fn export_text(app: AppHandle, name: String, ext: String, content: String) -> R<Option<String>> {
    if !matches!(ext.as_str(), "md" | "txt") {
        return Err("format".into());
    }
    let file = safe_name(&name, &ext, "Заметка");
    let filter = if ext == "md" { "Markdown" } else { "Text" };
    let Some(path) = app.dialog().file().add_filter(filter, &[ext.as_str()]).set_file_name(&file).blocking_save_file() else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(Some(path.display().to_string()))
}

#[tauri::command]
pub async fn export_pdf(app: AppHandle, name: String, html: String) -> R<Option<String>> {
    let file = safe_name(&name, "pdf", "Заметка");
    let Some(path) = app.dialog().file().add_filter("PDF", &["pdf"]).set_file_name(&file).blocking_save_file() else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    let shown = path.display().to_string();
    let title = name.clone();
    blocking!(app, crate::pdf::print_to_pdf(&app, html, title, path))?;
    Ok(Some(shown))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunking_keeps_text() {
        let t = "Первое предложение. Второе! Третье?\\nЧетвёртое ".repeat(40);
        let parts = chunks(&t, 450);
        assert!(parts.iter().all(|p| p.len() <= 450));
        assert_eq!(parts.concat(), t);
    }

    #[test]
    fn languages() {
        assert_eq!(detect_lang("Привет, как дела?"), "ru");
        assert_eq!(detect_lang("Hello there"), "en");
        assert_eq!(detect_lang("Къде е библиотеката? Аз съм студент и уча право."), "bg");
    }
}

// ——— право на удаление ———

/// Стирает данные устройства на сервере и все данные Clavis на этом компьютере, затем перезапускает приложение.
#[tauri::command]
pub async fn erase_everything(app: AppHandle) -> R<()> {
    blocking!(app, {
        let state = app.state::<AppState>();
        if state.is_owner() {
            return Err("owner".into());
        }
        let env = state.license.lock().signed("erase_me", json!({})).map_err(|e| e.to_string())?;
        crate::license::post(&env).map_err(|_| "network".to_string())?;
        *state.vault.lock() = None;
        state.secret_keys.lock().clear();
        let _ = std::fs::remove_dir_all(&state.dir);
        app.restart();
    })
}

// ——— статистика сниппетов ———

#[tauri::command]
pub fn snippet_used(state: State<AppState>, id: String) {
    state.count_use(&id);
}

#[tauri::command]
pub fn snippet_uses(state: State<AppState>) -> R<std::collections::BTreeMap<String, vault::SnippetUse>> {
    read(&state, |v| v.data.snippet_uses.clone())
}

// ——— изображение из истории в заметку ———

/// Полное изображение записи истории как data URL, чтобы вставить его в заметку.
#[tauri::command]
pub async fn history_image(app: AppHandle, id: String) -> R<String> {
    blocking!(app, {
        let state = app.state::<AppState>();
        let png = read(&state, |v| v.get_blob(&id).map(|b| b.to_vec()))?.map_err(|_| "missing".to_string())?;
        Ok(format!("data:image/png;base64,{}", B64.encode(png)))
    })
}

// ——— резервная копия ———

const BACKUP_MAGIC: &[u8] = b"CLAVISBK1";

/// Сохраняет все данные одним файлом, зашифрованным отдельным паролем.
#[tauri::command]
pub async fn backup_export(app: AppHandle, password: String) -> R<Option<String>> {
    if password.chars().count() < 8 {
        return Err("password_short".into());
    }
    let state = app.state::<AppState>();
    let bundle = read(&state, |v| v.export_bundle())?.map_err(|e| e.to_string())?;
    let date = chrono_like_date();
    let Some(path) = app.dialog().file().add_filter("Clavis", &["clavisbak"]).set_file_name(&format!("Clavis {date}.clavisbak")).blocking_save_file() else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    blocking!(app, {
        let plain = zeroize::Zeroizing::new(serde_json::to_vec(&bundle).map_err(|e| e.to_string())?);
        let salt = crate::crypto::random::<16>();
        let key = vault::secret_key(&password, &salt).map_err(|e| e.to_string())?;
        let mut out = BACKUP_MAGIC.to_vec();
        out.extend_from_slice(&salt);
        out.extend(crate::crypto::seal(&key, BACKUP_MAGIC, &plain));
        std::fs::write(&path, out).map_err(|e| e.to_string())?;
        let _ = with_vault(&app.state::<AppState>(), |v| { v.log_now("backup_export", ""); Ok(()) });
        Ok(Some(path.display().to_string()))
    })
}

/// Восстанавливает данные из файла резервной копии.
#[tauri::command]
pub async fn backup_import(app: AppHandle, password: String) -> R<bool> {
    let Some(path) = app.dialog().file().add_filter("Clavis", &["clavisbak"]).blocking_pick_file() else {
        return Ok(false);
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    blocking!(app, {
        let raw = std::fs::read(&path).map_err(|e| e.to_string())?;
        if raw.len() < BACKUP_MAGIC.len() + 16 || !raw.starts_with(BACKUP_MAGIC) {
            return Err("format".into());
        }
        let salt = &raw[BACKUP_MAGIC.len()..BACKUP_MAGIC.len() + 16];
        let key = vault::secret_key(&password, salt).map_err(|e| e.to_string())?;
        let plain = crate::crypto::open(&key, BACKUP_MAGIC, &raw[BACKUP_MAGIC.len() + 16..]).map_err(|_| "wrong_password".to_string())?;
        let bundle: Value = serde_json::from_slice(&plain).map_err(|_| "format".to_string())?;
        let state = app.state::<AppState>();
        {
            let mut g = state.vault.lock();
            let v = g.as_mut().ok_or("locked")?;
            v.import_bundle(&bundle).map_err(|e| e.to_string())?;
        }
        state.configure_expander();
        for ev in ["history:changed", "snippets:changed", "settings:changed"] {
            let _ = app.emit(ev, ());
        }
        Ok(true)
    })
}

fn chrono_like_date() -> String {
    // Дата для имени файла без лишних зависимостей: ГГГГ-ММ-ДД.
    let days = vault::now() / 86_400_000;
    let (mut y, mut d) = (1970i64, days);
    loop {
        let len = if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 { 366 } else { 365 };
        if d < len { break; }
        d -= len;
        y += 1;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let months = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut m = 0;
    while d >= months[m] { d -= months[m]; m += 1; }
    format!("{y}-{:02}-{:02}", m + 1, d + 1)
}

// ——— быстрая заметка ———

/// Создаёт заметку из текста быстрого окна: первая строка становится заголовком.
#[tauri::command]
pub fn quick_note(app: AppHandle, state: State<AppState>, text: String) -> R<()> {
    let text = text.trim().to_string();
    if text.is_empty() {
        ui::hide_quick(&app);
        return Ok(());
    }
    let mut lines = text.lines();
    let title: String = lines.next().unwrap_or("").trim().chars().take(200).collect();
    let para = |t: &str| if t.trim().is_empty() { json!({ "type": "paragraph" }) } else { json!({ "type": "paragraph", "content": [{ "type": "text", "text": t }] }) };
    let mut content = vec![json!({ "type": "heading", "attrs": { "level": 1 }, "content": [{ "type": "text", "text": title }] })];
    content.extend(lines.map(para));
    let doc = json!({ "type": "doc", "content": content });
    let id = uuid::Uuid::new_v4().to_string();
    with_vault(&state, |v| {
        v.put_note(&id, &doc).map_err(|e| e.to_string())?;
        v.data.notes.push(NoteMeta {
            id: id.clone(),
            title: title.clone(),
            updated: vault::now(),
            text: text.chars().take(4000).collect(),
            folder: None,
            tags: vec![],
            pinned: false,
            secret: false,
            trashed: 0,
        });
        Ok(())
    })?;
    ui::hide_quick(&app);
    let _ = app.emit("notes:changed", ());
    Ok(())
}

#[tauri::command]
pub fn quick_hide(app: AppHandle) {
    ui::hide_quick(&app);
}

#[tauri::command]
pub fn open_note(app: AppHandle, id: String) {
    ui::show_main(&app, Some("notes"));
    let _ = app.emit("open:note", id);
}

/// Уведомление Windows. Нажатие на него открывает заметку, если она указана.
pub fn notify(app: &AppHandle, title: &str, body: &str, note: Option<String>) {
    use tauri_plugin_notification::NotificationExt;
    let id = app.config().identifier.clone();
    let handle = app.clone();
    let click: Box<dyn Fn() + Send + Sync> = Box::new(move || {
        let h = handle.clone();
        let note = note.clone();
        let _ = handle.run_on_main_thread(move || match note {
            Some(n) => {
                ui::show_main(&h, Some("notes"));
                let _ = h.emit("open:note", n);
            }
            None => ui::show_main(&h, None),
        });
    });
    if winrt::toast(&id, title, body, click).is_err() {
        let _ = app.notification().builder().title(title).body(body).show();
    }
}

pub const GITHUB_URL: &str = "https://github.com/Minorq5/clavis";

/// Открывает страницу проекта на GitHub в браузере по умолчанию.
#[tauri::command]
pub fn open_github() -> R<()> {
    #[cfg(windows)]
    {
        std::process::Command::new("explorer").arg(GITHUB_URL).spawn().map_err(|e| e.to_string())?;
    }
    Ok(())
}
