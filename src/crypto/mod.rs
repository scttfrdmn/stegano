//! Cryptographic layer for secure steganography.
//!
//! This module provides:
//!
//! * [`derive_key`] — Argon2id password-based key derivation with a per-payload
//!   random salt (parameters in [`PRODUCTION_KDF`]).
//! * [`aead_seal`] / [`aead_open`] — AES-256-GCM / ChaCha20-Poly1305 with
//!   associated data, used by the payload wire format in [`crate::codec`].
//! * [`CryptoLayer`] — a standalone encrypt/decrypt helper with its own small
//!   self-describing serialisation ([`CryptoLayer::serialize_payload`]).
//!
//! All derived keys and stored key material are wiped from memory on drop
//! via [`zeroize`].  See `docs/WIRE_FORMAT.md` for the byte layouts.

use crate::core::*;
use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm,
};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::ChaCha20Poly1305;
use rand::{thread_rng, RngCore};
use zeroize::Zeroizing;

/// Length of the per-payload Argon2id salt in bytes.
pub const SALT_LEN: usize = 16;
/// Length of the AEAD nonce in bytes (both ciphers).
pub const NONCE_LEN: usize = 12;
/// Length of the AEAD authentication tag in bytes (both ciphers).
pub const TAG_LEN: usize = 16;
/// Length of derived symmetric keys in bytes.
pub const KEY_LEN: usize = 32;

/// Argon2id cost parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfParams {
    /// Memory cost in KiB.
    pub m_cost_kib: u32,
    /// Number of passes (time cost).
    pub t_cost: u32,
    /// Degree of parallelism (lanes).
    pub p_cost: u32,
}

/// Production Argon2id parameters: 64 MiB memory, 3 passes, 1 lane.
///
/// These follow the RFC 9106 "second recommended" profile and are fixed by
/// wire-format version 2 (they are implied by the version byte and not
/// stored per payload).
pub const PRODUCTION_KDF: KdfParams = KdfParams {
    m_cost_kib: 64 * 1024,
    t_cost: 3,
    p_cost: 1,
};

/// Cheap parameters used only by this crate's own unit tests (`cfg(test)`)
/// to keep the test suite fast.  Never used in a normal build.
#[cfg(test)]
pub(crate) const TEST_KDF: KdfParams = KdfParams {
    m_cost_kib: 1024,
    t_cost: 1,
    p_cost: 1,
};

/// The parameters actually used by [`derive_key`].
#[cfg(not(test))]
pub const ACTIVE_KDF: KdfParams = PRODUCTION_KDF;
/// The parameters actually used by [`derive_key`] (cheap test build).
#[cfg(test)]
pub const ACTIVE_KDF: KdfParams = TEST_KDF;

/// Derive a 256-bit key from a password and salt with Argon2id using
/// [`ACTIVE_KDF`] (equal to [`PRODUCTION_KDF`] in all non-test builds).
pub fn derive_key(password: &[u8], salt: &[u8; SALT_LEN]) -> Result<Zeroizing<[u8; KEY_LEN]>> {
    derive_key_with_params(password, salt, &ACTIVE_KDF)
}

/// Derive a 256-bit key from a password and salt with explicit Argon2id
/// parameters.
pub fn derive_key_with_params(
    password: &[u8],
    salt: &[u8; SALT_LEN],
    kdf: &KdfParams,
) -> Result<Zeroizing<[u8; KEY_LEN]>> {
    let params = Params::new(kdf.m_cost_kib, kdf.t_cost, kdf.p_cost, Some(KEY_LEN))
        .map_err(|e| SteganoError::EncryptionError(format!("Argon2 params: {e}")))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut out = Zeroizing::new([0u8; KEY_LEN]);
    argon
        .hash_password_into(password, salt, out.as_mut())
        .map_err(|e| SteganoError::EncryptionError(format!("Argon2 derivation: {e}")))?;
    Ok(out)
}

/// Fill an `N`-byte array from the OS-seeded thread RNG.
pub(crate) fn random_array<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    thread_rng().fill_bytes(&mut out);
    out
}

