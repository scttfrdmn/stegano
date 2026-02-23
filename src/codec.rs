//! Payload encoding pipeline: encryption + Reed-Solomon error correction.
//!
//! This module provides a self-framing encode/decode pipeline that applies,
//! in order: AES-256-GCM encryption (from `EmbedParams::encryption_key`) and
//! Reed-Solomon error correction (from `EmbedParams::error_correction_level`).
//!
//! The exact same `EmbedParams` must be used for both `encode_payload` and
//! `decode_payload`; there is no format auto-detection.

use crate::core::{ECLevel, EmbedParams, SteganoError};
use crate::crypto::CryptoLayer;
use reed_solomon::{Decoder, Encoder};

// Flags stored in the first byte of the encoded payload.
const FLAG_ENCRYPTED: u8 = 0x01;
const FLAG_EC: u8 = 0x02;

/// Encode a payload, optionally applying encryption and Reed-Solomon error
/// correction as specified by `params`.
///
/// Returns a self-framing byte sequence that [`decode_payload`] can reverse.
/// The returned bytes begin with a flags byte so the decoder knows which
/// operations were applied.
pub fn encode_payload(payload: &[u8], params: &EmbedParams) -> crate::core::Result<Vec<u8>> {
    let mut flags = 0u8;
    let mut data = payload.to_vec();

    // Step 1: encrypt with params key if provided.
    if !params.encryption_key.is_empty() {
        let key = normalize_key(&params.encryption_key);
        let crypto = CryptoLayer::new_from_key_with_kind(&key, params.cipher_kind)?;
        let sp = crypto.encrypt(&data)?;
        data = CryptoLayer::serialize_payload(&sp);
        flags |= FLAG_ENCRYPTED;
    }

    // Step 2: apply Reed-Solomon error correction.
    let ecc_len = ec_level_to_ecc_len(params.error_correction_level);
    if ecc_len > 0 {
        data = rs_encode(&data, ecc_len)?;
        flags |= FLAG_EC;
    }

    // Build output: [flags(1)] [if EC: ecc_len(1)] [data]
    let mut out = Vec::with_capacity(1 + if ecc_len > 0 { 1 } else { 0 } + data.len());
    out.push(flags);
    if ecc_len > 0 {
        out.push(ecc_len as u8);
    }
    out.extend_from_slice(&data);
    Ok(out)
}

/// Decode a payload previously encoded with [`encode_payload`].
///
/// The same `params` must be used as were used for encoding.
pub fn decode_payload(data: &[u8], params: &EmbedParams) -> crate::core::Result<Vec<u8>> {
    if data.is_empty() {
        return Err(SteganoError::IntegrityError(
            "Empty codec payload".to_string(),
        ));
    }

    let flags = data[0];
    let mut current = data[1..].to_vec();

    // Step 1: RS decode (EC is the outermost layer; decode it first).
    if flags & FLAG_EC != 0 {
        if current.is_empty() {
            return Err(SteganoError::IntegrityError(
                "Missing EC length byte".to_string(),
            ));
        }
        let ecc_len = current[0] as usize;
        current = rs_decode(&current[1..], ecc_len)?;
    }

    // Step 2: decrypt.
    if flags & FLAG_ENCRYPTED != 0 {
        if params.encryption_key.is_empty() {
            return Err(SteganoError::EncryptionError(
                "Encryption key required for decryption".to_string(),
            ));
        }
        let key = normalize_key(&params.encryption_key);
        let crypto = CryptoLayer::new_from_key(&key)?;
        let sp = CryptoLayer::deserialize_payload(&current)?;
        current = crypto.decrypt(&sp)?;
    }

    Ok(current)
}

