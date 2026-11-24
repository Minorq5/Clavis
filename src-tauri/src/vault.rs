//! Зашифрованное хранилище.
//!
//! На диске:
//!   vault.json    заголовок: соли, обёрнутые ключи, счётчик ошибок PIN
//!   data.bin      история, сниппеты, папки, указатель заметок
//!   notes/<id>    каждая заметка отдельно
//!   blobs/<id>    полноразмерные изображения из истории
//!   inbox/<n>     записи, сделанные, пока хранилище было заблокировано
//!
//! Все файлы, кроме заголовка, зашифрованы ключом данных (DEK).
//! DEK обёрнут дважды: ключом из PIN + секрета устройства и ключом из фразы восстановления.

use crate::crypto::{self, sealed_box, Key};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

pub const MAX_PIN_FAILURES: u32 = 10;

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("хранилище заблокировано")]
    Locked,
    #[error("неверный PIN")]
    WrongPin,
    #[error("PIN отключён после слишком многих ошибок, нужна фраза восстановления")]
    PinDisabled,
    #[error("фраза восстановления не подошла")]
    WrongPhrase,
    #[error("файл повреждён")]
    Corrupt,
    #[error("ошибка диска: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Device(#[from] crate::device::DeviceError),
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Header {
    pub version: u32,
    pub pin_salt: String,
    pub pin_wrap: Option<String>,
    pub rec_salt: String,
    pub rec_wrap: String,
    pub pin_failures: u32,
    pub recovery_confirmed: bool,
    /// Открытый ключ для записей, сделанных при заблокированном хранилище.
    pub inbox_pk: String,
    /// Закрытый ключ к нему, зашифрованный DEK.
    pub inbox_sk: String,
    /// DEK, обёрнутый ключом из подписи Windows Hello и секрета устройства.
    #[serde(default)]
    pub hello_wrap: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClipItem {
    pub id: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
    #[serde(default)]
    pub rich: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_size: Option<[u32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub at: i64,
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Folder {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Snippet {
    pub id: String,
    pub title: String,
    pub body: String,
    pub folder: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub abbr: String,
    pub updated: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NoteMeta {
    pub id: String,
    pub title: String,
    pub updated: i64,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub folder: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub pinned: bool,
    /// Содержимое дополнительно зашифровано отдельным паролем.
    #[serde(default)]
    pub secret: bool,
    /// Время переноса в корзину (0, если заметка не удалена).
    #[serde(default)]
    pub trashed: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct JournalEntry {
    pub at: i64,
    pub kind: String,
    #[serde(default)]
    pub detail: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Reminder {
    pub id: String,
    pub note: String,
    pub text: String,
    pub at: i64,
    #[serde(default)]
    pub fired: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VersionMeta {
    pub id: String,
    pub note: String,
    pub at: i64,
    pub size: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Template {
    pub id: String,
    pub title: String,
    pub doc: serde_json::Value,
}

/// Общий документ, известный этому компьютеру. Ключи по эпохам хранятся только здесь,
/// внутри зашифрованного хранилища.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SharedLocal {
    pub id: String,
    #[serde(default)]
    pub title: String,
    pub role: String,
    pub epoch: u32,
    #[serde(default)]
    pub keys: std::collections::BTreeMap<u32, String>,
    #[serde(default)]
    pub last_id: i64,
    #[serde(default)]
    pub updated: i64,
    /// Документ подтверждён сервером (создан нами, получен по ссылке или из списка).
    #[serde(default)]
    pub joined: bool,
    /// "note" или "snippets" (командный набор сниппетов).
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub versioned_at: i64,
}

fn default_kind() -> String {
    "note".into()
}

/// Правка общего документа, ещё не отправленная на сервер.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OutItem {
    pub doc: String,
    pub update: String,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Data {
    #[serde(default)]
    pub history: Vec<ClipItem>,
    #[serde(default)]
    pub folders: Vec<Folder>,
    #[serde(default)]
    pub snippets: Vec<Snippet>,
    #[serde(default)]
    pub notes: Vec<NoteMeta>,
    #[serde(default)]
    pub shared: Vec<SharedLocal>,
    #[serde(default)]
    pub outbox: Vec<OutItem>,
    #[serde(default)]
    pub note_folders: Vec<Folder>,
    #[serde(default)]
    pub journal: Vec<JournalEntry>,
    #[serde(default)]
    pub reminders: Vec<Reminder>,
    #[serde(default)]
    pub versions: Vec<VersionMeta>,
    #[serde(default)]
    pub templates: Vec<Template>,
    /// Разрешения на отправку текста во внешние службы (перевод, короткие ссылки).
    #[serde(default)]
    pub consents: Vec<String>,
    /// Сколько раз и когда последний раз вставляли каждый сниппет.
    #[serde(default)]
    pub snippet_uses: std::collections::BTreeMap<String, SnippetUse>,
    /// Время последнего упоминания, о котором уже сообщили, по каждой общей заметке.
    #[serde(default)]
    pub mentions_seen: std::collections::BTreeMap<String, i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SnippetUse {
    pub count: u64,
    pub last: i64,
}

pub struct Vault {
    dir: PathBuf,
    dek: Key,
    pub data: Data,
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

fn safe_id(id: &str) -> Option<&str> {
    (!id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')).then_some(id)
}

pub fn header_path(dir: &Path) -> PathBuf {
    dir.join("vault.json")
}

pub fn read_header(dir: &Path) -> Option<Header> {
    let raw = fs::read(header_path(dir)).ok()?;
    serde_json::from_slice(&raw).ok()
}

fn write_header(dir: &Path, h: &Header) -> Result<(), VaultError> {
    write_atomic(&header_path(dir), &serde_json::to_vec_pretty(h).map_err(|_| VaultError::Corrupt)?)?;
    Ok(())
}

fn pin_kek(pin: &str, salt: &[u8], device_secret: &[u8]) -> Result<Key, VaultError> {
    let stretched = crypto::stretch(pin.as_bytes(), salt).map_err(|_| VaultError::Corrupt)?;
    let mut ikm = Zeroizing::new(Vec::with_capacity(64));
    ikm.extend_from_slice(stretched.as_ref());
    ikm.extend_from_slice(device_secret);
    Ok(crypto::derive(&ikm, "pin-kek"))
}

fn hello_kek(signature: &[u8], device_secret: &[u8]) -> Key {
    let mut ikm = Zeroizing::new(signature.to_vec());
    ikm.extend_from_slice(device_secret);
    crypto::derive(&ikm, "hello-kek")
}

/// Ключ секретной заметки из её пароля.
pub fn secret_key(password: &str, salt: &[u8]) -> Result<Key, VaultError> {
    let s = crypto::stretch(password.as_bytes(), salt).map_err(|_| VaultError::Corrupt)?;
    Ok(crypto::derive(s.as_ref(), "secret-note"))
}

fn rec_kek(entropy: &[u8], salt: &[u8]) -> Result<Key, VaultError> {
    let stretched = crypto::stretch(entropy, salt).map_err(|_| VaultError::Corrupt)?;
    Ok(crypto::derive(stretched.as_ref(), "recovery-kek"))
}

fn unwrap_dek(kek: &[u8; 32], wrapped_b64: &str) -> Option<Key> {
    let wrapped = B64.decode(wrapped_b64).ok()?;
    let raw = crypto::open(kek, b"dek", &wrapped).ok()?;
    let mut k = Zeroizing::new([0u8; 32]);
    if raw.len() != 32 {
        return None;
    }
    k.copy_from_slice(&raw);
    Some(k)
}

fn b64d(s: &str) -> Result<Vec<u8>, VaultError> {
    B64.decode(s).map_err(|_| VaultError::Corrupt)
}

impl Vault {
    /// Создаёт новое хранилище. Возвращает его и 24 слова фразы восстановления.
    pub fn create(dir: &Path, pin: &str, device_secret: &[u8]) -> Result<(Vault, Vec<String>), VaultError> {
        fs::create_dir_all(dir)?;
        // Если это повторная настройка, старое содержимое не нужно.
        for sub in ["notes", "blobs", "inbox"] {
            let _ = fs::remove_dir_all(dir.join(sub));
        }
        let _ = fs::remove_file(dir.join("data.bin"));

        let dek = crypto::random_key();
        let pin_salt = crypto::random::<16>();
        let rec_salt = crypto::random::<16>();
        let entropy = Zeroizing::new(crypto::random::<32>());
        let mnemonic = bip39::Mnemonic::from_entropy(entropy.as_ref()).map_err(|_| VaultError::Corrupt)?;
        let words: Vec<String> = mnemonic.words().map(String::from).collect();

        let pkek = pin_kek(pin, &pin_salt, device_secret)?;
        let rkek = rec_kek(entropy.as_ref(), &rec_salt)?;
        let (inbox_sk, inbox_pk) = sealed_box::keypair();

        let header = Header {
            version: 1,
            pin_salt: B64.encode(pin_salt),
            pin_wrap: Some(B64.encode(crypto::seal(&pkek, b"dek", dek.as_ref()))),
            rec_salt: B64.encode(rec_salt),
            rec_wrap: B64.encode(crypto::seal(&rkek, b"dek", dek.as_ref())),
            pin_failures: 0,
            recovery_confirmed: false,
            inbox_pk: B64.encode(inbox_pk),
            inbox_sk: B64.encode(crypto::seal(&dek, b"inbox-sk", inbox_sk.as_ref())),
            hello_wrap: None,
        };
        write_header(dir, &header)?;
        let v = Vault { dir: dir.to_path_buf(), dek, data: Data::default() };
        v.save()?;
        Ok((v, words))
    }

    pub fn confirm_recovery(&self) -> Result<(), VaultError> {
        let mut h = read_header(&self.dir).ok_or(VaultError::Corrupt)?;
        h.recovery_confirmed = true;
        write_header(&self.dir, &h)
    }

    pub fn unlock(dir: &Path, pin: &str, device_secret: &[u8]) -> Result<Vault, (VaultError, u32)> {
        let mut h = read_header(dir).ok_or((VaultError::Corrupt, 0))?;
        let Some(wrap) = h.pin_wrap.clone() else {
            return Err((VaultError::PinDisabled, 0));
        };
        let salt = b64d(&h.pin_salt).map_err(|e| (e, 0))?;
        let kek = pin_kek(pin, &salt, device_secret).map_err(|e| (e, 0))?;
        match unwrap_dek(&kek, &wrap) {
            Some(dek) => {
                if h.pin_failures != 0 {
                    h.pin_failures = 0;
                    let _ = write_header(dir, &h);
                }
                let v = Vault::load(dir, dek).map_err(|e| (e, MAX_PIN_FAILURES))?;
                Ok(v)
            }
            None => {
                h.pin_failures += 1;
                if h.pin_failures >= MAX_PIN_FAILURES {
                    h.pin_wrap = None; // дальше только фраза восстановления
                }
                let left = MAX_PIN_FAILURES.saturating_sub(h.pin_failures);
                let _ = write_header(dir, &h);
                Err((if left == 0 { VaultError::PinDisabled } else { VaultError::WrongPin }, left))
            }
        }
    }

    /// Открывает хранилище фразой восстановления и задаёт новый PIN.
    pub fn recover(dir: &Path, words: &[String], new_pin: &str, device_secret: &[u8]) -> Result<Vault, VaultError> {
        let mut h = read_header(dir).ok_or(VaultError::Corrupt)?;
        let phrase = words.join(" ");
        let mnemonic = bip39::Mnemonic::parse_normalized(&phrase).map_err(|_| VaultError::WrongPhrase)?;
        let entropy = Zeroizing::new(mnemonic.to_entropy());
        let rkek = rec_kek(&entropy, &b64d(&h.rec_salt)?)?;
        let dek = unwrap_dek(&rkek, &h.rec_wrap).ok_or(VaultError::WrongPhrase)?;
        let pin_salt = crypto::random::<16>();
        let pkek = pin_kek(new_pin, &pin_salt, device_secret)?;
        h.pin_salt = B64.encode(pin_salt);
        h.pin_wrap = Some(B64.encode(crypto::seal(&pkek, b"dek", dek.as_ref())));
        h.pin_failures = 0;
        write_header(dir, &h)?;
        Vault::load(dir, dek)
    }

    fn load(dir: &Path, dek: Key) -> Result<Vault, VaultError> {
        let path = dir.join("data.bin");
        let data = if path.exists() {
            let raw = fs::read(&path)?;
            let plain = crypto::open(&dek, b"data", &raw).map_err(|_| VaultError::Corrupt)?;
            serde_json::from_slice(&plain).map_err(|_| VaultError::Corrupt)?
        } else {
            Data::default()
        };
        let mut v = Vault { dir: dir.to_path_buf(), dek, data };
        v.drain_inbox();
        Ok(v)
    }

    pub fn save(&self) -> Result<(), VaultError> {
        let plain = Zeroizing::new(serde_json::to_vec(&self.data).map_err(|_| VaultError::Corrupt)?);
        write_atomic(&self.dir.join("data.bin"), &crypto::seal(&self.dek, b"data", &plain))?;
        Ok(())
    }

    fn inbox_sk(&self) -> Option<Zeroizing<[u8; 32]>> {
        let h = read_header(&self.dir)?;
        let raw = crypto::open(&self.dek, b"inbox-sk", &B64.decode(h.inbox_sk).ok()?).ok()?;
        let mut k = Zeroizing::new([0u8; 32]);
        k.copy_from_slice(raw.get(..32)?);
        Some(k)
    }

    /// Переносит в историю то, что было скопировано при заблокированном хранилище.
    fn drain_inbox(&mut self) {
        let dir = self.dir.join("inbox");
        let Ok(entries) = fs::read_dir(&dir) else { return };
        let Some(sk) = self.inbox_sk() else { return };
        let mut items: Vec<(ClipItem, Option<Vec<u8>>)> = Vec::new();
        let mut journal: Vec<JournalEntry> = Vec::new();
        let mut files: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        files.sort();
        for f in &files {
            if let Ok(raw) = fs::read(f) {
                if let Ok(plain) = sealed_box::open_with(&sk, &raw) {
                    if let Ok(entry) = serde_json::from_slice::<InboxEntry>(&plain) {
                        if let Some(item) = entry.item {
                            items.push((item, entry.blob.and_then(|b| B64.decode(b).ok())));
                        }
                        if let Some(j) = entry.journal {
                            journal.push(j);
                        }
                    }
                }
            }
        }
        for (item, blob) in items {
            if let Some(b) = blob {
                let _ = self.put_blob(&item.id, &b);
            }
            self.push_history(item, usize::MAX);
        }
        for j in journal {
            self.log(j);
        }
        for f in files {
            let _ = fs::remove_file(f);
        }
        let _ = self.save();
    }

    pub fn log(&mut self, e: JournalEntry) {
        self.data.journal.push(e);
        self.data.journal.sort_by_key(|j| j.at);
        let n = self.data.journal.len();
        if n > 500 {
            self.data.journal.drain(..n - 500);
        }
    }

    pub fn log_now(&mut self, kind: &str, detail: &str) {
        self.log(JournalEntry { at: now(), kind: kind.into(), detail: detail.into() });
    }

    // ——— Windows Hello ———

    pub fn enable_hello(&self, signature: &[u8], device_secret: &[u8]) -> Result<(), VaultError> {
        let kek = hello_kek(signature, device_secret);
        let mut h = read_header(&self.dir).ok_or(VaultError::Corrupt)?;
        h.hello_wrap = Some(B64.encode(crypto::seal(&kek, b"dek", self.dek.as_ref())));
        write_header(&self.dir, &h)
    }

    pub fn disable_hello(&self) -> Result<(), VaultError> {
        let mut h = read_header(&self.dir).ok_or(VaultError::Corrupt)?;
        h.hello_wrap = None;
        write_header(&self.dir, &h)
    }

    pub fn unlock_hello(dir: &Path, signature: &[u8], device_secret: &[u8]) -> Result<Vault, VaultError> {
        let h = read_header(dir).ok_or(VaultError::Corrupt)?;
        let wrap = h.hello_wrap.ok_or(VaultError::WrongPin)?;
        let dek = unwrap_dek(&hello_kek(signature, device_secret), &wrap).ok_or(VaultError::WrongPin)?;
        Vault::load(dir, dek)
    }

    // ——— история версий ———

    pub fn put_version(&mut self, note: &str, bytes: &[u8]) -> Result<(), VaultError> {
        let id = format!("ver-{}", uuid::Uuid::new_v4().simple());
        self.put_blob(&id, bytes)?;
        self.data.versions.push(VersionMeta { id, note: note.to_string(), at: now(), size: bytes.len() });
        self.prune_versions(note);
        Ok(())
    }

    /// Оставляет все версии за последние сутки (не чаще раза в 10 минут),
    /// по одной в день за месяц и по одной в неделю до полугода.
    fn prune_versions(&mut self, note: &str) {
        let t = now();
        let mut keep_slots = std::collections::HashSet::new();
        let mut list: Vec<VersionMeta> = self.data.versions.iter().filter(|v| v.note == note).cloned().collect();
        list.sort_by(|a, b| b.at.cmp(&a.at));
        let mut drop = Vec::new();
        for v in &list {
            let age = t - v.at;
            let slot = if age < 86_400_000 {
                format!("m{}", v.at / 600_000)
            } else if age < 30 * 86_400_000 {
                format!("d{}", v.at / 86_400_000)
            } else if age < 180 * 86_400_000 {
                format!("w{}", v.at / (7 * 86_400_000))
            } else {
                drop.push(v.id.clone());
                continue;
            };
            if !keep_slots.insert(slot) {
                drop.push(v.id.clone());
            }
        }
        for id in &drop {
            let _ = self.remove_blob(id);
        }
        self.data.versions.retain(|v| !drop.contains(&v.id));
    }

    pub fn drop_versions(&mut self, note: &str) {
        let ids: Vec<String> = self.data.versions.iter().filter(|v| v.note == note).map(|v| v.id.clone()).collect();
        for id in &ids {
            let _ = self.remove_blob(id);
        }
        self.data.versions.retain(|v| v.note != note);
    }

    pub fn push_history(&mut self, item: ClipItem, limit: usize) {
        // Повтор того же содержимого поднимает старую запись наверх.
        if let Some(pos) = self.data.history.iter().position(|h| same_content(h, &item)) {
            let mut old = self.data.history.remove(pos);
            old.at = item.at;
            old.source = item.source.or(old.source);
            if item.kind == "image" {
                let _ = self.remove_blob(&item.id);
            }
            self.data.history.insert(0, old);
        } else {
            self.data.history.insert(0, item);
        }
        self.trim_history(limit);
    }

    pub fn trim_history(&mut self, limit: usize) {
        let mut unpinned = 0;
        let mut drop_ids = Vec::new();
        self.data.history.retain(|h| {
            if h.pinned {
                return true;
            }
            unpinned += 1;
            if unpinned > limit {
                drop_ids.push(h.id.clone());
                false
            } else {
                true
            }
        });
        for id in drop_ids {
            let _ = self.remove_blob(&id);
        }
    }

    pub fn put_blob(&self, id: &str, bytes: &[u8]) -> Result<(), VaultError> {
        let id = safe_id(id).ok_or(VaultError::Corrupt)?;
        write_atomic(&self.dir.join("blobs").join(id), &crypto::seal(&self.dek, format!("blob:{id}").as_bytes(), bytes))?;
        Ok(())
    }

    pub fn get_blob(&self, id: &str) -> Result<Zeroizing<Vec<u8>>, VaultError> {
        let id = safe_id(id).ok_or(VaultError::Corrupt)?;
        let raw = fs::read(self.dir.join("blobs").join(id))?;
        crypto::open(&self.dek, format!("blob:{id}").as_bytes(), &raw).map_err(|_| VaultError::Corrupt)
    }

    pub fn remove_blob(&self, id: &str) -> Result<(), VaultError> {
        if let Some(id) = safe_id(id) {
            let p = self.dir.join("blobs").join(id);
            if p.exists() {
                fs::remove_file(p)?;
            }
        }
        Ok(())
    }

    pub fn put_note(&self, id: &str, doc: &serde_json::Value) -> Result<(), VaultError> {
        let id = safe_id(id).ok_or(VaultError::Corrupt)?;
        let plain = Zeroizing::new(serde_json::to_vec(doc).map_err(|_| VaultError::Corrupt)?);
        write_atomic(&self.dir.join("notes").join(id), &crypto::seal(&self.dek, format!("note:{id}").as_bytes(), &plain))?;
        Ok(())
    }

    pub fn get_note(&self, id: &str) -> Result<serde_json::Value, VaultError> {
        let id = safe_id(id).ok_or(VaultError::Corrupt)?;
        let raw = fs::read(self.dir.join("notes").join(id))?;
        let plain = crypto::open(&self.dek, format!("note:{id}").as_bytes(), &raw).map_err(|_| VaultError::Corrupt)?;
        serde_json::from_slice(&plain).map_err(|_| VaultError::Corrupt)
    }

    /// Все данные хранилища одним JSON для резервной копии.
    pub fn export_bundle(&self) -> Result<serde_json::Value, VaultError> {
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD;
        let mut notes = serde_json::Map::new();
        for n in &self.data.notes {
            if let Ok(doc) = self.get_note(&n.id) {
                notes.insert(n.id.clone(), doc);
            }
        }
        let mut blobs = serde_json::Map::new();
        if let Ok(rd) = fs::read_dir(self.dir.join("blobs")) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if let Ok(bytes) = self.get_blob(&name) {
                    blobs.insert(name, serde_json::Value::String(b64.encode(&bytes[..])));
                }
            }
        }
        Ok(serde_json::json!({
            "format": "clavis-backup/1",
            "created": now(),
            "data": serde_json::to_value(&self.data).map_err(|_| VaultError::Corrupt)?,
            "notes": notes,
            "blobs": blobs,
        }))
    }

    /// Восстанавливает данные из резервной копии. Общие заметки привязаны к этому компьютеру и остаются как есть.
    pub fn import_bundle(&mut self, v: &serde_json::Value) -> Result<(), VaultError> {
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD;
        if v.get("format").and_then(|f| f.as_str()) != Some("clavis-backup/1") {
            return Err(VaultError::Corrupt);
        }
        let mut data: Data = serde_json::from_value(v.get("data").cloned().unwrap_or_default()).map_err(|_| VaultError::Corrupt)?;
        if let Some(blobs) = v.get("blobs").and_then(|b| b.as_object()) {
            for (id, b) in blobs {
                if let Some(bytes) = b.as_str().and_then(|s| b64.decode(s).ok()) {
                    self.put_blob(id, &bytes)?;
                }
            }
        }
        if let Some(notes) = v.get("notes").and_then(|n| n.as_object()) {
            for (id, doc) in notes {
                self.put_note(id, doc)?;
            }
        }
        data.shared = std::mem::take(&mut self.data.shared);
        data.outbox = std::mem::take(&mut self.data.outbox);
        data.journal = std::mem::take(&mut self.data.journal);
        self.data = data;
        self.log_now("backup_restore", "");
        self.save()
    }

    pub fn remove_note(&self, id: &str) -> Result<(), VaultError> {
        if let Some(id) = safe_id(id) {
            let p = self.dir.join("notes").join(id);
            if p.exists() {
                fs::remove_file(p)?;
            }
        }
        Ok(())
    }
}

fn same_content(a: &ClipItem, b: &ClipItem) -> bool {
    if a.kind != b.kind {
        return false;
    }
    match a.kind.as_str() {
        "text" => a.text == b.text,
        "files" => a.files == b.files,
        "image" => a.image_size == b.image_size && a.image == b.image,
        _ => false,
    }
}

#[derive(Serialize, Deserialize)]
struct InboxEntry {
    #[serde(default)]
    item: Option<ClipItem>,
    #[serde(default)]
    blob: Option<String>,
    #[serde(default)]
    journal: Option<JournalEntry>,
}

/// Записывает элемент истории, когда хранилище заблокировано: шифрует его открытым ключом,
/// так что прочитать запись можно только после разблокировки.
pub fn write_inbox(dir: &Path, item: ClipItem, blob: Option<&[u8]>) -> Result<(), VaultError> {
    let h = read_header(dir).ok_or(VaultError::Locked)?;
    let entry = InboxEntry { item: Some(item), blob: blob.map(|b| B64.encode(b)), journal: None };
    write_sealed(dir, &h, &entry)
}

/// Запись в журнал, когда хранилище заблокировано (например, неверный PIN).
pub fn write_journal_locked(dir: &Path, kind: &str, detail: &str) -> Result<(), VaultError> {
    let h = read_header(dir).ok_or(VaultError::Locked)?;
    let entry = InboxEntry { item: None, blob: None, journal: Some(JournalEntry { at: now(), kind: kind.into(), detail: detail.into() }) };
    write_sealed(dir, &h, &entry)
}

fn write_sealed(dir: &Path, h: &Header, entry: &InboxEntry) -> Result<(), VaultError> {
    let pk_raw = b64d(&h.inbox_pk)?;
    let pk: [u8; 32] = pk_raw.try_into().map_err(|_| VaultError::Corrupt)?;
    let plain = Zeroizing::new(serde_json::to_vec(entry).map_err(|_| VaultError::Corrupt)?);
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let name = format!("{:020}-{:08}-{}", now(), n, uuid::Uuid::new_v4().simple());
    write_atomic(&dir.join("inbox").join(name), &sealed_box::seal_to(&pk, &plain))?;
    // Не даём папке разрастись без ограничений.
    if let Ok(rd) = fs::read_dir(dir.join("inbox")) {
        let mut all: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        if all.len() > 500 {
            all.sort();
            for p in &all[..all.len() - 500] {
                let _ = fs::remove_file(p);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("clavis-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn text(t: &str) -> ClipItem {
        ClipItem { id: uuid::Uuid::new_v4().to_string(), kind: "text".into(), text: Some(t.into()), html: None, rich: false, image: None, image_size: None, files: None, source: None, at: now(), pinned: false }
    }

    #[test]
    fn create_unlock_recover() {
        let dir = tmp();
        let secret = [7u8; 32];
        let (mut v, words) = Vault::create(&dir, "2468", &secret).unwrap();
        assert_eq!(words.len(), 24);
        v.push_history(text("привет"), 100);
        v.save().unwrap();
        drop(v);

        assert!(matches!(Vault::unlock(&dir, "1111", &secret), Err((VaultError::WrongPin, 9))));
        let v = Vault::unlock(&dir, "2468", &secret).ok().unwrap();
        assert_eq!(v.data.history[0].text.as_deref(), Some("привет"));
        drop(v);

        // Другой секрет устройства: правильный PIN не помогает.
        assert!(Vault::unlock(&dir, "2468", &[1u8; 32]).is_err());

        let v = Vault::recover(&dir, &words, "1357", &secret).unwrap();
        assert_eq!(v.data.history.len(), 1);
        drop(v);
        assert!(Vault::unlock(&dir, "1357", &secret).is_ok());
    }

    #[test]
    fn inbox_and_dedup() {
        let dir = tmp();
        let secret = [3u8; 32];
        let (mut v, _) = Vault::create(&dir, "0000", &secret).unwrap();
        v.push_history(text("a"), 100);
        v.save().unwrap();
        drop(v);
        write_inbox(&dir, text("b"), None).unwrap();
        write_inbox(&dir, text("a"), None).unwrap();
        let v = Vault::unlock(&dir, "0000", &secret).ok().unwrap();
        let texts: Vec<_> = v.data.history.iter().map(|h| h.text.clone().unwrap()).collect();
        assert_eq!(texts, vec!["a", "b"]);
    }

    #[test]
    fn pin_lockout() {
        let dir = tmp();
        let secret = [9u8; 32];
        let (v, words) = Vault::create(&dir, "4321", &secret).unwrap();
        drop(v);
        for _ in 0..MAX_PIN_FAILURES {
            let _ = Vault::unlock(&dir, "0000", &secret);
        }
        assert!(matches!(Vault::unlock(&dir, "4321", &secret), Err((VaultError::PinDisabled, _))));
        assert!(Vault::recover(&dir, &words, "5555", &secret).is_ok());
    }

    #[test]
    fn hello_and_journal() {
        let dir = tmp();
        let secret = [5u8; 32];
        let (v, _) = Vault::create(&dir, "1234", &secret).unwrap();
        v.enable_hello(b"signature-bytes", &secret).unwrap();
        drop(v);
        assert!(Vault::unlock_hello(&dir, b"other", &secret).is_err());
        assert!(Vault::unlock_hello(&dir, b"signature-bytes", &[6u8; 32]).is_err());
        write_journal_locked(&dir, "unlock_fail", "PIN").unwrap();
        let mut v = Vault::unlock_hello(&dir, b"signature-bytes", &secret).ok().unwrap();
        assert_eq!(v.data.journal.len(), 1);
        assert_eq!(v.data.journal[0].kind, "unlock_fail");
        v.disable_hello().unwrap();
        v.log_now("lock", "");
        assert_eq!(v.data.journal.len(), 2);
        drop(v);
        assert!(Vault::unlock_hello(&dir, b"signature-bytes", &secret).is_err());
    }

    #[test]
    fn versions_are_pruned() {
        let dir = tmp();
        let (mut v, _) = Vault::create(&dir, "1234", &[1u8; 32]).unwrap();
        for _ in 0..5 {
            v.put_version("n1", b"{}").unwrap();
        }
        // Пять версий за одну и ту же десятиминутку схлопываются в одну.
        assert_eq!(v.data.versions.iter().filter(|x| x.note == "n1").count(), 1);
        let id = v.data.versions[0].id.clone();
        assert_eq!(&v.get_blob(&id).unwrap()[..], b"{}");
        v.drop_versions("n1");
        assert!(v.data.versions.is_empty());
        assert!(v.get_blob(&id).is_err());
    }
}