/// AEAD-encrypt `plaintext`, authenticating `aad`.  Output is `ct ‖ tag`.
pub fn aead_seal(
    kind: CipherKind,
    key: &[u8; KEY_LEN],
    nonce: &[u8; NONCE_LEN],
    aad: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>> {
    let payload = Payload {
        msg: plaintext,
        aad,
    };
    match kind {
        CipherKind::Aes256Gcm => Aes256Gcm::new_from_slice(key)
            .map_err(|e| SteganoError::EncryptionError(format!("AES key init: {e}")))?
            .encrypt(aes_gcm::Nonce::from_slice(nonce), payload)
            .map_err(|e| SteganoError::EncryptionError(format!("AES encrypt: {e}"))),
        CipherKind::ChaCha20Poly1305 => ChaCha20Poly1305::new_from_slice(key)
            .map_err(|e| SteganoError::EncryptionError(format!("ChaCha key init: {e}")))?
            .encrypt(chacha20poly1305::Nonce::from_slice(nonce), payload)
            .map_err(|e| SteganoError::EncryptionError(format!("ChaCha encrypt: {e}"))),
    }
}

/// AEAD-decrypt `ct ‖ tag`, verifying `aad`.  Any mismatch (wrong key,
/// modified ciphertext, nonce or associated data) yields
/// [`SteganoError::EncryptionError`].
pub fn aead_open(
    kind: CipherKind,
    key: &[u8; KEY_LEN],
    nonce: &[u8; NONCE_LEN],
    aad: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>> {
    let payload = Payload {
        msg: ciphertext,
        aad,
    };
    let fail = |_| {
        SteganoError::EncryptionError(
            "Decryption failed: wrong password or tampered payload".to_string(),
        )
    };
    match kind {
        CipherKind::Aes256Gcm => Aes256Gcm::new_from_slice(key)
            .map_err(|e| SteganoError::EncryptionError(format!("AES key init: {e}")))?
            .decrypt(aes_gcm::Nonce::from_slice(nonce), payload)
            .map_err(fail),
        CipherKind::ChaCha20Poly1305 => ChaCha20Poly1305::new_from_slice(key)
            .map_err(|e| SteganoError::EncryptionError(format!("ChaCha key init: {e}")))?
            .decrypt(chacha20poly1305::Nonce::from_slice(nonce), payload)
            .map_err(fail),
    }
}

/// Key material held by a [`CryptoLayer`]; zeroized on drop.
enum Secret {
    /// Password: a fresh key is derived per payload with Argon2id + salt.
    Password(Zeroizing<Vec<u8>>),
    /// Caller-supplied raw 256-bit key (no KDF).
    Key(Zeroizing<[u8; KEY_LEN]>),
}

/// Authenticated encryption layer.
///
/// Supports AES-256-GCM (default, fast with AES-NI) and ChaCha20-Poly1305
/// (constant-time in software, better on ARM / embedded targets).
///
/// Password-based layers derive a new key for every payload with Argon2id and
/// a random 16-byte salt stored in the [`SecurePayload`].
pub struct CryptoLayer {
    kind: CipherKind,
    secret: Secret,
}

impl std::fmt::Debug for CryptoLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CryptoLayer")
            .field("kind", &self.kind)
            .field("secret", &"<redacted>")
            .finish()
    }
}

/// An encrypted payload produced by [`CryptoLayer::encrypt`].
#[derive(Debug, Clone)]
pub struct SecurePayload {
    /// Cipher used to produce `encrypted_data`.
    pub cipher: CipherKind,
    /// `ciphertext ‖ tag`.
    pub encrypted_data: Vec<u8>,
    /// AEAD nonce.
    pub nonce: [u8; NONCE_LEN],
    /// Argon2id salt (random; only used for password-based layers).
    pub salt: [u8; SALT_LEN],
}

/// Version byte of the [`CryptoLayer::serialize_payload`] format.
const SECURE_PAYLOAD_VERSION: u8 = 2;
/// `[version][cipher][salt16][nonce12][len u32]`
const SECURE_PAYLOAD_HEADER_LEN: usize = 1 + 1 + SALT_LEN + NONCE_LEN + 4;

impl SecurePayload {
    /// Serialised header; also used as the AEAD associated data so that the
    /// cipher byte, salt, nonce and length are all authenticated.
    fn header(&self) -> [u8; SECURE_PAYLOAD_HEADER_LEN] {
        let len = u32::try_from(self.encrypted_data.len()).unwrap_or(u32::MAX);
        Self::header_for(self.cipher, &self.salt, &self.nonce, len)
    }

    fn header_for(
        cipher: CipherKind,
        salt: &[u8; SALT_LEN],
        nonce: &[u8; NONCE_LEN],
        len: u32,
    ) -> [u8; SECURE_PAYLOAD_HEADER_LEN] {
        let mut h = [0u8; SECURE_PAYLOAD_HEADER_LEN];
        h[0] = SECURE_PAYLOAD_VERSION;
        h[1] = cipher.to_u8();
        h[2..2 + SALT_LEN].copy_from_slice(salt);
        h[2 + SALT_LEN..2 + SALT_LEN + NONCE_LEN].copy_from_slice(nonce);
        h[2 + SALT_LEN + NONCE_LEN..].copy_from_slice(&len.to_le_bytes());
        h
    }
}

