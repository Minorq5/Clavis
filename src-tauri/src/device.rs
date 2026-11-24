//! Привязка к компьютеру.
//!
//! Секрет устройства шифруется ключом RSA, который живёт внутри чипа TPM
//! и не может быть из него извлечён, а результат дополнительно защищается
//! DPAPI текущего пользователя Windows. Если TPM нет, остаётся только DPAPI.

use zeroize::Zeroizing;

#[derive(Debug, thiserror::Error)]
pub enum DeviceError {
    #[error("не удалось защитить данные на этом компьютере")]
    Protect,
    #[error("данные защищены на другом компьютере или для другого пользователя")]
    Unprotect,
}

const TPM: u8 = 1;
const PLAIN: u8 = 0;

pub fn protect(data: &[u8]) -> Result<Vec<u8>, DeviceError> {
    let (flag, inner) = match imp::tpm_encrypt(data) {
        Some(ct) => (TPM, ct),
        None => (PLAIN, data.to_vec()),
    };
    let outer = imp::dpapi_protect(&inner).ok_or(DeviceError::Protect)?;
    let mut out = vec![1u8, flag];
    out.extend(outer);
    Ok(out)
}

pub fn unprotect(blob: &[u8]) -> Result<Zeroizing<Vec<u8>>, DeviceError> {
    if blob.len() < 3 || blob[0] != 1 {
        return Err(DeviceError::Unprotect);
    }
    let inner = imp::dpapi_unprotect(&blob[2..]).ok_or(DeviceError::Unprotect)?;
    match blob[1] {
        TPM => imp::tpm_decrypt(&inner).ok_or(DeviceError::Unprotect),
        PLAIN => Ok(inner),
        _ => Err(DeviceError::Unprotect),
    }
}

/// Отпечаток железа: не меняется при переустановке Windows.
pub fn hw_hash() -> String {
    let id = imp::hardware_id();
    crate::crypto::sha256_hex(format!("clavis-hw:{id}").as_bytes())
}

pub fn machine_name() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_else(|_| "PC".into())
}

/// Почему не удалось открыть ключ PIN в TPM.
#[derive(Debug, PartialEq)]
pub enum GuardError {
    /// TPM временно блокирует попытки после серии неверных PIN.
    Locked,
    /// Ключа больше нет (TPM очищен или ключ удалён).
    Missing,
    /// Неверный PIN или другая ошибка.
    Failed,
}

/// Создаёт в TPM ключ, которым можно пользоваться только с `auth`, и
/// шифрует им `secret`. Неверные попытки считает сам чип и после серии
/// ошибок блокирует ключ на время, поэтому подбор PIN по копии файлов
/// упирается в TPM. Перед возвратом проверяет, что расшифровка работает.
pub fn guard_create(name: &str, auth: &str, secret: &[u8]) -> Option<Vec<u8>> {
    imp::guard_create(name, auth, secret)
}

pub fn guard_open(name: &str, auth: &str, wrapped: &[u8]) -> Result<Zeroizing<Vec<u8>>, GuardError> {
    imp::guard_open(name, auth, wrapped)
}

pub fn guard_delete(name: &str) {
    imp::guard_delete(name)
}

#[cfg(windows)]
mod imp {
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{HLOCAL, LocalFree};
    use windows::Win32::Security::Cryptography::*;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
    use windows::Win32::System::SystemInformation::{GetSystemFirmwareTable, FIRMWARE_TABLE_PROVIDER};
    use super::GuardError;
    use zeroize::Zeroizing;

    const KEY_NAME: PCWSTR = w!("Clavis Device Key");
    const ENTROPY: &[u8] = b"clavis/device/v1";

    pub fn dpapi_protect(data: &[u8]) -> Option<Vec<u8>> {
        unsafe {
            let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
            let entropy = CRYPT_INTEGER_BLOB { cbData: ENTROPY.len() as u32, pbData: ENTROPY.as_ptr() as *mut u8 };
            let mut out = CRYPT_INTEGER_BLOB::default();
            CryptProtectData(&input, PCWSTR::null(), Some(&entropy), None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out).ok()?;
            let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
            LocalFree(Some(HLOCAL(out.pbData as *mut _)));
            Some(v)
        }
    }

