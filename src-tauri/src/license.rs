//! Активация и связь с сервером.
//!
//! Устройство один раз создаёт ключ Ed25519; закрытая часть защищена TPM/DPAPI.
//! Каждый запрос к серверу подписан этим ключом, так что подделать запрос
//! от чужого имени нельзя. Сервер в ответ на активацию выдаёт лицензию,
//! подписанную своим ключом; её открытая часть зашита в приложение.

use crate::device;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use zeroize::Zeroizing;

pub const SERVER_URL: &str = "https://zjoctahbygocbbzeqvor.supabase.co/functions/v1/clavis";
pub const SERVER_PK: &str = "oeQfz8Ebbt/46aMR2CTPBoyzMCs2rYFpEARFpba6Mdc=";
pub const MAX_CODE_ATTEMPTS: u32 = 15;

#[derive(Debug, thiserror::Error)]
pub enum LicenseError {
    #[error("нет связи с сервером")]
    Network,
    #[error("часы компьютера идут неверно, проверьте дату и время")]
    Clock,
    #[error("сервер ответил ошибкой: {0}")]
    Server(String),
    #[error("ответ сервера не прошёл проверку подписи")]
    BadSignature,
    #[error(transparent)]
    Device(#[from] device::DeviceError),
    #[error("ошибка диска")]
    Io(#[from] std::io::Error),
}

#[derive(Serialize, Deserialize, Default, Clone)]
struct Store {
    device_pk: String,
    device_sk: String,
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    license_sig: Option<String>,
    #[serde(default)]
    blocked: bool,
    #[serde(default)]
    revoked: bool,
    #[serde(default)]
    attempts_left: Option<u32>,
    /// Ключ X25519 устройства: им открываются ключи общих документов.
    #[serde(default)]
    kx_pk: Option<String>,
    #[serde(default)]
    kx_sk: Option<String>,
    /// Идентификатор устройства на сервере и имя, под которым оно зарегистрировано.
    #[serde(default)]
    server_id: Option<String>,
    #[serde(default)]
    registered_name: Option<String>,
}

#[derive(Deserialize)]
struct LicenseBody {
    pubkey: String,
    hw: String,
    owner: bool,
}

pub struct License {
    path: PathBuf,
    store: Store,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum State {
    NotActivated,
    Active { owner: bool },
    Blocked,
    Revoked,
}

fn agent() -> ureq::Agent {
    let tls = native_tls::TlsConnector::new().ok();
    let mut b = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(4))
        .timeout(Duration::from_secs(20))
        .user_agent("Clavis");
    if let Some(t) = tls {
        b = b.tls_connector(std::sync::Arc::new(t));
    }
    b.build()
}

pub fn post(body: &Value) -> Result<Value, LicenseError> {
    match agent().post(SERVER_URL).send_json(body) {
        Ok(r) => r.into_json().map_err(|_| LicenseError::Network),
        Err(ureq::Error::Status(_, r)) => {
            let v: Value = r.into_json().unwrap_or(Value::Null);
            let e = v.get("error").and_then(|e| e.as_str()).unwrap_or("unknown").to_string();
            if e == "clock" {
                Err(LicenseError::Clock)
            } else {
                Err(LicenseError::Server(e))
            }
        }
        Err(_) => Err(LicenseError::Network),
    }
}

impl License {
    pub fn load(dir: &Path) -> Result<License, LicenseError> {
        fs::create_dir_all(dir)?;
        let path = dir.join("device.json");
        let store = match fs::read(&path).ok().and_then(|b| serde_json::from_slice::<Store>(&b).ok()) {
            Some(s) => s,
            None => {
                let sk = SigningKey::generate(&mut rand::rngs::OsRng);
                let s = Store {
                    device_pk: B64.encode(sk.verifying_key().to_bytes()),
                    device_sk: B64.encode(device::protect(&sk.to_bytes())?),
                    ..Default::default()
                };
                fs::write(&path, serde_json::to_vec_pretty(&s).unwrap_or_default())?;
                s
            }
        };
        Ok(License { path, store })
    }

    fn save(&self) {
        let _ = fs::write(&self.path, serde_json::to_vec_pretty(&self.store).unwrap_or_default());
    }

    fn signing_key(&self) -> Result<SigningKey, LicenseError> {
        let blob = B64.decode(&self.store.device_sk).map_err(|_| LicenseError::BadSignature)?;
        let raw = device::unprotect(&blob)?;
        let arr: Zeroizing<[u8; 32]> = Zeroizing::new(raw.as_slice().try_into().map_err(|_| LicenseError::BadSignature)?);
        Ok(SigningKey::from_bytes(&arr))
    }

    pub fn state(&self) -> State {
        if self.store.blocked {
            return State::Blocked;
        }
        if self.store.revoked {
            return State::Revoked;
        }
        match self.verified() {
            Some(owner) => State::Active { owner },
            None => State::NotActivated,
        }
    }

    pub fn attempts_left(&self) -> u32 {
        self.store.attempts_left.unwrap_or(MAX_CODE_ATTEMPTS)
    }

    /// Лицензия действительна, только если подпись сервера верна и она выдана
    /// именно этому ключу на этом железе.
    fn verified(&self) -> Option<bool> {
        let lic = self.store.license.as_ref()?;
        let sig = B64.decode(self.store.license_sig.as_ref()?).ok()?;
        let pk = VerifyingKey::from_bytes(&B64.decode(SERVER_PK).ok()?.try_into().ok()?).ok()?;
        pk.verify(lic.as_bytes(), &Signature::from_slice(&sig).ok()?).ok()?;
        let body: LicenseBody = serde_json::from_str(lic).ok()?;
        (body.pubkey == self.store.device_pk && body.hw == device::hw_hash()).then_some(body.owner)
    }

    fn envelope(&self, op: &str, payload: Value) -> Result<Value, LicenseError> {
        let sk = self.signing_key()?;
        let ts = crate::vault::now();
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        let payload = payload.to_string();
        let msg = format!("{op}\n{ts}\n{nonce}\n{payload}");
        let sig = sk.sign(msg.as_bytes());
        Ok(json!({ "op": op, "pubkey": self.store.device_pk, "ts": ts, "nonce": nonce, "payload": payload, "sig": B64.encode(sig.to_bytes()) }))
    }


    pub fn activate(&mut self, code: &str) -> Result<(bool, u32), LicenseError> {
        if self.store.blocked {
            return Ok((false, 0));
        }
        let env = self.envelope("activate", json!({ "code": code, "hw": device::hw_hash(), "name": device::machine_name() }))?;
        let v = post(&env)?;
        if v.get("ok").and_then(Value::as_bool) == Some(true) {
            let lic = v.get("license").and_then(Value::as_str).unwrap_or_default().to_string();
            let sig = v.get("licenseSig").and_then(Value::as_str).unwrap_or_default().to_string();
            self.store.license = Some(lic);
            self.store.license_sig = Some(sig);
            self.store.attempts_left = None;
            if self.verified().is_none() {
                self.store.license = None;
                self.store.license_sig = None;
                self.save();
                return Err(LicenseError::BadSignature);
            }
            self.save();
            return Ok((true, MAX_CODE_ATTEMPTS));
        }
        let left = v.get("attemptsLeft").and_then(Value::as_u64).unwrap_or(0) as u32;
        self.store.attempts_left = Some(left);
        if v.get("blocked").and_then(Value::as_bool) == Some(true) || left == 0 {
            self.store.blocked = true;
        }
        self.save();
        Ok((false, left))
    }

    pub fn access_request(&self, name: &str, message: &str) -> Result<Value, LicenseError> {
        self.envelope("request", json!({ "name": name, "message": message, "hw": device::hw_hash() }))
    }


    /// Готовит подписанный запрос, не отправляя его: так сеть не держит замок лицензии.
    pub fn signed(&self, op: &str, payload: Value) -> Result<Value, LicenseError> {
        if !matches!(self.state(), State::Active { .. }) {
            return Err(LicenseError::Server("forbidden".into()));
        }
        self.envelope(op, payload)
    }

    /// Открытый и закрытый ключи X25519 устройства; создаются при первом обращении.
    pub fn kx(&mut self) -> Result<([u8; 32], Zeroizing<[u8; 32]>), LicenseError> {
        if self.store.kx_pk.is_none() || self.store.kx_sk.is_none() {
            let (sk, pk) = crate::crypto::sealed_box::keypair();
            self.store.kx_pk = Some(B64.encode(pk));
            self.store.kx_sk = Some(B64.encode(device::protect(sk.as_ref())?));
            self.store.server_id = None;
            self.save();
        }
        let pk: [u8; 32] = B64.decode(self.store.kx_pk.as_ref().unwrap()).ok().and_then(|v| v.try_into().ok()).ok_or(LicenseError::BadSignature)?;
        let raw = device::unprotect(&B64.decode(self.store.kx_sk.as_ref().unwrap()).map_err(|_| LicenseError::BadSignature)?)?;
        let sk: Zeroizing<[u8; 32]> = Zeroizing::new(raw.as_slice().try_into().map_err(|_| LicenseError::BadSignature)?);
        Ok((pk, sk))
    }

    /// Если ключ X25519 и имя уже зарегистрированы на сервере, возвращает id устройства,
    /// иначе готовит подписанный запрос регистрации.
    pub fn registration(&mut self, name: &str) -> Result<Result<String, Value>, LicenseError> {
        let (pk, _) = self.kx()?;
        if let (Some(id), Some(n)) = (&self.store.server_id, &self.store.registered_name) {
            if n == name {
                return Ok(Ok(id.clone()));
            }
        }
        Ok(Err(self.signed("register_kx", json!({ "kx_pub": B64.encode(pk), "name": name }))?))
    }

    /// Идентификатор устройства на сервере, если оно уже зарегистрировано.
    pub fn server_id(&self) -> Option<String> {
        self.store.server_id.clone()
    }

    pub fn set_registered(&mut self, id: &str, name: &str) {
        self.store.server_id = Some(id.to_string());
        self.store.registered_name = Some(name.to_string());
        self.save();
    }

    /// Запрос на сервер о состоянии устройства (см. `apply_startup`).
    pub fn startup_request(&self) -> Option<Value> {
        if matches!(self.state(), State::NotActivated) {
            Some(json!({ "op": "status", "hw": device::hw_hash() }))
        } else {
            self.envelope("check", json!({})).ok()
        }
    }

    /// Применяет ответ сервера при запуске: попытки, блокировку, отзыв устройства.
    pub fn apply_startup(&mut self, v: &Value) {
        if matches!(self.state(), State::NotActivated) {
            if v.get("blocked").and_then(Value::as_bool) == Some(true) {
                self.store.blocked = true;
            }
            if let Some(n) = v.get("attemptsLeft").and_then(Value::as_u64) {
                self.store.attempts_left = Some(n as u32);
            }
        } else if v.get("revoked").and_then(Value::as_bool) == Some(true) && self.store.license.is_some() {
            self.store.revoked = true;
        } else if let (Some(lic), Some(sig)) = (v.get("license").and_then(Value::as_str), v.get("licenseSig").and_then(Value::as_str)) {
            // Новая лицензия принимается, только если подпись сервера верна и она выдана этому устройству.
            let (old_l, old_s) = (self.store.license.replace(lic.to_string()), self.store.license_sig.replace(sig.to_string()));
            if self.verified().is_none() {
                self.store.license = old_l;
                self.store.license_sig = old_s;
            }
        }
        self.save();
    }

    pub fn owner_request(&self, action: &str, mut args: Value) -> Result<Value, LicenseError> {
        if !matches!(self.state(), State::Active { owner: true }) {
            return Err(LicenseError::Server("forbidden".into()));
        }
        if let Value::Object(ref mut m) = args {
            m.insert("action".into(), Value::String(action.into()));
        }
        self.envelope("owner", args)
    }

}