/// Hash an arbitrary-length key to exactly 32 bytes using Blake3.
///
/// If the key is already 32 bytes it is used verbatim.  All other lengths
/// are hashed to produce a deterministic 32-byte value.
pub(crate) fn normalize_key(key: &[u8]) -> [u8; 32] {
    if key.len() == 32 {
        let mut arr = [0u8; 32];
        arr.copy_from_slice(key);
        return arr;
    }
    let mut hasher = blake3::Hasher::new();
    hasher.update(key);
    hasher.update(b"stegano-key-derive-v1");
    *hasher.finalize().as_bytes()
}

/// Map an [`ECLevel`] to the number of Reed-Solomon ECC bytes per block.
///
/// Returns 0 for `ECLevel::None` (no error correction).
fn ec_level_to_ecc_len(level: ECLevel) -> usize {
    match level {
        ECLevel::None => 0,
        ECLevel::Low => 16,    // ~6.7 % overhead per 239-byte block
        ECLevel::Medium => 32,  // ~14.3 % overhead per 223-byte block
        ECLevel::High => 64,    // ~33.5 % overhead per 191-byte block
        ECLevel::Maximum => 128, // ~100 % overhead per 127-byte block
    }
}

/// Reed-Solomon encode arbitrary-length data.
///
/// Data is split into fixed-size chunks of `255 - ecc_len` bytes (the last
/// chunk is zero-padded to the same size).  Each chunk is independently
/// encoded to exactly 255 bytes.
///
/// Output format: `[4-byte LE original_len][N × 255-byte RS blocks]`.
fn rs_encode(data: &[u8], ecc_len: usize) -> crate::core::Result<Vec<u8>> {
    let data_chunk = 255 - ecc_len;
    let enc = Encoder::new(ecc_len);
    let orig_len = data.len();
    // At least one block even for empty input.
    let num_blocks = orig_len.div_ceil(data_chunk).max(1);

    let capacity = num_blocks
        .checked_mul(255)
        .and_then(|n| n.checked_add(4))
        .ok_or_else(|| {
            SteganoError::InvalidParameter("Payload too large for Reed-Solomon encoding".into())
        })?;
    let mut out = Vec::with_capacity(capacity);
    out.extend_from_slice(&(orig_len as u32).to_le_bytes());

    for block_idx in 0..num_blocks {
        let start = block_idx * data_chunk;
        let end = (start + data_chunk).min(orig_len);
        let chunk = &data[start..end];

        // Pad to exactly data_chunk bytes so all encoded blocks are 255 bytes.
        let mut padded = vec![0u8; data_chunk];
        padded[..chunk.len()].copy_from_slice(chunk);

        let encoded = enc.encode(&padded);
        // encoded[..] gives all data_chunk + ecc_len = 255 bytes via Deref.
        out.extend_from_slice(&encoded[..]);
    }

    Ok(out)
}

