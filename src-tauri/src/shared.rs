//! Общие заметки.
//!
//! Документ — это CRDT Yjs. Каждая правка шифруется ключом документа (XChaCha20-Poly1305)
//! до того, как покинет компьютер. Ключ документа знают только участники: для каждого он
//! зашифрован его ключом X25519. Когда владелец удаляет участника, ключ меняется (новая эпоха),
//! и удалённый участник не может читать дальнейшие правки.
//!
//! Правки идут двумя путями: сразу в канал реального времени (чтобы соавторы видели их мгновенно)
//! и в журнал на сервере (чтобы их получили те, кто сейчас не в сети). Пока сети нет,
//! правки копятся в зашифрованном хранилище и уходят при подключении.

use crate::crypto::{self, sealed_box};
use crate::license::LicenseError;
use crate::realtime::{self, Realtime};
use crate::state::AppState;
use crate::vault::{self, OutItem, SharedLocal, Vault};
use base64::{engine::general_purpose::STANDARD as B64, engine::general_purpose::URL_SAFE_NO_PAD as B64URL, Engine};
use parking_lot::Mutex;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use zeroize::Zeroizing;

/// После скольких правок в журнале их стоит свернуть в снимок.
const COMPACT_AFTER: u64 = 300;
const INVITE_PREFIX: &str = "clavis:join/";

