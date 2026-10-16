//! Reed-Solomon error correction used by the payload wire format.
//!
//! Each RS codeword is at most 255 bytes (GF(2^8)).  With `ecc_len` parity
//! bytes a codeword can correct up to `ecc_len / 2` corrupted bytes.
//!
//! The previous XOR-parity `IntegritySystem` was removed in v0.6.0: it did not
//! actually correct errors.  All error correction now goes through
//! [`crate::codec`]; see `docs/WIRE_FORMAT.md`.

use crate::core::{ECLevel, Result, SteganoError};
use reed_solomon::{Decoder, Encoder};

/// Size of a full Reed-Solomon codeword in bytes.
pub const RS_BLOCK: usize = 255;

/// Map an [`ECLevel`] to the number of Reed-Solomon parity bytes per block.
///
/// Returns 0 for `ECLevel::None` (no error correction).
pub fn ecc_len_for(level: ECLevel) -> usize {
    match level {
        ECLevel::None => 0,
        ECLevel::Low => 16,      // corrects 8 bytes per 255-byte block
        ECLevel::Medium => 32,   // corrects 16 bytes per block
        ECLevel::High => 64,     // corrects 32 bytes per block
        ECLevel::Maximum => 128, // corrects 64 bytes per block
    }
}

/// All parity lengths produced by [`ecc_len_for`], including 0.
pub(crate) const ECC_LENS: [usize; 5] = [0, 16, 32, 64, 128];

/// RS-encode a single message of at most `255 - ecc_len` bytes, returning
/// `message ‖ parity` (a shortened codeword of `message.len() + ecc_len` bytes).
pub(crate) fn rs_encode_block(message: &[u8], ecc_len: usize) -> Result<Vec<u8>> {
    if ecc_len == 0 || ecc_len >= RS_BLOCK || message.len() + ecc_len > RS_BLOCK {
        return Err(SteganoError::InvalidParameter(format!(
            "Invalid RS block: {} data bytes with {} parity bytes",
            message.len(),
            ecc_len
        )));
    }
    Ok(Encoder::new(ecc_len).encode(message)[..].to_vec())
}

/// Correct a single codeword produced by [`rs_encode_block`], returning the
/// message part.
pub(crate) fn rs_correct_block(codeword: &[u8], ecc_len: usize) -> Result<Vec<u8>> {
    if ecc_len == 0 || codeword.len() <= ecc_len || codeword.len() > RS_BLOCK {
        return Err(SteganoError::IntegrityError(
            "Invalid Reed-Solomon codeword length".to_string(),
        ));
    }
    Decoder::new(ecc_len)
        .correct(codeword, None)
        .map(|b| b.data().to_vec())
        .map_err(|_| SteganoError::IntegrityError("Reed-Solomon error correction failed".into()))
}

/// Number of 255-byte blocks needed to carry `len` data bytes at `ecc_len`.
pub(crate) fn rs_block_count(len: usize, ecc_len: usize) -> usize {
    len.div_ceil(RS_BLOCK - ecc_len)
}

/// RS-encode arbitrary-length data as a sequence of full 255-byte codewords.
/// The final chunk is zero-padded; the caller must record the true length.
pub(crate) fn rs_encode_stream(data: &[u8], ecc_len: usize) -> Result<Vec<u8>> {
    let k = RS_BLOCK - ecc_len;
    let blocks = rs_block_count(data.len(), ecc_len);
    let cap = blocks.checked_mul(RS_BLOCK).ok_or_else(|| {
        SteganoError::InvalidParameter("Payload too large for Reed-Solomon encoding".into())
    })?;
    let mut out = Vec::with_capacity(cap);
    let mut padded = vec![0u8; k];
    for chunk in data.chunks(k) {
        padded.fill(0);
        padded[..chunk.len()].copy_from_slice(chunk);
        out.extend_from_slice(&rs_encode_block(&padded, ecc_len)?);
    }
    Ok(out)
}

/// Decode `len` data bytes from full 255-byte codewords produced by
/// [`rs_encode_stream`].  `encoded` must contain at least
/// `rs_block_count(len, ecc_len) * 255` bytes; this is checked before any
/// allocation.
pub(crate) fn rs_decode_stream(encoded: &[u8], len: usize, ecc_len: usize) -> Result<Vec<u8>> {
    let blocks = rs_block_count(len, ecc_len);
    let need = blocks
        .checked_mul(RS_BLOCK)
        .filter(|&n| n <= encoded.len())
        .ok_or_else(|| {
            SteganoError::IntegrityError("RS data shorter than declared length".to_string())
        })?;
    let mut out = Vec::with_capacity(blocks * (RS_BLOCK - ecc_len));
    for cw in encoded[..need].chunks(RS_BLOCK) {
        out.extend_from_slice(&rs_correct_block(cw, ecc_len)?);
    }
    out.truncate(len);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_roundtrip_with_errors() {
        let msg = b"header bytes to protect";
        let mut cw = rs_encode_block(msg, 16).unwrap();
        assert_eq!(cw.len(), msg.len() + 16);
        for i in 0..8 {
            cw[i * 3] ^= 0xFF;
        }
        assert_eq!(rs_correct_block(&cw, 16).unwrap(), msg);
    }

    #[test]
    fn test_stream_roundtrip() {
        for &ecc in &ECC_LENS[1..] {
            let data: Vec<u8> = (0..1000u32).map(|i| i as u8).collect();
            let mut enc = rs_encode_stream(&data, ecc).unwrap();
            assert_eq!(enc.len(), rs_block_count(data.len(), ecc) * RS_BLOCK);
            enc[10] ^= 0x55;
            assert_eq!(rs_decode_stream(&enc, data.len(), ecc).unwrap(), data);
        }
    }

    #[test]
    fn test_stream_rejects_short_input() {
        assert!(rs_decode_stream(&[0u8; 100], 1000, 16).is_err());
        assert!(rs_decode_stream(&[], usize::MAX, 16).is_err());
    }

    #[test]
    fn test_empty_stream() {
        assert!(rs_encode_stream(&[], 32).unwrap().is_empty());
        assert!(rs_decode_stream(&[], 0, 32).unwrap().is_empty());
    }
}
