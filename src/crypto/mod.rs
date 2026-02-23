//! Cryptographic layer for secure steganography

use crate::core::*;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use blake3::Hasher;
use chacha20poly1305::ChaCha20Poly1305;
use rand::{thread_rng, RngCore};

/// Authenticated encryption layer.
///
/// Supports AES-256-GCM (default, fast with AES-NI) and ChaCha20-Poly1305
/// (constant-time in software, better on ARM / embedded targets).
pub struct CryptoLayer {
    kind: CipherKind,
    key_bytes: [u8; 32],
}

#[derive(Debug)]
pub struct SecurePayload {
    pub cipher: CipherKind,
    pub encrypted_data: Vec<u8>,
    pub nonce: [u8; 12],
    pub hash: [u8; 32],
}

impl CryptoLayer {
    /// Create new crypto layer from password (AES-256-GCM by default).
    pub fn new_from_password(password: &str) -> Result<Self> {
        let key_bytes = Self::derive_key_bytes(password)?;
        Ok(Self {
            kind: CipherKind::Aes256Gcm,
            key_bytes,
        })
    }

    /// Create crypto layer from raw key (AES-256-GCM).
    pub fn new_from_key(key_bytes: &[u8]) -> Result<Self> {
        Self::new_from_key_with_kind(key_bytes, CipherKind::Aes256Gcm)
    }

