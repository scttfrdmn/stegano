//! Cryptographic layer for secure steganography

use crate::core::*;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use blake3::Hasher;
use rand::{thread_rng, RngCore};

pub struct CryptoLayer {
    cipher: Aes256Gcm,
}

#[derive(Debug)]
pub struct SecurePayload {
    pub encrypted_data: Vec<u8>,
    pub nonce: [u8; 12],
    pub hash: [u8; 32],
}

impl CryptoLayer {
    /// Create new crypto layer from password
    pub fn new_from_password(password: &str) -> Result<Self> {
        let key = Self::derive_key(password)?;
        let cipher = Aes256Gcm::new(&key);
        Ok(Self { cipher })
    }

    /// Create crypto layer from raw key
    pub fn new_from_key(key_bytes: &[u8]) -> Result<Self> {
        if key_bytes.len() != 32 {
            return Err(SteganoError::EncryptionError(
                "Key must be exactly 32 bytes".to_string(),
            ));
        }

        let key = Key::<Aes256Gcm>::from_slice(key_bytes);
        let cipher = Aes256Gcm::new(key);
        Ok(Self { cipher })
    }

    /// Derive 256-bit key from password using Blake3
    fn derive_key(password: &str) -> Result<Key<Aes256Gcm>> {
        let mut hasher = Hasher::new();
        hasher.update(password.as_bytes());
        hasher.update(b"stegano-salt-2025"); // Fixed salt for deterministic keys
        let hash = hasher.finalize();
        Ok(*Key::<Aes256Gcm>::from_slice(hash.as_bytes()))
    }

    /// Encrypt payload with authenticated encryption
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<SecurePayload> {
        // Generate random nonce
        let mut nonce_bytes = [0u8; 12];
        thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt data
        let encrypted_data = self
            .cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| SteganoError::EncryptionError(format!("Encryption failed: {e}")))?;

        // Generate hash for integrity
        let mut hasher = Hasher::new();
        hasher.update(&encrypted_data);
        hasher.update(&nonce_bytes);
        let hash = *hasher.finalize().as_bytes();