/// Reed-Solomon decode data produced by [`rs_encode`].
fn rs_decode(data: &[u8], ecc_len: usize) -> crate::core::Result<Vec<u8>> {
    if data.len() < 4 {
        return Err(SteganoError::IntegrityError(
            "RS data too short for length header".to_string(),
        ));
    }

    let orig_len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let encoded_data = &data[4..];

    if !encoded_data.len().is_multiple_of(255) {
        return Err(SteganoError::IntegrityError(format!(
            "RS encoded data length {} is not a multiple of 255",
            encoded_data.len()
        )));
    }

    let dec = Decoder::new(ecc_len);
    let mut result = Vec::with_capacity(orig_len.max(1));

    for chunk in encoded_data.chunks(255) {
        let recovered = dec.correct(chunk, None).map_err(|_| {
            SteganoError::IntegrityError("Reed-Solomon error correction failed".to_string())
        })?;
        result.extend_from_slice(recovered.data());
    }

    result.truncate(orig_len);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{ECLevel, EmbedParams};
    use std::collections::HashMap;

    fn params_with_key(key: &[u8]) -> EmbedParams {
        EmbedParams {
            encryption_key: key.to_vec(),
            error_correction_level: ECLevel::None,
            stealth_level: crate::core::StealthLevel::Medium,
            use_gpu: false,
            enable_validation: false,
            format_specific: HashMap::new(),
            cipher_kind: Default::default(),
        }
    }

    fn params_with_ec(level: ECLevel) -> EmbedParams {
        EmbedParams {
            encryption_key: vec![],
            error_correction_level: level,
            stealth_level: crate::core::StealthLevel::Medium,
            use_gpu: false,
            enable_validation: false,
            format_specific: HashMap::new(),
            cipher_kind: Default::default(),
        }
    }

    fn params_with_both(key: &[u8], level: ECLevel) -> EmbedParams {
        EmbedParams {
            encryption_key: key.to_vec(),
            error_correction_level: level,
            stealth_level: crate::core::StealthLevel::Medium,
            use_gpu: false,
            enable_validation: false,
            format_specific: HashMap::new(),
            cipher_kind: Default::default(),
        }
    }

    fn no_codec_params() -> EmbedParams {
        EmbedParams {
            encryption_key: vec![],
            error_correction_level: ECLevel::None,
            stealth_level: crate::core::StealthLevel::Medium,
            use_gpu: false,
            enable_validation: false,
            format_specific: HashMap::new(),
            cipher_kind: Default::default(),
        }
    }

    #[test]
    fn test_no_op_roundtrip() {
        let params = no_codec_params();
        let payload = b"hello world";
        let encoded = encode_payload(payload, &params).unwrap();
        let decoded = decode_payload(&encoded, &params).unwrap();
        assert_eq!(decoded, payload);
        // flags byte == 0, so encoded is [0x00] + payload
        assert_eq!(encoded.len(), 1 + payload.len());
    }

    #[test]
    fn test_encryption_only_roundtrip() {
        let key = b"test key for encryption testing!"; // 32 bytes
        let params = params_with_key(key);
        let payload = b"secret payload data";
        let encoded = encode_payload(payload, &params).unwrap();
        let decoded = decode_payload(&encoded, &params).unwrap();
        assert_eq!(decoded, payload);
        // First byte should have FLAG_ENCRYPTED set
        assert!(encoded[0] & FLAG_ENCRYPTED != 0);
        assert!(encoded[0] & FLAG_EC == 0);
    }

    #[test]
    fn test_ec_only_roundtrip_low() {
        let params = params_with_ec(ECLevel::Low);
        let payload = b"data with error correction";
        let encoded = encode_payload(payload, &params).unwrap();
        let decoded = decode_payload(&encoded, &params).unwrap();
        assert_eq!(decoded, payload);
        assert!(encoded[0] & FLAG_EC != 0);
        assert!(encoded[0] & FLAG_ENCRYPTED == 0);
    }

    #[test]
    fn test_ec_only_roundtrip_medium() {
        let params = params_with_ec(ECLevel::Medium);
        let payload = b"medium error correction test";
        let encoded = encode_payload(payload, &params).unwrap();
        let decoded = decode_payload(&encoded, &params).unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn test_ec_only_roundtrip_high() {
        let params = params_with_ec(ECLevel::High);
        let payload = b"high error correction test";
        let encoded = encode_payload(payload, &params).unwrap();
        let decoded = decode_payload(&encoded, &params).unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn test_ec_only_roundtrip_maximum() {
        let params = params_with_ec(ECLevel::Maximum);
        let payload = b"maximum error correction";
        let encoded = encode_payload(payload, &params).unwrap();
        let decoded = decode_payload(&encoded, &params).unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn test_encryption_and_ec_roundtrip() {
        let key = b"encryption plus EC test key!!!!!"; // 32 bytes
        let params = params_with_both(key, ECLevel::Medium);
        let payload = b"secret data with error correction";
        let encoded = encode_payload(payload, &params).unwrap();
        let decoded = decode_payload(&encoded, &params).unwrap();
        assert_eq!(decoded, payload);
        // Both flags should be set
        assert!(encoded[0] & FLAG_ENCRYPTED != 0);
        assert!(encoded[0] & FLAG_EC != 0);
    }

    #[test]
    fn test_short_key_normalization() {
        let short_key = b"short";
        let params = params_with_key(short_key);
        let payload = b"test with short key";
        let encoded = encode_payload(payload, &params).unwrap();
        let decoded = decode_payload(&encoded, &params).unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn test_long_key_normalization() {
        let long_key = b"this is a very long key that is definitely more than 32 bytes long";
        let params = params_with_key(long_key);
        let payload = b"test with long key";
        let encoded = encode_payload(payload, &params).unwrap();
        let decoded = decode_payload(&encoded, &params).unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn test_empty_payload_roundtrip() {
        let key = b"key for empty payload test!!!!!!"; // 32 bytes
        let params = params_with_both(key, ECLevel::Low);
        let payload = b"";
        let encoded = encode_payload(payload, &params).unwrap();
        let decoded = decode_payload(&encoded, &params).unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn test_empty_payload_all_ec_levels() {
        let payload = b"";
        for level in [ECLevel::None, ECLevel::Medium, ECLevel::High, ECLevel::Maximum] {
            let params = params_with_ec(level);
            let encoded = encode_payload(payload, &params).unwrap();
            let decoded = decode_payload(&encoded, &params).unwrap();
            assert_eq!(decoded, payload, "failed for {level:?}");
        }
    }

    #[test]
    fn test_large_payload_roundtrip() {
        let params = params_with_ec(ECLevel::Low);
        let payload = vec![0xABu8; 5000]; // 5 KB, spans many RS blocks
        let encoded = encode_payload(&payload, &params).unwrap();
        let decoded = decode_payload(&encoded, &params).unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn test_rs_encode_decode_correctness() {
        let data = b"Reed-Solomon test data with some content";
        let ecc_len = 16;
        let encoded = rs_encode(data, ecc_len).unwrap();
        let decoded = rs_decode(&encoded, ecc_len).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_rs_encode_block_size() {
        // Verify encoded output is [4-byte header] + [N * 255 bytes]
        let data = vec![1u8; 300]; // > 255 - 16 = 239 bytes → 2 blocks
        let ecc_len = 16;
        let encoded = rs_encode(&data, ecc_len).unwrap();
        // 2 blocks = 510 bytes + 4 header = 514 bytes
        assert_eq!(encoded.len(), 4 + 2 * 255);
    }

    #[test]
    fn test_normalize_key_32_bytes() {
        let key = [0x42u8; 32];
        let normalized = normalize_key(&key);
        assert_eq!(normalized, key);
    }

    #[test]
    fn test_normalize_key_other_lengths() {
        // Different lengths should produce different (deterministic) 32-byte keys
        let k1 = normalize_key(b"short");
        let k2 = normalize_key(b"short"); // same → same
        assert_eq!(k1, k2);

        let k3 = normalize_key(b"different");
        assert_ne!(k1, k3);
    }

    #[test]
    fn test_decode_wrong_key_fails() {
        let key1 = b"correct encryption key 32 bytes!";
        let key2 = b"wrong   encryption key 32 bytes!";
        let params1 = params_with_key(key1);
        let params2 = params_with_key(key2);
        let payload = b"secret";
        let encoded = encode_payload(payload, &params1).unwrap();
        let result = decode_payload(&encoded, &params2);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_missing_key_fails() {
        let key = b"encryption key for this test!!!!"; // 32 bytes
        let params_enc = params_with_key(key);
        let params_no_key = no_codec_params();
        let payload = b"needs a key";
        let encoded = encode_payload(payload, &params_enc).unwrap();
        let result = decode_payload(&encoded, &params_no_key);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_empty_fails() {
        let params = no_codec_params();
        let result = decode_payload(&[], &params);
        assert!(result.is_err());
    }
}