impl CryptoLayer {
    /// Create a password-based crypto layer (AES-256-GCM).
    pub fn new_from_password(password: &str) -> Result<Self> {
        Ok(Self {
            kind: CipherKind::Aes256Gcm,
            secret: Secret::Password(Zeroizing::new(password.as_bytes().to_vec())),
        })
    }

    /// Create a password-based crypto layer using ChaCha20-Poly1305.
    pub fn new_from_password_chacha20(password: &str) -> Result<Self> {
        Ok(Self {
            kind: CipherKind::ChaCha20Poly1305,
            secret: Secret::Password(Zeroizing::new(password.as_bytes().to_vec())),
        })
    }

    /// Create crypto layer from a raw 32-byte key (AES-256-GCM).
    pub fn new_from_key(key_bytes: &[u8]) -> Result<Self> {
        Self::new_from_key_with_kind(key_bytes, CipherKind::Aes256Gcm)
    }

    /// Create crypto layer from a raw 32-byte key with a specified cipher.
    pub fn new_from_key_with_kind(key_bytes: &[u8], kind: CipherKind) -> Result<Self> {
        if key_bytes.len() != KEY_LEN {
            return Err(SteganoError::EncryptionError(
                "Key must be exactly 32 bytes".to_string(),
            ));
        }
        let mut arr = Zeroizing::new([0u8; KEY_LEN]);
        arr.copy_from_slice(key_bytes);
        Ok(Self {
            kind,
            secret: Secret::Key(arr),
        })
    }

    /// Resolve the key to use for a payload with the given salt.
    fn key_for(&self, salt: &[u8; SALT_LEN]) -> Result<Zeroizing<[u8; KEY_LEN]>> {
        match &self.secret {
            Secret::Password(pw) => derive_key(pw, salt),
            Secret::Key(k) => Ok(Zeroizing::new(**k)),
        }
    }

