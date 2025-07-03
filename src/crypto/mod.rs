//! Cryptographic layer for secure steganography

use crate::core::*;
use aes_gcm::{Aes256Gcm, Key, Nonce, aead::{Aead, KeyInit}};
use blake3::Hasher;
use rand::{RngCore, thread_rng};

pub struct CryptoLayer {
    cipher: Aes256Gcm,
}

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
                "Key must be exactly 32 bytes".to_string()
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
        let encrypted_data = self.cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| SteganoError::EncryptionError(format!("Encryption failed: {}", e)))?;

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
                "Payload hash verification failed".to_string()
            ));
        }

        // Decrypt data
        let nonce = Nonce::from_slice(&secure_payload.nonce);
        let plaintext = self.cipher
            .decrypt(nonce, secure_payload.encrypted_data.as_ref())
            .map_err(|e| SteganoError::EncryptionError(format!("Decryption failed: {}", e)))?;

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
        if data.len() < 48 { // 4 + 12 + 32 = minimum size
            return Err(SteganoError::IntegrityError(
                "Invalid payload format".to_string()
            ));
        }

        // Read length
        let data_len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        
        if data.len() != 48 + data_len {
            return Err(SteganoError::IntegrityError(
                "Payload size mismatch".to_string()
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