        Ok(SecurePayload {
            encrypted_data,
            nonce: nonce_bytes,
            hash,
        })
    }

    /// Decrypt and verify payload
    pub fn decrypt(&self, secure_payload: &SecurePayload) -> Result<Vec<u8>> {
        // Verify integrity hash
        let mut hasher = Hasher::new();
        hasher.update(&secure_payload.encrypted_data);
        hasher.update(&secure_payload.nonce);
        let computed_hash = *hasher.finalize().as_bytes();

        if computed_hash != secure_payload.hash {
            return Err(SteganoError::IntegrityError(
                "Payload hash verification failed".to_string(),
            ));
        }

        // Decrypt data
        let nonce = Nonce::from_slice(&secure_payload.nonce);
        let plaintext = self
            .cipher
            .decrypt(nonce, secure_payload.encrypted_data.as_ref())
            .map_err(|e| SteganoError::EncryptionError(format!("Decryption failed: {e}")))?;

        Ok(plaintext)
    }

    /// Serialize secure payload to bytes
    pub fn serialize_payload(payload: &SecurePayload) -> Vec<u8> {
        let mut result = Vec::new();

        // Length of encrypted data (4 bytes)
        result.extend_from_slice(&(payload.encrypted_data.len() as u32).to_le_bytes());

        // Nonce (12 bytes)
        result.extend_from_slice(&payload.nonce);

        // Hash (32 bytes)
        result.extend_from_slice(&payload.hash);

        // Encrypted data
        result.extend_from_slice(&payload.encrypted_data);

        result
    }

    /// Deserialize secure payload from bytes
    pub fn deserialize_payload(data: &[u8]) -> Result<SecurePayload> {
        if data.len() < 48 {
            // 4 + 12 + 32 = minimum size
            return Err(SteganoError::IntegrityError(
                "Invalid payload format".to_string(),
            ));
        }

        // Read length
        let data_len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;

        if data.len() != 48 + data_len {
            return Err(SteganoError::IntegrityError(
                "Payload size mismatch".to_string(),
            ));
        }

        // Read nonce
        let mut nonce = [0u8; 12];
        nonce.copy_from_slice(&data[4..16]);

        // Read hash
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&data[16..48]);

        // Read encrypted data
        let encrypted_data = data[48..].to_vec();

        Ok(SecurePayload {
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
        // Test that we can create a crypto layer and use it
        let test_data = b"test";
        let encrypted = crypto.encrypt(test_data).unwrap();
        let decrypted = crypto.decrypt(&encrypted).unwrap();
        assert_eq!(test_data, decrypted.as_slice());
    }

    #[test]
    fn test_crypto_layer_from_key() {
        let key = [1u8; 32];
        let crypto = CryptoLayer::new_from_key(&key).unwrap();
        // Test that we can create a crypto layer and use it
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
        let key1 = CryptoLayer::derive_key("password123").unwrap();
        let key2 = CryptoLayer::derive_key("password123").unwrap();
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_key_derivation_different_passwords() {
        let key1 = CryptoLayer::derive_key("password1").unwrap();
        let key2 = CryptoLayer::derive_key("password2").unwrap();
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_encryption_decryption_roundtrip() {
        let crypto = CryptoLayer::new_from_password("secure_password").unwrap();
        let plaintext = b"Secret scientific data";

        let encrypted = crypto.encrypt(plaintext).unwrap();
        let decrypted = crypto.decrypt(&encrypted).unwrap();

        assert_eq!(plaintext, decrypted.as_slice());
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
        let plaintext = vec![0xAAu8; 10000]; // 10KB of data

        let encrypted = crypto.encrypt(&plaintext).unwrap();
        let decrypted = crypto.decrypt(&encrypted).unwrap();

        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn test_tampered_encrypted_data_fails() {
        let crypto = CryptoLayer::new_from_password("secure_password").unwrap();
        let plaintext = b"Secret data";

        let mut encrypted = crypto.encrypt(plaintext).unwrap();

        // Tamper with encrypted data
        if !encrypted.encrypted_data.is_empty() {
            encrypted.encrypted_data[0] ^= 1;
        }

        let result = crypto.decrypt(&encrypted);
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(matches!(e, SteganoError::IntegrityError(_)));
        }
    }

    #[test]
    fn test_tampered_nonce_fails() {
        let crypto = CryptoLayer::new_from_password("secure_password").unwrap();
        let plaintext = b"Secret data";

        let mut encrypted = crypto.encrypt(plaintext).unwrap();

        // Tamper with nonce
        encrypted.nonce[0] ^= 1;

        let result = crypto.decrypt(&encrypted);
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(matches!(e, SteganoError::IntegrityError(_)));
        }
    }

    #[test]
    fn test_tampered_hash_fails() {
        let crypto = CryptoLayer::new_from_password("secure_password").unwrap();
        let plaintext = b"Secret data";

        let mut encrypted = crypto.encrypt(plaintext).unwrap();

        // Tamper with hash
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

        assert_eq!(encrypted.encrypted_data, deserialized.encrypted_data);
        assert_eq!(encrypted.nonce, deserialized.nonce);
        assert_eq!(encrypted.hash, deserialized.hash);
    }

    #[test]
    fn test_empty_payload_serialization() {
        let payload = SecurePayload {
            encrypted_data: vec![],
            nonce: [0u8; 12],
            hash: [0u8; 32],
        };

        let serialized = CryptoLayer::serialize_payload(&payload);
        let deserialized = CryptoLayer::deserialize_payload(&serialized).unwrap();

        assert_eq!(payload.encrypted_data, deserialized.encrypted_data);
        assert_eq!(payload.nonce, deserialized.nonce);
        assert_eq!(payload.hash, deserialized.hash);
    }

    #[test]
    fn test_deserialization_too_short() {
        let data = vec![0u8; 40]; // Less than 48 bytes
        let result = CryptoLayer::deserialize_payload(&data);
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(matches!(e, SteganoError::IntegrityError(_)));
        }
    }

    #[test]
    fn test_deserialization_size_mismatch() {
        let mut data = vec![0u8; 48];
        // Set length to 100 but provide only 48 total bytes
        data[0..4].copy_from_slice(&100u32.to_le_bytes());

        let result = CryptoLayer::deserialize_payload(&data);
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(matches!(e, SteganoError::IntegrityError(_)));
        }
    }

    #[test]
    fn test_serialization_format() {
        let payload = SecurePayload {
            encrypted_data: vec![0xAA, 0xBB, 0xCC],
            nonce: [1u8; 12],
            hash: [2u8; 32],
        };

        let serialized = CryptoLayer::serialize_payload(&payload);

        // Check length field
        assert_eq!(
            u32::from_le_bytes([serialized[0], serialized[1], serialized[2], serialized[3]]),
            3
        );

        // Check nonce
        assert_eq!(&serialized[4..16], &[1u8; 12]);

        // Check hash
        assert_eq!(&serialized[16..48], &[2u8; 32]);

        // Check encrypted data
        assert_eq!(&serialized[48..], &[0xAA, 0xBB, 0xCC]);
    }

    #[test]
    fn test_full_crypto_workflow() {
        let crypto = CryptoLayer::new_from_password("workflow_test").unwrap();
        let original_data = b"Complete workflow test data with various characters: !@#$%^&*()";

        // Encrypt
        let encrypted_payload = crypto.encrypt(original_data).unwrap();

        // Serialize
        let serialized = CryptoLayer::serialize_payload(&encrypted_payload);

        // Deserialize
        let deserialized_payload = CryptoLayer::deserialize_payload(&serialized).unwrap();

        // Decrypt
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

        // Different passwords should produce different encrypted results
        assert_ne!(encrypted1.encrypted_data, encrypted2.encrypted_data);

        // Cross-decryption should fail
        let result = crypto1.decrypt(&encrypted2);
        assert!(result.is_err());
    }

    #[test]
    fn test_secure_payload_structure() {
        let crypto = CryptoLayer::new_from_password("structure_test").unwrap();
        let plaintext = b"Test data";

        let encrypted = crypto.encrypt(plaintext).unwrap();

        // Verify nonce length
        assert_eq!(encrypted.nonce.len(), 12);

        // Verify hash length
        assert_eq!(encrypted.hash.len(), 32);

        // Verify encrypted data is different from plaintext
        assert_ne!(encrypted.encrypted_data, plaintext);

        // Verify encrypted data is not empty (for non-empty plaintext)
        assert!(!encrypted.encrypted_data.is_empty());
    }
}