    pub fn dpapi_unprotect(data: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
        unsafe {
            let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
            let entropy = CRYPT_INTEGER_BLOB { cbData: ENTROPY.len() as u32, pbData: ENTROPY.as_ptr() as *mut u8 };
            let mut out = CRYPT_INTEGER_BLOB::default();
            CryptUnprotectData(&input, None, Some(&entropy), None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out).ok()?;
            let v = Zeroizing::new(std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec());
            std::ptr::write_bytes(out.pbData, 0, out.cbData as usize);
            LocalFree(Some(HLOCAL(out.pbData as *mut _)));
            Some(v)
        }
    }

    struct Handle(NCRYPT_HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                let _ = NCryptFreeObject(self.0);
            }
        }
    }

    fn tpm_key(create: bool) -> Option<(Handle, Handle)> {
        unsafe {
            let mut prov = NCRYPT_PROV_HANDLE::default();
            NCryptOpenStorageProvider(&mut prov, MS_PLATFORM_CRYPTO_PROVIDER, 0).ok()?;
            let prov_h = Handle(NCRYPT_HANDLE(prov.0));
            let mut key = NCRYPT_KEY_HANDLE::default();
            if NCryptOpenKey(prov, &mut key, KEY_NAME, CERT_KEY_SPEC(0), NCRYPT_SILENT_FLAG).is_err() {
                if !create {
                    return None;
                }
                NCryptCreatePersistedKey(prov, &mut key, BCRYPT_RSA_ALGORITHM, KEY_NAME, CERT_KEY_SPEC(0), NCRYPT_FLAGS(0)).ok()?;
                if NCryptFinalizeKey(key, NCRYPT_SILENT_FLAG).is_err() {
                    let _ = NCryptFreeObject(NCRYPT_HANDLE(key.0));
                    return None;
                }
            }
            Some((prov_h, Handle(NCRYPT_HANDLE(key.0))))
        }
    }

    fn oaep() -> BCRYPT_OAEP_PADDING_INFO {
        BCRYPT_OAEP_PADDING_INFO { pszAlgId: BCRYPT_SHA256_ALGORITHM, pbLabel: std::ptr::null_mut(), cbLabel: 0 }
    }

    pub fn tpm_encrypt(data: &[u8]) -> Option<Vec<u8>> {
        let (_p, key) = tpm_key(true)?;
        let key = NCRYPT_KEY_HANDLE(key.0 .0);
        let pad = oaep();
        unsafe {
            let mut size = 0u32;
            NCryptEncrypt(key, Some(data), Some(&pad as *const _ as *const _), None, &mut size, NCRYPT_PAD_OAEP_FLAG).ok()?;
            let mut out = vec![0u8; size as usize];
            NCryptEncrypt(key, Some(data), Some(&pad as *const _ as *const _), Some(&mut out), &mut size, NCRYPT_PAD_OAEP_FLAG).ok()?;
            out.truncate(size as usize);
            // Проверяем, что расшифровка работает, прежде чем полагаться на TPM.
            let back = tpm_decrypt(&out)?;
            if back.as_slice() != data {
                return None;
            }
            Some(out)
        }
    }

    pub fn tpm_decrypt(data: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
        let (_p, key) = tpm_key(false)?;
        let key = NCRYPT_KEY_HANDLE(key.0 .0);
        let pad = oaep();
        unsafe {
            let mut size = 0u32;
            NCryptDecrypt(key, Some(data), Some(&pad as *const _ as *const _), None, &mut size, NCRYPT_PAD_OAEP_FLAG | NCRYPT_SILENT_FLAG).ok()?;
            let mut out = Zeroizing::new(vec![0u8; size as usize]);
            NCryptDecrypt(key, Some(data), Some(&pad as *const _ as *const _), Some(&mut out), &mut size, NCRYPT_PAD_OAEP_FLAG | NCRYPT_SILENT_FLAG).ok()?;
            out.truncate(size as usize);
            Some(out)
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    fn provider() -> Option<(NCRYPT_PROV_HANDLE, Handle)> {
        unsafe {
            let mut prov = NCRYPT_PROV_HANDLE::default();
            NCryptOpenStorageProvider(&mut prov, MS_PLATFORM_CRYPTO_PROVIDER, 0).ok()?;
            Some((prov, Handle(NCRYPT_HANDLE(prov.0))))
        }
    }

    unsafe fn set_auth(key: NCRYPT_KEY_HANDLE, auth: &str) -> bool {
        let w = Zeroizing::new(wide(auth));
        let bytes = std::slice::from_raw_parts(w.as_ptr() as *const u8, w.len() * 2);
        NCryptSetProperty(NCRYPT_HANDLE(key.0), NCRYPT_PIN_PROPERTY, bytes, NCRYPT_FLAGS(0)).is_ok()
    }

    fn classify(e: &windows::core::Error) -> GuardError {
        match e.code().0 as u32 {
            // TPM_20_E_LOCKOUT, TPM_E_DEFEND_LOCK_RUNNING, TPM_20_E_RETRY
            0x8028_0921 | 0x8028_0803 | 0x8028_0922 => GuardError::Locked,
            // NTE_BAD_KEYSET, NTE_NO_KEY
            0x8009_0016 | 0x8009_000D => GuardError::Missing,
            _ => GuardError::Failed,
        }
    }

    pub fn guard_create(name: &str, auth: &str, secret: &[u8]) -> Option<Vec<u8>> {
        let ct = unsafe {
            let (prov, _p) = provider()?;
            let wname = wide(name);
            let mut key = NCRYPT_KEY_HANDLE::default();
            NCryptCreatePersistedKey(prov, &mut key, BCRYPT_RSA_ALGORITHM, PCWSTR(wname.as_ptr()), CERT_KEY_SPEC(0), NCRYPT_OVERWRITE_KEY_FLAG).ok()?;
            if !set_auth(key, auth) || NCryptFinalizeKey(key, NCRYPT_SILENT_FLAG).is_err() {
                if NCryptDeleteKey(key, NCRYPT_SILENT_FLAG.0).is_err() {
                    let _ = NCryptFreeObject(NCRYPT_HANDLE(key.0));
                }
                return None;
            }
            let _k = Handle(NCRYPT_HANDLE(key.0));
            let pad = oaep();
            let mut size = 0u32;
            let mut out = Vec::new();
            let ok = NCryptEncrypt(key, Some(secret), Some(&pad as *const _ as *const _), None, &mut size, NCRYPT_PAD_OAEP_FLAG).is_ok() && {
                out = vec![0u8; size as usize];
                NCryptEncrypt(key, Some(secret), Some(&pad as *const _ as *const _), Some(&mut out), &mut size, NCRYPT_PAD_OAEP_FLAG).is_ok()
            };
            if !ok {
                drop(_k);
                guard_delete(name);
                return None;
            }
            out.truncate(size as usize);
            out
        };
        // Самопроверка через новый дескриптор, как при настоящем входе.
        match guard_open(name, auth, &ct) {
            Ok(back) if back.as_slice() == secret => Some(ct),
            _ => {
                guard_delete(name);
                None
            }
        }
    }

    pub fn guard_open(name: &str, auth: &str, wrapped: &[u8]) -> Result<Zeroizing<Vec<u8>>, GuardError> {
        unsafe {
            let (prov, _p) = provider().ok_or(GuardError::Failed)?;
            let wname = wide(name);
            let mut key = NCRYPT_KEY_HANDLE::default();
            NCryptOpenKey(prov, &mut key, PCWSTR(wname.as_ptr()), CERT_KEY_SPEC(0), NCRYPT_SILENT_FLAG).map_err(|e| classify(&e))?;
            let _k = Handle(NCRYPT_HANDLE(key.0));
            if !set_auth(key, auth) {
                return Err(GuardError::Failed);
            }
            let pad = oaep();
            let mut size = 0u32;
            NCryptDecrypt(key, Some(wrapped), Some(&pad as *const _ as *const _), None, &mut size, NCRYPT_PAD_OAEP_FLAG | NCRYPT_SILENT_FLAG).map_err(|e| classify(&e))?;
            let mut out = Zeroizing::new(vec![0u8; size as usize]);
            NCryptDecrypt(key, Some(wrapped), Some(&pad as *const _ as *const _), Some(&mut out), &mut size, NCRYPT_PAD_OAEP_FLAG | NCRYPT_SILENT_FLAG).map_err(|e| classify(&e))?;
            out.truncate(size as usize);
            Ok(out)
        }
    }

    pub fn guard_delete(name: &str) {
        unsafe {
            let Some((prov, _p)) = provider() else { return };
            let wname = wide(name);
            let mut key = NCRYPT_KEY_HANDLE::default();
            if NCryptOpenKey(prov, &mut key, PCWSTR(wname.as_ptr()), CERT_KEY_SPEC(0), NCRYPT_SILENT_FLAG).is_ok() && NCryptDeleteKey(key, NCRYPT_SILENT_FLAG.0).is_err() {
                let _ = NCryptFreeObject(NCRYPT_HANDLE(key.0));
            }
        }
    }

    /// UUID системы из таблицы SMBIOS (структура типа 1), иначе MachineGuid.
    pub fn hardware_id() -> String {
        smbios_uuid().or_else(machine_guid).unwrap_or_else(|| "unknown".into())
    }

    fn smbios_uuid() -> Option<String> {
        unsafe {
            let sig = FIRMWARE_TABLE_PROVIDER(u32::from_be_bytes(*b"RSMB"));
            let size = GetSystemFirmwareTable(sig, 0, None);
            if size == 0 {
                return None;
            }
            let mut buf = vec![0u8; size as usize];
            if GetSystemFirmwareTable(sig, 0, Some(&mut buf)) == 0 {
                return None;
            }
            // RawSMBIOSData: 8 байт заголовка, затем структуры.
            let len = u32::from_le_bytes(buf.get(4..8)?.try_into().ok()?) as usize;
            let table = buf.get(8..8 + len)?;
            let mut i = 0;
            while i + 4 <= table.len() {
                let kind = table[i];
                let hlen = table[i + 1] as usize;
                if hlen < 4 {
                    break;
                }
                if kind == 1 && hlen >= 0x18 {
                    let uuid = table.get(i + 8..i + 24)?;
                    if uuid.iter().all(|b| *b == 0) || uuid.iter().all(|b| *b == 0xff) {
                        return None;
                    }
                    return Some(uuid.iter().map(|b| format!("{b:02x}")).collect());
                }
                // Пропускаем строковую часть, которая заканчивается двумя нулями.
                let mut j = i + hlen;
                while j + 1 < table.len() && !(table[j] == 0 && table[j + 1] == 0) {
                    j += 1;
                }
                i = j + 2;
                if kind == 127 {
                    break;
                }
            }
            None
        }
    }

    fn machine_guid() -> Option<String> {
        unsafe {
            let mut buf = [0u16; 128];
            let mut size = (buf.len() * 2) as u32;
            let r = RegGetValueW(
                HKEY_LOCAL_MACHINE,
                w!("SOFTWARE\\Microsoft\\Cryptography"),
                w!("MachineGuid"),
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr() as *mut _),
                Some(&mut size),
            );
            if r.is_err() {
                return None;
            }
            let n = (size as usize / 2).saturating_sub(1);
            Some(String::from_utf16_lossy(&buf[..n]))
        }
    }
}