pub struct Shared {
    rt: Realtime,
    open: Mutex<HashSet<String>>,
    /// Канал Realtime каждого открытого документа.
    topics: Mutex<HashMap<String, String>>,
    flush_now: Arc<AtomicBool>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SharedMeta {
    pub id: String,
    pub title: String,
    pub role: String,
    pub updated: i64,
    pub kind: String,
    pub pinned: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TeamSnippet {
    pub id: String,
    pub set_id: String,
    pub set_title: String,
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
    pub abbr: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Opened {
    pub state: String,
    pub role: String,
    pub me: String,
    pub name: String,
    pub online: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    pub device: String,
    pub name: String,
    pub role: String,
    pub me: bool,
}

type R<T> = Result<T, String>;

fn err(e: LicenseError) -> String {
    match e {
        LicenseError::Network => "network".into(),
        LicenseError::Clock => "clock".into(),
        LicenseError::Server(s) => s,
        other => other.to_string(),
    }
}

/// Имя канала выводится из ключа документа: без ключа канал не найти,
/// а после смены ключа (удаления участника) канал тоже меняется.
fn topic(doc: &SharedLocal) -> Option<String> {
    let key = key_for(doc, doc.epoch)?;
    let tag = crypto::derive(key.as_ref(), &format!("clavis/topic/{}/{}", doc.id, doc.epoch));
    Some(format!("d{}", tag[..16].iter().map(|b| format!("{b:02x}")).collect::<String>()))
}

/// Подключается к каналу текущей эпохи, если документ открыт.
fn sync_topic(state: &AppState, id: &str) {
    let want = if state.shared.open.lock().contains(id) { local(state, id).ok().and_then(|d| topic(&d)) } else { None };
    let mut topics = state.shared.topics.lock();
    let have = topics.get(id).cloned();
    if have == want {
        return;
    }
    if let Some(t) = have {
        state.shared.rt.leave(&t);
        topics.remove(id);
    }
    if let Some(t) = want {
        state.shared.rt.join(&t);
        topics.insert(id.to_string(), t);
    }
}

fn send(state: &AppState, id: &str, event: &str, payload: Value) {
    let t = state.shared.topics.lock().get(id).cloned();
    if let Some(t) = t {
        state.shared.rt.send(&t, event, payload);
    }
}

fn valid_uuid(id: &str) -> bool {
    uuid::Uuid::parse_str(id).is_ok()
}

fn aad(kind: &str, doc: &str, epoch: u32) -> Vec<u8> {
    format!("clavis/{kind}/{doc}/{epoch}").into_bytes()
}

fn key_from_b64(s: &str) -> Option<Zeroizing<[u8; 32]>> {
    let v = Zeroizing::new(B64.decode(s).ok()?);
    let arr: [u8; 32] = v.as_slice().try_into().ok()?;
    Some(Zeroizing::new(arr))
}

/// Склеивает обновления Yjs в одно. Пустой список даёт пустой документ.
pub fn merge(updates: &[Vec<u8>]) -> Option<Vec<u8>> {
    if updates.is_empty() {
        return Some(yrs::Doc::new().transact().encode_state_as_update_v1(&yrs::StateVector::default()));
    }
    let refs: Vec<&[u8]> = updates.iter().map(|u| u.as_slice()).collect();
    yrs::merge_updates_v1(&refs).ok()
}

use yrs::{ReadTxn, Transact};

impl Shared {
    pub fn start(app: AppHandle) -> Shared {
        let handle = app.clone();
        let rt = Realtime::start(move |ev| on_realtime(&handle, ev));
        let flush_now = Arc::new(AtomicBool::new(false));
        let shared = Shared { rt, open: Mutex::new(HashSet::new()), topics: Mutex::new(HashMap::new()), flush_now: flush_now.clone() };

        // Отправка накопленных правок и периодическая сверка с сервером.
        let h = app.clone();
        std::thread::Builder::new()
            .name("shared-sync".into())
            .spawn(move || {
                let mut tick: u32 = 0;
                loop {
                    std::thread::sleep(Duration::from_millis(250));
                    tick = tick.wrapping_add(1);
                    let urgent = flush_now.swap(false, Ordering::SeqCst);
                    // Не чаще раза в 1,5 секунды, если правки идут непрерывно.
                    if (urgent || tick % 6 == 0) && h.try_state::<AppState>().is_some() {
                        flush(&h);
                    }
                    // Раз в минуту обновляем командные сниппеты.
                    if tick % 240 == 40 && h.try_state::<AppState>().is_some() {
                        sync_snippet_sets(&h);
                    }
                    // Раз в минуту проверяем, не упомянули ли вас в комментариях.
                    if tick % 240 == 160 && h.try_state::<AppState>().is_some() {
                        scan_mentions(&h);
                    }
                    // Раз в 30 секунд догоняем то, что могли пропустить в канале.
                    if tick % 120 == 0 {
                        let Some(st) = h.try_state::<AppState>() else { continue };
                        let ids: Vec<String> = st.shared.open.lock().iter().cloned().collect();
                        for id in ids {
                            let _ = pull_and_emit(&h, &id);
                        }
                    }
                }
            })
            .ok();
        shared
    }

    pub fn online(&self) -> bool {
        self.rt.online()
    }
}

fn with_vault<T>(state: &AppState, f: impl FnOnce(&mut Vault) -> R<T>) -> R<T> {
    let mut g = state.vault.lock();
    let Some(v) = g.as_mut() else { return Err("locked".into()) };
    f(v)
}

fn display_name(state: &AppState) -> String {
    let n = state.settings.read().display_name.trim().to_string();
    if n.is_empty() {
        crate::device::machine_name()
    } else {
        n
    }
}

fn register(state: &AppState) -> R<String> {
    let name = display_name(state);
    let req = state.license.lock().registration(&name).map_err(err)?;
    match req {
        Ok(id) => Ok(id),
        Err(env) => {
            let v = crate::license::post(&env).map_err(err)?;
            let id = v.get("device").and_then(Value::as_str).ok_or("server")?.to_string();
            state.license.lock().set_registered(&id, &name);
            Ok(id)
        }
    }
}

fn call(state: &AppState, op: &str, payload: Value) -> R<Value> {
    let env = state.license.lock().signed(op, payload).map_err(err)?;
    crate::license::post(&env).map_err(err)
}

fn local(state: &AppState, id: &str) -> R<SharedLocal> {
    with_vault(state, |v| v.data.shared.iter().find(|s| s.id == id).cloned().ok_or_else(|| "missing".into()))
}

fn key_for(doc: &SharedLocal, epoch: u32) -> Option<Zeroizing<[u8; 32]>> {
    doc.keys.get(&epoch).and_then(|k| key_from_b64(k))
}

/// Открывает обёрнутые ключи документа своим ключом X25519 и запоминает их.
fn absorb_keys(state: &AppState, id: &str, keys: &[Value]) {
    let Ok((_, sk)) = state.license.lock().kx() else { return };
    let mut found: BTreeMap<u32, String> = BTreeMap::new();
    for k in keys {
        let (Some(ep), Some(w)) = (k.get("epoch").and_then(Value::as_u64), k.get("wrapped").and_then(Value::as_str)) else { continue };
        if let Ok(raw) = B64.decode(w) {
            if let Ok(key) = sealed_box::open_with(&sk, &raw) {
                if key.len() == 32 {
                    found.insert(ep as u32, B64.encode(&key[..]));
                }
            }
        }
    }
    if found.is_empty() {
        return;
    }
    let _ = with_vault(state, |v| {
        if let Some(d) = v.data.shared.iter_mut().find(|s| s.id == id) {
            for (ep, k) in found {
                d.keys.insert(ep, k);
            }
        }
        v.save().map_err(|e| e.to_string())
    });
}

fn encrypt_meta(key: &[u8; 32], id: &str, epoch: u32, title: &str, kind: &str) -> String {
    let plain = json!({ "title": title, "kind": kind }).to_string();
    B64.encode(crypto::seal(key, &aad("meta", id, epoch), plain.as_bytes()))
}

/// Название и вид документа из зашифрованных метаданных.
fn decrypt_meta(doc: &SharedLocal, epoch: u32, ct: &str) -> Option<(String, String)> {
    let key = key_for(doc, epoch)?;
    let raw = crypto::open(&key, &aad("meta", &doc.id, epoch), &B64.decode(ct).ok()?).ok()?;
    let v: Value = serde_json::from_slice(&raw).ok()?;
    let title = v.get("title").and_then(Value::as_str).unwrap_or_default().to_string();
    let kind = v.get("kind").and_then(Value::as_str).filter(|k| *k == "snippets").unwrap_or("note").to_string();
    Some((title, kind))
}

fn meta_of(s: &SharedLocal) -> SharedMeta {
    SharedMeta { id: s.id.clone(), title: s.title.clone(), role: s.role.clone(), updated: s.updated, kind: s.kind.clone(), pinned: s.pinned }
}

// ——— список, создание, открытие ———

pub fn list(app: &AppHandle) -> R<Vec<SharedMeta>> {
    let state = app.state::<AppState>();
    // Сначала то, что известно локально: так список виден и без сети.
    let cached = |state: &AppState| -> R<Vec<SharedMeta>> {
        with_vault(state, |v| {
            let mut out: Vec<SharedMeta> = v.data.shared.iter().map(meta_of).collect();
            out.sort_by(|a, b| b.updated.cmp(&a.updated));
            Ok(out)
        })
    };
    if register(&state).is_err() {
        return cached(&state);
    }
    let Ok(remote) = call(&state, "doc_list", json!({})) else { return cached(&state) };
    let Some(items) = remote.as_array() else { return cached(&state) };

    let remote_ids: HashSet<String> = items.iter().filter_map(|d| d.get("id").and_then(Value::as_str).map(String::from)).collect();
    for d in items {
        let Some(id) = d.get("id").and_then(Value::as_str) else { continue };
        if !valid_uuid(id) {
            continue;
        }
        let role = d.get("role").and_then(Value::as_str).unwrap_or("editor").to_string();
        let epoch = d.get("epoch").and_then(Value::as_u64).unwrap_or(1) as u32;
        let updated = d.get("updated").and_then(Value::as_i64).unwrap_or(0);
        with_vault(&state, |v| {
            if !v.data.shared.iter().any(|s| s.id == id) {
                v.data.shared.push(SharedLocal { id: id.to_string(), role: role.clone(), epoch, joined: true, ..Default::default() });
            }
            if let Some(s) = v.data.shared.iter_mut().find(|s| s.id == id) {
                s.role = role.clone();
                s.epoch = epoch;
                s.joined = true;
                s.updated = updated.max(s.updated);
            }
            Ok(())
        })?;
        if let Some(keys) = d.get("keys").and_then(Value::as_array) {
            absorb_keys(&state, id, keys);
        }
        if let Some(ct) = d.get("metaCt").and_then(Value::as_str) {
            if let Ok(doc) = local(&state, id) {
                if let Some((title, kind)) = decrypt_meta(&doc, epoch, ct) {
                    let _ = with_vault(&state, |v| {
                        if let Some(s) = v.data.shared.iter_mut().find(|s| s.id == id) {
                            s.title = title;
                            s.kind = kind;
                        }
                        Ok(())
                    });
                }
            }
        }
    }
    // Документы, из которых нас удалили или которые удалены, убираем вместе с локальной копией.
    with_vault(&state, |v| {
        let gone: Vec<String> = v.data.shared.iter().filter(|s| !remote_ids.contains(&s.id) && s.joined).map(|s| s.id.clone()).collect();
        for id in &gone {
            let _ = v.remove_blob(&format!("ys-{id}"));
        }
        v.data.shared.retain(|s| remote_ids.contains(&s.id) || !s.joined);
        v.data.outbox.retain(|o| !gone.contains(&o.doc));
        v.save().map_err(|e| e.to_string())
    })?;
    cached(&state)
}

pub fn create(app: &AppHandle, title: &str, kind: &str) -> R<SharedMeta> {
    let kind = if kind == "snippets" { "snippets" } else { "note" };
    let state = app.state::<AppState>();
    register(&state)?;
    let (pk, _) = state.license.lock().kx().map_err(err)?;
    let id = uuid::Uuid::new_v4().to_string();
    let key = crypto::random_key();
    let wrapped = B64.encode(sealed_box::seal_to(&pk, key.as_ref()));
    let meta = encrypt_meta(&key, &id, 1, title, kind);
    call(&state, "doc_create", json!({ "doc_id": id, "wrapped": wrapped, "meta_ct": meta }))?;
    let now = vault::now();
    with_vault(&state, |v| {
        let mut keys = BTreeMap::new();
        keys.insert(1u32, B64.encode(key.as_ref()));
        v.data.shared.push(SharedLocal { id: id.clone(), title: title.to_string(), role: "owner".into(), epoch: 1, keys, last_id: 0, updated: now, joined: true, kind: kind.into(), pinned: false, versioned_at: 0 });
        v.save().map_err(|e| e.to_string())
    })?;
    Ok(SharedMeta { id, title: title.to_string(), role: "owner".into(), updated: now, kind: kind.into(), pinned: false })
}

fn cached_state(state: &AppState, id: &str) -> Vec<u8> {
    with_vault(state, |v| Ok(v.get_blob(&format!("ys-{id}")).map(|b| b.to_vec()).unwrap_or_default())).unwrap_or_default()
}

fn store_state(state: &AppState, id: &str, bytes: &[u8]) {
    let _ = with_vault(state, |v| v.put_blob(&format!("ys-{id}"), bytes).map_err(|e| e.to_string()));
}

/// Забирает с сервера всё новое, расшифровывает и вливает в локальную копию.
/// Возвращает новые правки (для отправки в открытый редактор).
fn pull(app: &AppHandle, id: &str) -> R<Vec<Vec<u8>>> {
    let state = app.state::<AppState>();
    let mut fresh: Vec<Vec<u8>> = Vec::new();
    let mut base: Option<Vec<u8>> = None;
    let mut total: u64;
    loop {
        let doc = local(&state, id)?;
        let v = call(&state, "doc_pull", json!({ "doc_id": id, "after": doc.last_id }))?;
        if let Some(keys) = v.get("keys").and_then(Value::as_array) {
            absorb_keys(&state, id, keys);
        }
        let doc = local(&state, id)?;
        let epoch = v.get("epoch").and_then(Value::as_u64).unwrap_or(doc.epoch as u64) as u32;
        total = v.get("total").and_then(Value::as_u64).unwrap_or(0);
        let mut last = doc.last_id;

        if let Some(snap) = v.get("snapshot").filter(|s| !s.is_null()) {
            let ep = snap.get("epoch").and_then(Value::as_u64).unwrap_or(0) as u32;
            let upto = snap.get("upto").and_then(Value::as_i64).unwrap_or(0);
            if let (Some(key), Some(ct)) = (key_for(&doc, ep), snap.get("ct").and_then(Value::as_str)) {
                if let Ok(plain) = B64.decode(ct).map_err(|_| ()).and_then(|c| crypto::open(&key, &aad("snap", id, ep), &c).map_err(|_| ())) {
                    base = Some(plain.to_vec());
                    fresh.push(plain.to_vec());
                    last = last.max(upto);
                }
            }
        }
        let ups = v.get("updates").and_then(Value::as_array).cloned().unwrap_or_default();
        for u in &ups {
            let uid = u.get("id").and_then(Value::as_i64).unwrap_or(0);
            let ep = u.get("epoch").and_then(Value::as_u64).unwrap_or(0) as u32;
            if let (Some(key), Some(ct)) = (key_for(&doc, ep), u.get("ct").and_then(Value::as_str)) {
                if let Ok(c) = B64.decode(ct) {
                    if let Ok(plain) = crypto::open(&key, &aad("upd", id, ep), &c) {
                        fresh.push(plain.to_vec());
                    }
                }
            }
            last = last.max(uid);
        }
        let title = v.get("metaCt").and_then(Value::as_str).and_then(|ct| decrypt_meta(&doc, epoch, ct));
        with_vault(&state, |vv| {
            if let Some(s) = vv.data.shared.iter_mut().find(|s| s.id == id) {
                s.last_id = last;
                s.epoch = epoch;
                s.joined = true;
                if let Some((t, k)) = title {
                    s.title = t;
                    s.kind = k;
                }
            }
            vv.save().map_err(|e| e.to_string())
        })?;
        if !v.get("more").and_then(Value::as_bool).unwrap_or(false) {
            break;
        }
    }

    if !fresh.is_empty() {
        let mut parts = vec![base.unwrap_or_else(|| cached_state(&state, id))];
        parts.retain(|p| !p.is_empty());
        parts.extend(fresh.iter().cloned());
        if let Some(merged) = merge(&parts) {
            store_state(&state, id, &merged);
        }
    }

    if total > COMPACT_AFTER {
        let _ = compact(app, id);
    }
    Ok(fresh)
}

/// Сворачивает журнал в снимок: новым участникам не нужно скачивать тысячи мелких правок.
fn compact(app: &AppHandle, id: &str) -> R<()> {
    let state = app.state::<AppState>();
    let doc = local(&state, id)?;
    let key = key_for(&doc, doc.epoch).ok_or("key")?;
    let st = cached_state(&state, id);
    if st.is_empty() {
        return Ok(());
    }
    let ct = B64.encode(crypto::seal(&key, &aad("snap", id, doc.epoch), &st));
    call(&state, "doc_snapshot", json!({ "doc_id": id, "epoch": doc.epoch, "upto": doc.last_id, "ct": ct })).map(|_| ())
}

pub fn pull_and_emit(app: &AppHandle, id: &str) -> R<()> {
    let fresh = match pull(app, id) {
        Ok(f) => f,
        Err(e) if e == "forbidden" => {
            forget(app, id);
            let _ = app.emit("shared:removed", json!({ "id": id }));
            return Err(e);
        }
        Err(e) => return Err(e),
    };
    sync_topic(&app.state::<AppState>(), id);
    if !fresh.is_empty() {
        if let Some(m) = merge(&fresh) {
            let _ = app.emit("shared:update", json!({ "id": id, "data": B64.encode(m) }));
        }
        let is_set = app.state::<AppState>().vault.lock().as_ref().is_some_and(|v| v.data.shared.iter().any(|s| s.id == id && s.kind == "snippets"));
        if is_set {
            snippets_changed(app);
        }
    }
    Ok(())
}

pub fn open(app: &AppHandle, id: &str) -> R<Opened> {
    if !valid_uuid(id) {
        return Err("malformed".into());
    }
    let state = app.state::<AppState>();
    let doc = local(&state, id)?;
    state.shared.open.lock().insert(id.to_string());
    sync_topic(&state, id);
    let me = register(&state).unwrap_or_default();
    // Без сети открываем локальную копию; правки уйдут позже.
    let _ = pull(app, id);
    sync_topic(&state, id);
    let mut parts = vec![cached_state(&state, id)];
    // Неотправленные правки тоже часть документа.
    let pending: Vec<Vec<u8>> = with_vault(&state, |v| Ok(v.data.outbox.iter().filter(|o| o.doc == id).filter_map(|o| B64.decode(&o.update).ok()).collect()))?;
    parts.extend(pending);
    parts.retain(|p| !p.is_empty());
    let st = merge(&parts).unwrap_or_default();
    let doc = local(&state, id).unwrap_or(doc);
    Ok(Opened { state: B64.encode(st), role: doc.role, me, name: display_name(&state), online: state.shared.rt.online() })
}

pub fn close(app: &AppHandle, id: &str) {
    let state = app.state::<AppState>();
    state.shared.open.lock().remove(id);
    sync_topic(&state, id);
}

// ——— правки и курсоры ———

pub fn push(app: &AppHandle, id: &str, update_b64: &str) -> R<()> {
    let state = app.state::<AppState>();
    let update = B64.decode(update_b64).map_err(|_| "malformed".to_string())?;
    if update.len() > 8 * 1024 * 1024 {
        return Err("too_large".into());
    }
    let doc = local(&state, id)?;
    with_vault(&state, |v| {
        v.data.outbox.push(OutItem { doc: id.to_string(), update: update_b64.to_string() });
        if let Some(s) = v.data.shared.iter_mut().find(|s| s.id == id) {
            s.updated = vault::now();
        }
        v.save().map_err(|e| e.to_string())
    })?;
    // Мгновенная доставка соавторам; журнал на сервере догонит через flush.
    if let Some(key) = key_for(&doc, doc.epoch) {
        let ct = B64.encode(crypto::seal(&key, &aad("upd", id, doc.epoch), &update));
        if ct.len() <= realtime::MAX_BROADCAST {
            send(&state, id, "u", json!({ "ep": doc.epoch, "ct": ct }));
        } else {
            state.shared.flush_now.store(true, Ordering::SeqCst);
        }
    }
    if doc.kind == "snippets" {
        snippets_changed(app);
    }
    Ok(())
}

/// Сокращения командных сниппетов должны работать сразу после правки.
fn snippets_changed(app: &AppHandle) {
    app.state::<AppState>().configure_expander();
    let _ = app.emit("snippets:changed", ());
}

pub fn awareness(app: &AppHandle, id: &str, data_b64: &str) -> R<()> {
    let state = app.state::<AppState>();
    let data = B64.decode(data_b64).map_err(|_| "malformed".to_string())?;
    let doc = local(&state, id)?;
    let key = key_for(&doc, doc.epoch).ok_or("key")?;
    let ct = B64.encode(crypto::seal(&key, &aad("aw", id, doc.epoch), &data));
    if ct.len() <= 16 * 1024 {
        send(&state, id, "a", json!({ "ep": doc.epoch, "ct": ct }));
    }
    Ok(())
}

/// Отправляет накопленные правки в журнал сервера.
fn flush(app: &AppHandle) {
    let state = app.state::<AppState>();
    let pending: Vec<OutItem> = match state.vault.lock().as_ref() {
        Some(v) => v.data.outbox.clone(),
        None => return,
    };
    if pending.is_empty() {
        return;
    }
    let mut docs: Vec<String> = pending.iter().map(|o| o.doc.clone()).collect();
    docs.dedup();
    let docs: Vec<String> = docs.into_iter().collect::<HashSet<_>>().into_iter().collect();
    for id in docs {
        // Отправляем порциями не больше 4 МБ: сервер не примет слишком большой запрос.
        let mut size = 0usize;
        let items: Vec<&OutItem> = pending
            .iter()
            .filter(|o| o.doc == id)
            .take_while(|o| {
                size += o.update.len();
                size <= 4 * 1024 * 1024 || size == o.update.len()
            })
            .collect();
        let n = items.len();
        let raw: Vec<Vec<u8>> = items.iter().filter_map(|o| B64.decode(&o.update).ok()).collect();
        let Some(merged) = merge(&raw) else {
            // Повреждённые записи не должны блокировать очередь навсегда.
            drop_outbox(&state, &id, n);
            continue;
        };
        let Ok(doc) = local(&state, &id) else {
            drop_outbox(&state, &id, n);
            continue;
        };
        let Some(key) = key_for(&doc, doc.epoch) else { continue };
        let ct = B64.encode(crypto::seal(&key, &aad("upd", &id, doc.epoch), &merged));
        match call(&state, "doc_append", json!({ "doc_id": id, "epoch": doc.epoch, "cts": [ct] })) {
            Ok(_) => {
                drop_outbox(&state, &id, n);
                let mut parts = vec![cached_state(&state, &id)];
                parts.retain(|p| !p.is_empty());
                parts.push(merged);
                if let Some(m) = merge(&parts) {
                    store_state(&state, &id, &m);
                    snapshot_version(&state, &id, &m);
                }
            }
            Err(e) if e == "epoch" => {
                // Ключ сменился: получаем новый и пробуем снова в следующий раз.
                let _ = pull(app, &id);
            }
            Err(e) if e == "forbidden" => {
                // Нас удалили из документа: эти правки больше некуда отправить.
                forget(app, &id);
                let _ = app.emit("shared:removed", json!({ "id": id }));
            }
            Err(_) => {}
        }
    }
}

/// Сохраняет версию общей заметки не чаще раза в 10 минут.
fn snapshot_version(state: &AppState, id: &str, st: &[u8]) {
    let _ = with_vault(state, |v| {
        let Some(s) = v.data.shared.iter().find(|s| s.id == id) else { return Ok(()) };
        if s.kind != "note" || vault::now() - s.versioned_at < 600_000 {
            return Ok(());
        }
        v.put_version(id, st).map_err(|e| e.to_string())?;
        if let Some(s) = v.data.shared.iter_mut().find(|s| s.id == id) {
            s.versioned_at = vault::now();
        }
        v.save().map_err(|e| e.to_string())
    });
}

/// Сниппеты из командных наборов: документ Yjs с картой "snippets".
pub fn team_snippets(state: &AppState) -> Vec<TeamSnippet> {
    use yrs::{Map, updates::decoder::Decode};
    let sets: Vec<(String, String)> = match state.vault.lock().as_ref() {
        Some(v) => v.data.shared.iter().filter(|s| s.kind == "snippets").map(|s| (s.id.clone(), s.title.clone())).collect(),
        None => return vec![],
    };
    let mut out = Vec::new();
    for (set_id, set_title) in sets {
        let mut parts = vec![cached_state(state, &set_id)];
        let pending: Vec<Vec<u8>> = with_vault(state, |v| Ok(v.data.outbox.iter().filter(|o| o.doc == set_id).filter_map(|o| B64.decode(&o.update).ok()).collect())).unwrap_or_default();
        parts.extend(pending);
        parts.retain(|p| !p.is_empty());
        let Some(st) = merge(&parts) else { continue };
        let doc = yrs::Doc::new();
        let map = doc.get_or_insert_map("snippets");
        if let Ok(u) = yrs::Update::decode_v1(&st) {
            let _ = doc.transact_mut().apply_update(u);
        }
        let tx = doc.transact();
        for (key, val) in map.iter(&tx) {
            if let yrs::Out::YMap(m) = val {
                let get = |f: &str| m.get(&tx, f).map(|o| o.to_string(&tx)).unwrap_or_default();
                let tags = get("tags").split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
                out.push(TeamSnippet { id: key.to_string(), set_id: set_id.clone(), set_title: set_title.clone(), title: get("title"), body: get("body"), tags, abbr: get("abbr") });
            }
        }
    }
    out
}

fn any_json(a: &yrs::Any) -> Value {
    use yrs::Any;
    match a {
        Any::Bool(b) => Value::Bool(*b),
        Any::Number(n) => json!(n),
        Any::String(s) => Value::String(s.to_string()),
        Any::Array(xs) => Value::Array(xs.iter().map(any_json).collect()),
        Any::Map(m) => Value::Object(m.iter().map(|(k, v)| (k.clone(), any_json(v))).collect()),
        _ => Value::Null,
    }
}

/// Ищет в общих заметках новые комментарии с упоминанием этого устройства и показывает уведомление.
fn scan_mentions(app: &AppHandle) {
    use yrs::{Map, updates::decoder::Decode};
    let state = app.state::<AppState>();
    let Some(me) = state.license.lock().server_id() else { return };
    let docs: Vec<(String, String)> = match state.vault.lock().as_ref() {
        Some(v) => v.data.shared.iter().filter(|s| s.kind == "note").map(|s| (s.id.clone(), s.title.clone())).collect(),
        None => return,
    };
    let ru = state.settings.read().lang == "ru";
    for (id, title) in docs {
        let _ = pull(app, &id);
        let st = cached_state(&state, &id);
        if st.is_empty() {
            continue;
        }
        let doc = yrs::Doc::new();
        let map = doc.get_or_insert_map("comments");
        if let Ok(u) = yrs::Update::decode_v1(&st) {
            let _ = doc.transact_mut().apply_update(u);
        }
        let mut found: Vec<(i64, String, String)> = Vec::new();
        let mut newest = 0i64;
        {
            let tx = doc.transact();
            for (_, val) in map.iter(&tx) {
                let yrs::Out::Any(any) = val else { continue };
                let c = any_json(&any);
                let mut items = vec![c.clone()];
                if let Some(rs) = c.get("replies").and_then(Value::as_array) {
                    items.extend(rs.iter().cloned());
                }
                for it in items {
                    let at = it.get("at").and_then(Value::as_f64).unwrap_or(0.0) as i64;
                    newest = newest.max(at);
                    let mentioned = it.get("mentions").and_then(Value::as_array).is_some_and(|m| m.iter().any(|x| x.as_str() == Some(me.as_str())));
                    if mentioned {
                        let author = it.get("author").and_then(Value::as_str).unwrap_or("").to_string();
                        let text = it.get("text").and_then(Value::as_str).unwrap_or("").chars().take(140).collect();
                        found.push((at, author, text));
                    }
                }
            }
        }
        let seen = state.vault.lock().as_ref().and_then(|v| v.data.mentions_seen.get(&id).copied());
        let _ = with_vault(&state, |v| {
            v.data.mentions_seen.insert(id.clone(), newest.max(seen.unwrap_or(0)));
            Ok(())
        });
        // Первый просмотр заметки: старые упоминания не показываем.
        let Some(seen) = seen else { continue };
        found.retain(|(at, _, _)| *at > seen);
        found.sort_by_key(|f| f.0);
        for (_, author, text) in found.into_iter().rev().take(3) {
            let head = if ru { format!("{author} упомянул(а) вас в «{title}»") } else { format!("{author} mentioned you in “{title}”") };
            crate::extras::notify(app, &head, &text, Some(id.clone()));
        }
    }
}

/// Держит командные наборы сниппетов в актуальном виде, даже если они не открыты.
fn sync_snippet_sets(app: &AppHandle) {
    let state = app.state::<AppState>();
    let ids: Vec<String> = match state.vault.lock().as_ref() {
        Some(v) => v.data.shared.iter().filter(|s| s.kind == "snippets").map(|s| s.id.clone()).collect(),
        None => return,
    };
    let mut changed = false;
    for id in ids {
        if let Ok(fresh) = pull(app, &id) {
            changed |= !fresh.is_empty();
        }
    }
    if changed {
        snippets_changed(app);
    }
}

pub fn set_pinned(app: &AppHandle, id: &str, pinned: bool) -> R<()> {
    let state = app.state::<AppState>();
    with_vault(&state, |v| {
        if let Some(s) = v.data.shared.iter_mut().find(|s| s.id == id) {
            s.pinned = pinned;
        }
        v.save().map_err(|e| e.to_string())
    })
}

fn drop_outbox(state: &AppState, id: &str, n: usize) {
    let _ = with_vault(state, |v| {
        let mut left = n;
        v.data.outbox.retain(|o| {
            if left > 0 && o.doc == id {
                left -= 1;
                false
            } else {
                true
            }
        });
        v.save().map_err(|e| e.to_string())
    });
}

fn on_realtime(app: &AppHandle, ev: realtime::Event) {
    if app.try_state::<AppState>().is_none() {
        return;
    }
    match ev {
        realtime::Event::Online(on) => {
            let _ = app.emit("shared:online", on);
            if on {
                let state = app.state::<AppState>();
                state.shared.flush_now.store(true, Ordering::SeqCst);
                let ids: Vec<String> = state.shared.open.lock().iter().cloned().collect();
                let h = app.clone();
                std::thread::spawn(move || {
                    for id in ids {
                        let _ = pull_and_emit(&h, &id);
                    }
                });
            }
        }
        realtime::Event::Message { topic, event, payload } => {
            let state = app.state::<AppState>();
            let found = state.shared.topics.lock().iter().find(|(_, t)| **t == topic).map(|(i, _)| i.clone());
            let Some(id) = found else { return };
            let id = id.as_str();
            if !state.shared.open.lock().contains(id) {
                return;
            }
            let Ok(doc) = local(&state, id) else { return };
            if event == "k" {
                // Владелец сменил ключ: забираем новый (или узнаём, что доступ закрыт).
                let h = app.clone();
                let id = id.to_string();
                std::thread::spawn(move || {
                    let _ = pull_and_emit(&h, &id);
                });
                return;
            }
            let ep = payload.get("ep").and_then(Value::as_u64).unwrap_or(0) as u32;
            let Some(ct) = payload.get("ct").and_then(Value::as_str).and_then(|c| B64.decode(c).ok()) else { return };
            let Some(key) = key_for(&doc, ep) else {
                // Неизвестная эпоха: ключ сменился, забираем новый с сервера.
                let h = app.clone();
                let id = id.to_string();
                std::thread::spawn(move || {
                    let _ = pull_and_emit(&h, &id);
                });
                return;
            };
            let kind = if event == "a" { "aw" } else { "upd" };
            let Ok(plain) = crypto::open(&key, &aad(kind, id, ep), &ct) else { return };
            let name = if event == "a" { "shared:awareness" } else { "shared:update" };
            let _ = app.emit(name, json!({ "id": id, "data": B64.encode(&plain[..]) }));
        }
    }
}

// ——— название ———

pub fn set_title(app: &AppHandle, id: &str, title: &str) -> R<()> {
    let state = app.state::<AppState>();
    let title: String = title.chars().take(200).collect();
    let doc = local(&state, id)?;
    if doc.title == title {
        return Ok(());
    }
    with_vault(&state, |v| {
        if let Some(s) = v.data.shared.iter_mut().find(|s| s.id == id) {
            s.title = title.clone();
            s.updated = vault::now();
        }
        v.save().map_err(|e| e.to_string())
    })?;
    if let Some(key) = key_for(&doc, doc.epoch) {
        let _ = call(&state, "doc_meta", json!({ "doc_id": id, "epoch": doc.epoch, "meta_ct": encrypt_meta(&key, id, doc.epoch, &title, &doc.kind) }));
    }
    Ok(())
}

// ——— приглашения ———

/// Создаёт одноразовую ссылку. Секрет ссылки не уходит на сервер: туда попадает только его хэш
/// и ключ документа, зашифрованный ключом, выведенным из секрета.
pub fn invite(app: &AppHandle, id: &str) -> R<String> {
    let state = app.state::<AppState>();
    let doc = local(&state, id)?;
    if doc.role != "owner" {
        return Err("forbidden".into());
    }
    let _ = pull(app, id);
    let doc = local(&state, id)?;
    let key = key_for(&doc, doc.epoch).ok_or("key")?;
    let secret = Zeroizing::new(crypto::random::<32>());
    let wrap_key = crypto::derive(secret.as_ref(), &format!("invite/{id}"));
    let wrapped = B64.encode(crypto::seal(&wrap_key, &aad("invite", id, doc.epoch), key.as_ref()));
    let token_hash = crypto::sha256_hex(secret.as_ref());
    call(&state, "invite_create", json!({ "doc_id": id, "epoch": doc.epoch, "token_hash": token_hash, "wrapped": wrapped, "hours": 72 }))?;
    Ok(format!("{INVITE_PREFIX}{id}/{}", B64URL.encode(secret.as_ref())))
}

pub fn join(app: &AppHandle, link: &str) -> R<String> {
    let state = app.state::<AppState>();
    let link = link.trim();
    let rest = link.find(INVITE_PREFIX).map(|i| &link[i + INVITE_PREFIX.len()..]).ok_or("link")?;
    let rest = rest.split_whitespace().next().unwrap_or_default();
    let (id, secret_b64) = rest.split_once('/').ok_or("link")?;
    if !valid_uuid(id) {
        return Err("link".into());
    }
    let secret = Zeroizing::new(B64URL.decode(secret_b64.trim_end_matches('/')).map_err(|_| "link".to_string())?);
    if secret.len() != 32 {
        return Err("link".into());
    }
    if local(&state, id).map(|d| d.joined).unwrap_or(false) {
        return Err("already_member".into());
    }
    register(&state)?;
    let (pk, _) = state.license.lock().kx().map_err(err)?;
    let v = call(
        &state,
        "invite_accept",
        json!({ "doc_id": id, "token_hash": crypto::sha256_hex(&secret), "kx_pub": B64.encode(pk), "name": display_name(&state) }),
    )?;
    let epoch = v.get("epoch").and_then(Value::as_u64).ok_or("invite")? as u32;
    let wrapped = B64.decode(v.get("wrapped").and_then(Value::as_str).ok_or("invite")?).map_err(|_| "invite".to_string())?;
    let wrap_key = crypto::derive(&secret, &format!("invite/{id}"));
    let key = crypto::open(&wrap_key, &aad("invite", id, epoch), &wrapped).map_err(|_| "invite".to_string())?;
    if key.len() != 32 {
        return Err("invite".into());
    }
    // Сохраняем ключ для себя, чтобы открывать документ без ссылки.
    let mine = B64.encode(sealed_box::seal_to(&pk, &key));
    call(&state, "key_put", json!({ "doc_id": id, "epoch": epoch, "wrapped": mine }))?;
    with_vault(&state, |vv| {
        vv.data.shared.retain(|s| s.id != id);
        let mut keys = BTreeMap::new();
        keys.insert(epoch, B64.encode(&key[..]));
        vv.data.shared.push(SharedLocal { id: id.to_string(), title: String::new(), role: "editor".into(), epoch, keys, last_id: 0, updated: vault::now(), joined: true, kind: "note".into(), pinned: false, versioned_at: 0 });
        vv.save().map_err(|e| e.to_string())
    })?;
    let _ = pull(app, id);
    Ok(id.to_string())
}

// ——— участники ———

pub fn members(app: &AppHandle, id: &str) -> R<Vec<Member>> {
    let state = app.state::<AppState>();
    let v = call(&state, "doc_members", json!({ "doc_id": id }))?;
    Ok(v.as_array()
        .map(|a| {
            a.iter()
                .map(|m| Member {
                    device: m.get("device").and_then(Value::as_str).unwrap_or_default().to_string(),
                    name: m.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
                    role: m.get("role").and_then(Value::as_str).unwrap_or("editor").to_string(),
                    me: m.get("me").and_then(Value::as_bool).unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default())
}

/// Удаляет участника и меняет ключ документа, чтобы он не мог читать новые правки.
pub fn remove_member(app: &AppHandle, id: &str, device: &str) -> R<()> {
    let state = app.state::<AppState>();
    let doc = local(&state, id)?;
    if doc.role != "owner" {
        return Err("forbidden".into());
    }
    // Сначала отправляем свои правки и забираем чужие: снимок должен быть полным.
    flush(app);
    pull(app, id)?;
    let doc = local(&state, id)?;
    let list = call(&state, "doc_members", json!({ "doc_id": id }))?;
    let new_epoch = doc.epoch + 1;
    let new_key = crypto::random_key();
    let mut keys = Vec::new();
    for m in list.as_array().cloned().unwrap_or_default() {
        let dev = m.get("device").and_then(Value::as_str).unwrap_or_default();
        if dev == device {
            continue;
        }
        let pk: [u8; 32] = m
            .get("kxPub")
            .and_then(Value::as_str)
            .and_then(|s| B64.decode(s).ok())
            .and_then(|v| v.try_into().ok())
            .ok_or("member_key")?;
        keys.push(json!({ "device": dev, "wrapped": B64.encode(sealed_box::seal_to(&pk, new_key.as_ref())) }));
    }
    let st = cached_state(&state, id);
    let st = if st.is_empty() { merge(&[]).unwrap_or_default() } else { st };
    let snapshot = B64.encode(crypto::seal(&new_key, &aad("snap", id, new_epoch), &st));
    let meta = encrypt_meta(&new_key, id, new_epoch, &doc.title, &doc.kind);
    call(
        &state,
        "member_remove",
        json!({ "doc_id": id, "device": device, "epoch": new_epoch, "keys": keys, "snapshot": snapshot, "upto": doc.last_id, "meta_ct": meta }),
    )?;
    with_vault(&state, |v| {
        if let Some(s) = v.data.shared.iter_mut().find(|s| s.id == id) {
            s.epoch = new_epoch;
            s.keys.insert(new_epoch, B64.encode(new_key.as_ref()));
        }
        v.save().map_err(|e| e.to_string())
    })?;
    // Сообщаем соавторам в старом канале, что пора забрать новый ключ,
    // и сами переходим в канал новой эпохи.
    send(&state, id, "k", json!({}));
    sync_topic(&state, id);
    Ok(())
}

/// Убирает документ с этого компьютера (нас удалили или документ удалён).
fn forget(app: &AppHandle, id: &str) {
    let state = app.state::<AppState>();
    close(app, id);
    let _ = with_vault(&state, |v| {
        v.data.shared.retain(|s| s.id != id);
        v.data.outbox.retain(|o| o.doc != id);
        let _ = v.remove_blob(&format!("ys-{id}"));
        v.save().map_err(|e| e.to_string())
    });
}

pub fn leave(app: &AppHandle, id: &str) -> R<()> {
    let state = app.state::<AppState>();
    close(app, id);
    call(&state, "doc_leave", json!({ "doc_id": id }))?;
    with_vault(&state, |v| {
        v.data.shared.retain(|s| s.id != id);
        v.data.outbox.retain(|o| o.doc != id);
        let _ = v.remove_blob(&format!("ys-{id}"));
        v.save().map_err(|e| e.to_string())
    })
}

// ——— изображения ———

fn img_local_id(blob: &str) -> String {
    format!("img-{blob}")
}

/// Сохраняет изображение у себя и отправляет его участникам в зашифрованном виде.
pub fn image_put(app: &AppHandle, id: &str, data_url: &str) -> R<String> {
    let state = app.state::<AppState>();
    let (_, b64) = data_url.split_once(";base64,").ok_or("format")?;
    let bytes = B64.decode(b64).map_err(|_| "format".to_string())?;
    let img = image::load_from_memory(&bytes).map_err(|_| "format".to_string())?;
    let img = if img.width() > 2400 { img.resize(2400, 2400 * img.height() / img.width().max(1), image::imageops::FilterType::Lanczos3) } else { img };
    let png = crate::clip::png_from_image(&img).ok_or("format")?;
    if png.len() > 8 * 1024 * 1024 {
        return Err("too_large".into());
    }
    let blob = uuid::Uuid::new_v4().to_string();
    with_vault(&state, |v| v.put_blob(&img_local_id(&blob), &png).map_err(|e| e.to_string()))?;
    let doc = local(&state, id)?;
    if let Some(key) = key_for(&doc, doc.epoch) {
        let ct = B64.encode(crypto::seal(&key, &aad(&format!("img/{blob}"), id, doc.epoch), &png));
        let h = app.clone();
        let id = id.to_string();
        let b = blob.clone();
        // Загрузка в фоне: изображение уже видно у автора.
        std::thread::spawn(move || {
            let st = h.state::<AppState>();
            let _ = call(&st, "blob_put", json!({ "doc_id": id, "blob_id": b, "ct": ct }));
        });
    }
    Ok(format!("clavis-img:{blob}"))
}

pub fn image_get(app: &AppHandle, id: &str, blob: &str) -> R<String> {
    if !valid_uuid(blob) {
        return Err("malformed".into());
    }
    let state = app.state::<AppState>();
    let local_bytes = with_vault(&state, |v| Ok(v.get_blob(&img_local_id(blob)).ok().map(|b| b.to_vec())))?;
    let png = match local_bytes {
        Some(b) => b,
        None => {
            let v = call(&state, "blob_get", json!({ "doc_id": id, "blob_id": blob }))?;
            let ep = v.get("epoch").and_then(Value::as_u64).unwrap_or(0) as u32;
            let doc = local(&state, id)?;
            let key = key_for(&doc, ep).ok_or("key")?;
            let ct = B64.decode(v.get("ct").and_then(Value::as_str).ok_or("missing")?).map_err(|_| "format".to_string())?;
            let png = crypto::open(&key, &aad(&format!("img/{blob}"), id, ep), &ct).map_err(|_| "format".to_string())?.to_vec();
            with_vault(&state, |vv| vv.put_blob(&img_local_id(blob), &png).map_err(|e| e.to_string()))?;
            let _ = call(&state, "blob_ack", json!({ "doc_id": id, "blob_id": blob }));
            png
        }
    };
    Ok(format!("data:image/png;base64,{}", B64.encode(png)))
}

pub fn online(app: &AppHandle) -> bool {
    app.state::<AppState>().shared.online()
}

#[cfg(test)]
mod tests {
    use super::*;
    use yrs::{GetString, Text};

    #[test]
    fn merge_roundtrip() {
        let a = yrs::Doc::with_client_id(1);
        let t = a.get_or_insert_text("t");
        let u1 = {
            let mut tx = a.transact_mut();
            t.insert(&mut tx, 0, "привет");
            tx.encode_update_v1()
        };
        let u2 = {
            let mut tx = a.transact_mut();
            t.insert(&mut tx, 6, ", мир");
            tx.encode_update_v1()
        };
        let m = merge(&[u1, u2]).unwrap();
        let b = yrs::Doc::new();
        let tb = b.get_or_insert_text("t");
        {
            let mut tx = b.transact_mut();
            tx.apply_update(yrs::Update::decode_v1(&m).unwrap()).unwrap();
        }
        assert_eq!(tb.get_string(&b.transact()), "привет, мир");
        assert!(!merge(&[]).unwrap().is_empty());
    }
}
