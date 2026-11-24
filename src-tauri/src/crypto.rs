//! Криптографические примитивы Clavis.
//!
//! Шифрование: XChaCha20-Poly1305 (AEAD, 192-битный случайный nonce).
//! Растяжение секретов: Argon2id, 64 МиБ памяти, 3 прохода.
//! Вывод ключей: HKDF-SHA256 с явным контекстом для каждого назначения.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

pub type Key = Zeroizing<[u8; 32]>;

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("данные повреждены или ключ не подходит")]
    Decrypt,
    #[error("ошибка растяжения ключа")]
    Kdf,
}

pub fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    OsRng.fill_bytes(&mut b);
    b
}

pub fn random_key() -> Key {
    Zeroizing::new(random::<32>())
}

/// Шифрует данные. Результат: nonce (24 байта) и шифротекст с тегом.
pub fn seal(key: &[u8; 32], aad: &[u8], plain: &[u8]) -> Vec<u8> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let nonce = random::<24>();
    let ct = cipher
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: plain, aad })
        .expect("шифрование не может завершиться ошибкой");
    let mut out = Vec::with_capacity(24 + ct.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    out
}

pub fn open(key: &[u8; 32], aad: &[u8], data: &[u8]) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    if data.len() < 24 + 16 {
        return Err(CryptoError::Decrypt);
    }
    let cipher = XChaCha20Poly1305::new(key.into());
    cipher
        .decrypt(XNonce::from_slice(&data[..24]), Payload { msg: &data[24..], aad })
        .map(Zeroizing::new)
        .map_err(|_| CryptoError::Decrypt)
}

/// Argon2id: делает перебор PIN-кода и фразы дорогим.
pub fn stretch(secret: &[u8], salt: &[u8]) -> Result<Key, CryptoError> {
    let params = Params::new(64 * 1024, 3, 1, Some(32)).map_err(|_| CryptoError::Kdf)?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut out = Zeroizing::new([0u8; 32]);
    argon.hash_password_into(secret, salt, out.as_mut()).map_err(|_| CryptoError::Kdf)?;
    Ok(out)
}

pub fn derive(ikm: &[u8], info: &str) -> Key {
    let hk = Hkdf::<Sha256>::new(Some(b"clavis/v1"), ikm);
    let mut out = Zeroizing::new([0u8; 32]);
    hk.expand(info.as_bytes(), out.as_mut()).expect("32 байта всегда допустимы");
    out
}

pub fn sha256_hex(data: &[u8]) -> String {
    let h = Sha256::digest(data);
    h.iter().map(|b| format!("{b:02x}")).collect()
}

/// Запечатывание для открытого ключа X25519: позволяет записывать историю,
/// пока хранилище заблокировано, не имея доступа к основному ключу.
pub mod sealed_box {
    use super::*;
    use x25519_dalek::{PublicKey, StaticSecret};

    pub fn keypair() -> (Zeroizing<[u8; 32]>, [u8; 32]) {
        let sk = StaticSecret::random_from_rng(OsRng);
        let pk = PublicKey::from(&sk);
        (Zeroizing::new(sk.to_bytes()), pk.to_bytes())
    }

    pub fn seal_to(pk: &[u8; 32], plain: &[u8]) -> Vec<u8> {
        let eph = StaticSecret::random_from_rng(OsRng);
        let eph_pk = PublicKey::from(&eph);
        let shared = eph.diffie_hellman(&PublicKey::from(*pk));
        let mut ikm = Vec::with_capacity(96);
        ikm.extend_from_slice(shared.as_bytes());
        ikm.extend_from_slice(eph_pk.as_bytes());
        ikm.extend_from_slice(pk);
        let key = derive(&ikm, "sealed-box");
        let mut out = eph_pk.as_bytes().to_vec();
        out.extend(seal(&key, b"sealed-box", plain));
        out
    }

    pub fn open_with(sk: &[u8; 32], data: &[u8]) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
        if data.len() < 32 {
            return Err(CryptoError::Decrypt);
        }
        let sk = StaticSecret::from(*sk);
        let pk = PublicKey::from(&sk);
        let mut eph = [0u8; 32];
        eph.copy_from_slice(&data[..32]);
        let shared = sk.diffie_hellman(&PublicKey::from(eph));
        let mut ikm = Vec::with_capacity(96);
        ikm.extend_from_slice(shared.as_bytes());
        ikm.extend_from_slice(&eph);
        ikm.extend_from_slice(pk.as_bytes());
        let key = derive(&ikm, "sealed-box");
        open(&key, b"sealed-box", &data[32..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let k = random_key();
        let c = seal(&k, b"a", b"hello");
        assert_eq!(&open(&k, b"a", &c).unwrap()[..], b"hello");
        assert!(open(&k, b"b", &c).is_err());
    }

    #[test]
    fn sealed_box_roundtrip() {
        let (sk, pk) = sealed_box::keypair();
        let c = sealed_box::seal_to(&pk, b"secret");
        assert_eq!(&sealed_box::open_with(&sk, &c).unwrap()[..], b"secret");
    }
}