#[cfg(not(windows))]
mod imp {
    // Сборка вне Windows нужна только для тестов: защита здесь условная.
    use zeroize::Zeroizing;
    pub fn dpapi_protect(d: &[u8]) -> Option<Vec<u8>> { Some(d.to_vec()) }
    pub fn dpapi_unprotect(d: &[u8]) -> Option<Zeroizing<Vec<u8>>> { Some(Zeroizing::new(d.to_vec())) }
    pub fn tpm_encrypt(_: &[u8]) -> Option<Vec<u8>> { None }
    pub fn tpm_decrypt(_: &[u8]) -> Option<Zeroizing<Vec<u8>>> { None }
    pub fn hardware_id() -> String { "test-machine".into() }

    use super::GuardError;

    // В тестах TPM изображает простая проверка: шифротекст хранит хэш auth.
    #[cfg(test)]
    pub fn guard_create(_: &str, auth: &str, secret: &[u8]) -> Option<Vec<u8>> {
        let mut v = crate::crypto::sha256_hex(auth.as_bytes()).into_bytes();
        v.extend_from_slice(secret);
        Some(v)
    }
    #[cfg(test)]
    pub fn guard_open(_: &str, auth: &str, w: &[u8]) -> Result<Zeroizing<Vec<u8>>, GuardError> {
        let h = crate::crypto::sha256_hex(auth.as_bytes()).into_bytes();
        if w.len() > h.len() && w[..h.len()] == h[..] { Ok(Zeroizing::new(w[h.len()..].to_vec())) } else { Err(GuardError::Failed) }
    }
    #[cfg(not(test))]
    pub fn guard_create(_: &str, _: &str, _: &[u8]) -> Option<Vec<u8>> { None }
    #[cfg(not(test))]
    pub fn guard_open(_: &str, _: &str, _: &[u8]) -> Result<Zeroizing<Vec<u8>>, GuardError> { Err(GuardError::Missing) }
    pub fn guard_delete(_: &str) {}
}
