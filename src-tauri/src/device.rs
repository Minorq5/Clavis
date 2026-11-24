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

#[cfg(windows)]
mod imp {
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{HLOCAL, LocalFree};
    use windows::Win32::Security::Cryptography::*;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
    use windows::Win32::System::SystemInformation::{GetSystemFirmwareTable, FIRMWARE_TABLE_PROVIDER};
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
}