    /// Create crypto layer from raw key with a specified cipher.
    pub fn new_from_key_with_kind(key_bytes: &[u8], kind: CipherKind) -> Result<Self> {
        if key_bytes.len() != 32 {
            return Err(SteganoError::EncryptionError(
                "Key must be exactly 32 bytes".to_string(),
            ));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(key_bytes);
        Ok(Self { kind, key_bytes: arr })
    }

    /// Create crypto layer from password using ChaCha20-Poly1305.
    pub fn new_from_password_chacha20(password: &str) -> Result<Self> {
        let key_bytes = Self::derive_key_bytes(password)?;
        Ok(Self {
            kind: CipherKind::ChaCha20Poly1305,
            key_bytes,
        })
    }

    /// Derive a 256-bit key from a password using Blake3.
    fn derive_key_bytes(password: &str) -> Result<[u8; 32]> {
        let mut hasher = Hasher::new();
        hasher.update(password.as_bytes());
        hasher.update(b"stegano-salt-2025"); // Fixed salt for deterministic keys
        Ok(*hasher.finalize().as_bytes())
    }

    /// Encrypt payload with authenticated encryption.
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<SecurePayload> {
        // Generate random 12-byte nonce
        let mut nonce_bytes = [0u8; 12];
        thread_rng().fill_bytes(&mut nonce_bytes);

        let encrypted_data = match self.kind {
            CipherKind::Aes256Gcm => {
                let cipher = Aes256Gcm::new_from_slice(&self.key_bytes)
                    .map_err(|e| SteganoError::EncryptionError(format!("AES key init: {e}")))?;
                let nonce = aes_gcm::Nonce::from_slice(&nonce_bytes);
                cipher
                    .encrypt(nonce, plaintext)
                    .map_err(|e| SteganoError::EncryptionError(format!("AES encrypt: {e}")))?
            }
            CipherKind::ChaCha20Poly1305 => {
                let cipher = ChaCha20Poly1305::new_from_slice(&self.key_bytes)
                    .map_err(|e| SteganoError::EncryptionError(format!("ChaCha key init: {e}")))?;
                let nonce = chacha20poly1305::Nonce::from_slice(&nonce_bytes);
                cipher
                    .encrypt(nonce, plaintext)
                    .map_err(|e| SteganoError::EncryptionError(format!("ChaCha encrypt: {e}")))?
            }
        };

        // BLAKE3 integrity hash over (ciphertext ‖ nonce)
        let mut hasher = Hasher::new();
        hasher.update(&encrypted_data);
        hasher.update(&nonce_bytes);
        let hash = *hasher.finalize().as_bytes();

        Ok(SecurePayload {
            cipher: self.kind,
            encrypted_data,
            nonce: nonce_bytes,
            hash,
        })
    }

    /// Decrypt and verify a payload.  The cipher used is taken from `secure_payload.cipher`
    /// (stored in the payload header during encryption), not from `self.kind`.
    pub fn decrypt(&self, secure_payload: &SecurePayload) -> Result<Vec<u8>> {
        // Verify integrity hash first
        let mut hasher = Hasher::new();
        hasher.update(&secure_payload.encrypted_data);
        hasher.update(&secure_payload.nonce);
        let computed_hash = *hasher.finalize().as_bytes();

        if computed_hash != secure_payload.hash {
            return Err(SteganoError::IntegrityError(
                "Payload hash verification failed".to_string(),
            ));
        }

        // Dispatch on the cipher stored in the payload (auto-detect on extract)
        match secure_payload.cipher {
            CipherKind::Aes256Gcm => {
                let cipher = Aes256Gcm::new_from_slice(&self.key_bytes)
                    .map_err(|e| SteganoError::EncryptionError(format!("AES key init: {e}")))?;
                let nonce = Nonce::from_slice(&secure_payload.nonce);
                cipher
                    .decrypt(nonce, secure_payload.encrypted_data.as_ref())
                    .map_err(|e| SteganoError::EncryptionError(format!("AES decrypt: {e}")))
            }
            CipherKind::ChaCha20Poly1305 => {
                let cipher = ChaCha20Poly1305::new_from_slice(&self.key_bytes)
                    .map_err(|e| SteganoError::EncryptionError(format!("ChaCha key init: {e}")))?;
                let nonce = chacha20poly1305::Nonce::from_slice(&secure_payload.nonce);
                cipher
                    .decrypt(nonce, secure_payload.encrypted_data.as_ref())
                    .map_err(|e| SteganoError::EncryptionError(format!("ChaCha decrypt: {e}")))
            }
        }
    }

    /// Serialize secure payload to bytes.
    ///
    /// Layout: `[cipher(1)] [len(4)] [nonce(12)] [hash(32)] [ciphertext(N)]`
    /// Total minimum size: 49 bytes.
    pub fn serialize_payload(payload: &SecurePayload) -> Vec<u8> {
        let mut result = Vec::new();

        // Cipher kind byte (1 byte) — enables auto-detection on extract
        result.push(payload.cipher.to_u8());

        // Length of encrypted data (4 bytes, LE)
        result.extend_from_slice(&(payload.encrypted_data.len() as u32).to_le_bytes());

        // Nonce (12 bytes)
        result.extend_from_slice(&payload.nonce);

        // Hash (32 bytes)
        result.extend_from_slice(&payload.hash);

        // Encrypted data
        result.extend_from_slice(&payload.encrypted_data);

        result
    }

    /// Deserialize secure payload from bytes.
    ///
    /// The cipher kind stored in byte 0 is returned as part of [`SecurePayload`]
    /// so that [`decrypt`](CryptoLayer::decrypt) can auto-select the right algorithm.
    pub fn deserialize_payload(data: &[u8]) -> Result<SecurePayload> {
        // 1 (cipher) + 4 (len) + 12 (nonce) + 32 (hash) = 49 minimum bytes
        if data.len() < 49 {
            return Err(SteganoError::IntegrityError(
                "Invalid payload format".to_string(),
            ));
        }

        // Read cipher kind
        let cipher = CipherKind::from_u8(data[0]).ok_or_else(|| {
            SteganoError::IntegrityError(format!("Unknown cipher kind byte: {}", data[0]))
        })?;

        // Read length
        let data_len = u32::from_le_bytes([data[1], data[2], data[3], data[4]]) as usize;

        if data.len() != 49 + data_len {
            return Err(SteganoError::IntegrityError(
                "Payload size mismatch".to_string(),
            ));
        }

        // Read nonce (bytes 5–16)
        let mut nonce = [0u8; 12];
        nonce.copy_from_slice(&data[5..17]);

        // Read hash (bytes 17–48)
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&data[17..49]);

        // Read encrypted data
        let encrypted_data = data[49..].to_vec();

        Ok(SecurePayload {
            cipher,
            encrypted_data,
            nonce,
            hash,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crypto_layer_from_password() {
        let crypto = CryptoLayer::new_from_password("test_password").unwrap();
        let test_data = b"test";
        let encrypted = crypto.encrypt(test_data).unwrap();
        let decrypted = crypto.decrypt(&encrypted).unwrap();
        assert_eq!(test_data, decrypted.as_slice());
    }

    #[test]
    fn test_crypto_layer_from_key() {
        let key = [1u8; 32];
        let crypto = CryptoLayer::new_from_key(&key).unwrap();
        let test_data = b"test";
        let encrypted = crypto.encrypt(test_data).unwrap();
        let decrypted = crypto.decrypt(&encrypted).unwrap();
        assert_eq!(test_data, decrypted.as_slice());
    }

    #[test]
    fn test_crypto_layer_from_key_invalid_length() {
        let key = [1u8; 16]; // Invalid length
        let result = CryptoLayer::new_from_key(&key);
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(matches!(e, SteganoError::EncryptionError(_)));
        }
    }

    #[test]
    fn test_key_derivation_deterministic() {
        let k1 = CryptoLayer::derive_key_bytes("password123").unwrap();
        let k2 = CryptoLayer::derive_key_bytes("password123").unwrap();
        assert_eq!(k1, k2);
    }

    #[test]
    fn test_key_derivation_different_passwords() {
        let k1 = CryptoLayer::derive_key_bytes("password1").unwrap();
        let k2 = CryptoLayer::derive_key_bytes("password2").unwrap();
        assert_ne!(k1, k2);
    }

    #[test]
    fn test_aes_encryption_decryption_roundtrip() {
        let crypto = CryptoLayer::new_from_password("secure_password").unwrap();
        let plaintext = b"Secret scientific data";
        let encrypted = crypto.encrypt(plaintext).unwrap();
        assert_eq!(encrypted.cipher, CipherKind::Aes256Gcm);
        let decrypted = crypto.decrypt(&encrypted).unwrap();
        assert_eq!(plaintext, decrypted.as_slice());
    }

    #[test]
    fn test_chacha20_encryption_decryption_roundtrip() {
        let crypto = CryptoLayer::new_from_password_chacha20("secure_password").unwrap();
        let plaintext = b"Secret scientific data (ChaCha20)";
        let encrypted = crypto.encrypt(plaintext).unwrap();
        assert_eq!(encrypted.cipher, CipherKind::ChaCha20Poly1305);
        let decrypted = crypto.decrypt(&encrypted).unwrap();
        assert_eq!(plaintext, decrypted.as_slice());
    }

    #[test]
    fn test_chacha20_from_key() {
        let key = [0xBBu8; 32];
        let crypto =
            CryptoLayer::new_from_key_with_kind(&key, CipherKind::ChaCha20Poly1305).unwrap();
        let data = b"chacha test payload";
        let enc = crypto.encrypt(data).unwrap();
        let dec = crypto.decrypt(&enc).unwrap();
        assert_eq!(data, dec.as_slice());
    }

    #[test]
    fn test_cipher_stored_in_payload() {
        // After serialization+deserialization the cipher kind survives
        let crypto = CryptoLayer::new_from_password_chacha20("pw").unwrap();
        let enc = crypto.encrypt(b"hello").unwrap();
        let serialized = CryptoLayer::serialize_payload(&enc);
        let deser = CryptoLayer::deserialize_payload(&serialized).unwrap();
        assert_eq!(deser.cipher, CipherKind::ChaCha20Poly1305);
    }

    #[test]
    fn test_auto_detect_cipher_on_decrypt() {
        // Encrypt with ChaCha20, decrypt with a CryptoLayer created with AES kind.
        // The stored cipher byte must make decrypt() use ChaCha20 regardless of self.kind.
        let key = [0x42u8; 32];
        let enc_layer =
            CryptoLayer::new_from_key_with_kind(&key, CipherKind::ChaCha20Poly1305).unwrap();
        let payload = b"auto-detect test";
        let encrypted = enc_layer.encrypt(payload).unwrap();
        let serialized = CryptoLayer::serialize_payload(&encrypted);
        let deserialized = CryptoLayer::deserialize_payload(&serialized).unwrap();
        // Create decrypt layer with default AES kind — should still work because
        // decrypt() dispatches on deserialized.cipher
        let dec_layer = CryptoLayer::new_from_key(&key).unwrap();
        let decrypted = dec_layer.decrypt(&deserialized).unwrap();
        assert_eq!(payload, decrypted.as_slice());
    }

    #[test]
    fn test_encryption_produces_different_nonces() {
        let crypto = CryptoLayer::new_from_password("secure_password").unwrap();
        let plaintext = b"Same data";
        let encrypted1 = crypto.encrypt(plaintext).unwrap();
        let encrypted2 = crypto.encrypt(plaintext).unwrap();
        assert_ne!(encrypted1.nonce, encrypted2.nonce);
        assert_ne!(encrypted1.encrypted_data, encrypted2.encrypted_data);
    }

    #[test]
    fn test_empty_data_encryption() {
        let crypto = CryptoLayer::new_from_password("secure_password").unwrap();
        let plaintext = b"";
        let encrypted = crypto.encrypt(plaintext).unwrap();
        let decrypted = crypto.decrypt(&encrypted).unwrap();
        assert_eq!(plaintext, decrypted.as_slice());
    }

    #[test]
    fn test_large_data_encryption() {
        let crypto = CryptoLayer::new_from_password("secure_password").unwrap();
        let plaintext = vec![0xAAu8; 10000]; // 10 KB
        let encrypted = crypto.encrypt(&plaintext).unwrap();
        let decrypted = crypto.decrypt(&encrypted).unwrap();
        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn test_tampered_encrypted_data_fails() {
        let crypto = CryptoLayer::new_from_password("secure_password").unwrap();
        let plaintext = b"Secret data";
        let mut encrypted = crypto.encrypt(plaintext).unwrap();
        if !encrypted.encrypted_data.is_empty() {
            encrypted.encrypted_data[0] ^= 1;
        }
        let result = crypto.decrypt(&encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn test_tampered_nonce_fails() {
        let crypto = CryptoLayer::new_from_password("secure_password").unwrap();
        let plaintext = b"Secret data";
        let mut encrypted = crypto.encrypt(plaintext).unwrap();
        encrypted.nonce[0] ^= 1;
        let result = crypto.decrypt(&encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn test_tampered_hash_fails() {
        let crypto = CryptoLayer::new_from_password("secure_password").unwrap();
        let plaintext = b"Secret data";
        let mut encrypted = crypto.encrypt(plaintext).unwrap();
        encrypted.hash[0] ^= 1;
        let result = crypto.decrypt(&encrypted);
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(matches!(e, SteganoError::IntegrityError(_)));
        }
    }

    #[test]
    fn test_payload_serialization_roundtrip() {
        let crypto = CryptoLayer::new_from_password("secure_password").unwrap();
        let plaintext = b"Data to serialize";
        let encrypted = crypto.encrypt(plaintext).unwrap();
        let serialized = CryptoLayer::serialize_payload(&encrypted);
        let deserialized = CryptoLayer::deserialize_payload(&serialized).unwrap();
        assert_eq!(encrypted.cipher, deserialized.cipher);
        assert_eq!(encrypted.encrypted_data, deserialized.encrypted_data);
        assert_eq!(encrypted.nonce, deserialized.nonce);
        assert_eq!(encrypted.hash, deserialized.hash);
    }

    #[test]
    fn test_deserialization_too_short() {
        let data = vec![0u8; 40]; // less than 49 bytes
        let result = CryptoLayer::deserialize_payload(&data);
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(matches!(e, SteganoError::IntegrityError(_)));
        }
    }

    #[test]
    fn test_deserialization_size_mismatch() {
        let mut data = vec![0u8; 49]; // cipher(1)+len(4)+nonce(12)+hash(32) = 49
        // Set length to 100 but provide no encrypted bytes
        data[1..5].copy_from_slice(&100u32.to_le_bytes());
        let result = CryptoLayer::deserialize_payload(&data);
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(matches!(e, SteganoError::IntegrityError(_)));
        }
    }

    #[test]
    fn test_serialization_format() {
        let payload = SecurePayload {
            cipher: CipherKind::Aes256Gcm,
            encrypted_data: vec![0xAA, 0xBB, 0xCC],
            nonce: [1u8; 12],
            hash: [2u8; 32],
        };
        let serialized = CryptoLayer::serialize_payload(&payload);
        // Byte 0: cipher kind
        assert_eq!(serialized[0], 0); // Aes256Gcm
        // Bytes 1-4: length (3 LE)
        assert_eq!(
            u32::from_le_bytes([serialized[1], serialized[2], serialized[3], serialized[4]]),
            3
        );
        // Bytes 5-16: nonce
        assert_eq!(&serialized[5..17], &[1u8; 12]);
        // Bytes 17-48: hash
        assert_eq!(&serialized[17..49], &[2u8; 32]);
        // Bytes 49..: encrypted data
        assert_eq!(&serialized[49..], &[0xAA, 0xBB, 0xCC]);
    }

    #[test]
    fn test_full_crypto_workflow() {
        let crypto = CryptoLayer::new_from_password("workflow_test").unwrap();
        let original_data = b"Complete workflow test data with various characters: !@#$%^&*()";
        let encrypted_payload = crypto.encrypt(original_data).unwrap();
        let serialized = CryptoLayer::serialize_payload(&encrypted_payload);
        let deserialized_payload = CryptoLayer::deserialize_payload(&serialized).unwrap();
        let decrypted_data = crypto.decrypt(&deserialized_payload).unwrap();
        assert_eq!(original_data, decrypted_data.as_slice());
    }

    #[test]
    fn test_full_chacha20_workflow() {
        let crypto = CryptoLayer::new_from_password_chacha20("workflow_test_chacha").unwrap();
        let original_data = b"ChaCha20 full workflow test with: !@#$%^&*()";
        let encrypted_payload = crypto.encrypt(original_data).unwrap();
        let serialized = CryptoLayer::serialize_payload(&encrypted_payload);
        let deserialized_payload = CryptoLayer::deserialize_payload(&serialized).unwrap();
        let decrypted_data = crypto.decrypt(&deserialized_payload).unwrap();
        assert_eq!(original_data, decrypted_data.as_slice());
    }

    #[test]
    fn test_different_passwords_different_results() {
        let crypto1 = CryptoLayer::new_from_password("password1").unwrap();
        let crypto2 = CryptoLayer::new_from_password("password2").unwrap();
        let plaintext = b"Same plaintext";
        let encrypted1 = crypto1.encrypt(plaintext).unwrap();
        let encrypted2 = crypto2.encrypt(plaintext).unwrap();
        assert_ne!(encrypted1.encrypted_data, encrypted2.encrypted_data);
        // Cross-decryption should fail (AEAD tag mismatch)
        let result = crypto1.decrypt(&encrypted2);
        assert!(result.is_err());
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