    /// Encrypt payload with authenticated encryption.
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<SecurePayload> {
        let ct_len = plaintext
            .len()
            .checked_add(TAG_LEN)
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| {
                SteganoError::InvalidParameter("Payload too large to encrypt (> u32::MAX)".into())
            })?;
        let salt = random_array::<SALT_LEN>();
        let nonce = random_array::<NONCE_LEN>();
        let key = self.key_for(&salt)?;
        let aad = SecurePayload::header_for(self.kind, &salt, &nonce, ct_len);
        let encrypted_data = aead_seal(self.kind, &key, &nonce, &aad, plaintext)?;
        Ok(SecurePayload {
            cipher: self.kind,
            encrypted_data,
            nonce,
            salt,
        })
    }

    /// Decrypt and verify a payload.  The cipher used is taken from
    /// `secure_payload.cipher` (stored in the payload header), not `self.kind`.
    pub fn decrypt(&self, secure_payload: &SecurePayload) -> Result<Vec<u8>> {
        let key = self.key_for(&secure_payload.salt)?;
        aead_open(
            secure_payload.cipher,
            &key,
            &secure_payload.nonce,
            &secure_payload.header(),
            &secure_payload.encrypted_data,
        )
    }

    /// Serialize secure payload to bytes.
    ///
    /// Layout: `[version=2][cipher(1)][salt(16)][nonce(12)][len u32 LE][ct‖tag]`
    /// (34-byte header).  The header is the AEAD associated data.
    pub fn serialize_payload(payload: &SecurePayload) -> Vec<u8> {
        let header = payload.header();
        let mut result = Vec::with_capacity(header.len() + payload.encrypted_data.len());
        result.extend_from_slice(&header);
        result.extend_from_slice(&payload.encrypted_data);
        result
    }

    /// Deserialize a secure payload produced by [`serialize_payload`](Self::serialize_payload).
    pub fn deserialize_payload(data: &[u8]) -> Result<SecurePayload> {
        if data.len() < SECURE_PAYLOAD_HEADER_LEN + TAG_LEN {
            return Err(SteganoError::IntegrityError(
                "Invalid payload format".to_string(),
            ));
        }
        if data[0] != SECURE_PAYLOAD_VERSION {
            return Err(SteganoError::IntegrityError(format!(
                "Unsupported secure payload version {}",
                data[0]
            )));
        }
        let cipher = CipherKind::from_u8(data[1]).ok_or_else(|| {
            SteganoError::IntegrityError(format!("Unknown cipher kind byte: {}", data[1]))
        })?;
        let mut salt = [0u8; SALT_LEN];
        salt.copy_from_slice(&data[2..2 + SALT_LEN]);
        let mut nonce = [0u8; NONCE_LEN];
        nonce.copy_from_slice(&data[2 + SALT_LEN..2 + SALT_LEN + NONCE_LEN]);
        let off = 2 + SALT_LEN + NONCE_LEN;
        let len = u32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]]);
        let body = &data[SECURE_PAYLOAD_HEADER_LEN..];
        if u64::from(len) != body.len() as u64 {
            return Err(SteganoError::IntegrityError(
                "Payload size mismatch".to_string(),
            ));
        }
        Ok(SecurePayload {
            cipher,
            encrypted_data: body.to_vec(),
            nonce,
            salt,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_production_kdf_params_are_strong() {
        assert_eq!(PRODUCTION_KDF.m_cost_kib, 65536);
        assert_eq!(PRODUCTION_KDF.t_cost, 3);
        assert_eq!(PRODUCTION_KDF.p_cost, 1);
    }

    #[test]
    fn test_argon2_params_honoured() {
        let salt = [7u8; SALT_LEN];
        let ours = derive_key(b"pw", &salt).unwrap();
        // Reference computation with the argon2 crate and the same params.
        let p = Params::new(
            ACTIVE_KDF.m_cost_kib,
            ACTIVE_KDF.t_cost,
            ACTIVE_KDF.p_cost,
            Some(KEY_LEN),
        )
        .unwrap();
        let mut reference = [0u8; KEY_LEN];
        Argon2::new(Algorithm::Argon2id, Version::V0x13, p)
            .hash_password_into(b"pw", &salt, &mut reference)
            .unwrap();
        assert_eq!(*ours, reference);
        // Different params (t_cost) must give a different key.
        let other = derive_key_with_params(
            b"pw",
            &salt,
            &KdfParams {
                t_cost: ACTIVE_KDF.t_cost + 1,
                ..ACTIVE_KDF
            },
        )
        .unwrap();
        assert_ne!(*ours, *other);
        // Different salt must give a different key.
        let other_salt = derive_key(b"pw", &[8u8; SALT_LEN]).unwrap();
        assert_ne!(*ours, *other_salt);
    }

    #[test]
    fn test_crypto_layer_from_password() {
        let crypto = CryptoLayer::new_from_password("test_password").unwrap();
        let encrypted = crypto.encrypt(b"test").unwrap();
        assert_eq!(crypto.decrypt(&encrypted).unwrap(), b"test");
    }

    #[test]
    fn test_crypto_layer_from_key() {
        let crypto = CryptoLayer::new_from_key(&[1u8; 32]).unwrap();
        let encrypted = crypto.encrypt(b"test").unwrap();
        assert_eq!(crypto.decrypt(&encrypted).unwrap(), b"test");
    }

    #[test]
    fn test_crypto_layer_from_key_invalid_length() {
        let result = CryptoLayer::new_from_key(&[1u8; 16]);
        assert!(matches!(result, Err(SteganoError::EncryptionError(_))));
    }

    #[test]
    fn test_debug_redacts_secret() {
        let crypto = CryptoLayer::new_from_password("hunter2").unwrap();
        let s = format!("{crypto:?}");
        assert!(!s.contains("hunter2"));
        assert!(s.contains("redacted"));
    }

    #[test]
    fn test_password_salts_are_random() {
        let crypto = CryptoLayer::new_from_password("pw").unwrap();
        let a = crypto.encrypt(b"x").unwrap();
        let b = crypto.encrypt(b"x").unwrap();
        assert_ne!(a.salt, b.salt);
        assert_ne!(a.nonce, b.nonce);
        assert_ne!(a.encrypted_data, b.encrypted_data);
    }

    #[test]
    fn test_chacha20_roundtrip() {
        let crypto = CryptoLayer::new_from_password_chacha20("secure_password").unwrap();
        let encrypted = crypto.encrypt(b"Secret (ChaCha20)").unwrap();
        assert_eq!(encrypted.cipher, CipherKind::ChaCha20Poly1305);
        assert_eq!(crypto.decrypt(&encrypted).unwrap(), b"Secret (ChaCha20)");
    }

    #[test]
    fn test_auto_detect_cipher_on_decrypt() {
        let key = [0x42u8; 32];
        let enc_layer =
            CryptoLayer::new_from_key_with_kind(&key, CipherKind::ChaCha20Poly1305).unwrap();
        let serialized = CryptoLayer::serialize_payload(&enc_layer.encrypt(b"auto").unwrap());
        let deserialized = CryptoLayer::deserialize_payload(&serialized).unwrap();
        assert_eq!(deserialized.cipher, CipherKind::ChaCha20Poly1305);
        let dec_layer = CryptoLayer::new_from_key(&key).unwrap();
        assert_eq!(dec_layer.decrypt(&deserialized).unwrap(), b"auto");
    }

    #[test]
    fn test_empty_and_large_data() {
        let crypto = CryptoLayer::new_from_password("secure_password").unwrap();
        let e = crypto.encrypt(b"").unwrap();
        assert!(crypto.decrypt(&e).unwrap().is_empty());
        let big = vec![0xAAu8; 10000];
        let e = crypto.encrypt(&big).unwrap();
        assert_eq!(crypto.decrypt(&e).unwrap(), big);
    }

    #[test]
    fn test_tampering_fails() {
        let crypto = CryptoLayer::new_from_password("secure_password").unwrap();
        let enc = crypto.encrypt(b"Secret data").unwrap();

        let mut t = enc.clone();
        t.encrypted_data[0] ^= 1;
        assert!(crypto.decrypt(&t).is_err());

        let mut t = enc.clone();
        t.nonce[0] ^= 1;
        assert!(crypto.decrypt(&t).is_err());

        let mut t = enc.clone();
        t.salt[0] ^= 1;
        assert!(crypto.decrypt(&t).is_err());

        // Cipher byte is authenticated (AAD) as well as selecting the algorithm.
        let mut t = enc;
        t.cipher = CipherKind::ChaCha20Poly1305;
        assert!(matches!(
            crypto.decrypt(&t),
            Err(SteganoError::EncryptionError(_))
        ));
    }

    #[test]
    fn test_wrong_password_is_crypto_error() {
        let a = CryptoLayer::new_from_password("password1").unwrap();
        let b = CryptoLayer::new_from_password("password2").unwrap();
        let enc = a.encrypt(b"Same plaintext").unwrap();
        assert!(matches!(
            b.decrypt(&enc),
            Err(SteganoError::EncryptionError(_))
        ));
    }

    #[test]
    fn test_serialization_roundtrip_and_format() {
        let crypto = CryptoLayer::new_from_password("pw").unwrap();
        let enc = crypto.encrypt(b"Data to serialize").unwrap();
        let s = CryptoLayer::serialize_payload(&enc);
        assert_eq!(s.len(), SECURE_PAYLOAD_HEADER_LEN + 17 + TAG_LEN);
        assert_eq!(s[0], SECURE_PAYLOAD_VERSION);
        assert_eq!(s[1], CipherKind::Aes256Gcm.to_u8());
        assert_eq!(&s[2..18], &enc.salt);
        assert_eq!(&s[18..30], &enc.nonce);
        assert_eq!(
            u32::from_le_bytes([s[30], s[31], s[32], s[33]]) as usize,
            enc.encrypted_data.len()
        );
        let d = CryptoLayer::deserialize_payload(&s).unwrap();
        assert_eq!(crypto.decrypt(&d).unwrap(), b"Data to serialize");
    }

    #[test]
    fn test_deserialization_rejects_bad_input() {
        assert!(matches!(
            CryptoLayer::deserialize_payload(&[0u8; 40]),
            Err(SteganoError::IntegrityError(_))
        ));
        let crypto = CryptoLayer::new_from_password("pw").unwrap();
        let mut s = CryptoLayer::serialize_payload(&crypto.encrypt(b"abc").unwrap());
        // Length mismatch.
        let mut bad = s.clone();
        bad[30..34].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(CryptoLayer::deserialize_payload(&bad).is_err());
        // Unknown version.
        let mut bad = s.clone();
        bad[0] = 1;
        assert!(CryptoLayer::deserialize_payload(&bad).is_err());
        // Unknown cipher.
        s[1] = 9;
        assert!(CryptoLayer::deserialize_payload(&s).is_err());
    }

    #[test]
    fn test_cipher_kind_to_from_u8() {
        assert_eq!(CipherKind::Aes256Gcm.to_u8(), 0);
        assert_eq!(CipherKind::ChaCha20Poly1305.to_u8(), 1);
        assert_eq!(CipherKind::from_u8(0), Some(CipherKind::Aes256Gcm));
        assert_eq!(CipherKind::from_u8(1), Some(CipherKind::ChaCha20Poly1305));
        assert_eq!(CipherKind::from_u8(255), None);
    }
}
