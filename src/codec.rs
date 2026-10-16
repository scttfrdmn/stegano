//! Payload encoding pipeline (wire format v2): Argon2id + AEAD encryption and
//! Reed-Solomon error correction.
//!
//! Layout (see `docs/WIRE_FORMAT.md` for full details):
//!
//! ```text
//! header (36 bytes):
//!   [version=2][flags][cipher][ecc_len][salt 16][nonce 12][len u32 LE]
//! body (len bytes):
//!   ciphertext ‖ 16-byte tag   (if FLAG_ENCRYPTED)
//!   plaintext                  (otherwise)
//!
//! ecc_len == 0:  header ‖ body
//! ecc_len  > 0:  RS(header) [36 + ecc_len bytes] ‖ RS(body) [ceil(len / (255-ecc_len)) × 255 bytes]
//! ```
//!
//! When encrypted, the key is `Argon2id(password, salt)` and all 36 header
//! bytes are the AEAD associated data, so flags, cipher, EC level, salt,
//! nonce and length are authenticated.  The header is protected by the same
//! RS parity length as the body, so the chosen EC level covers it too.
//!
//! Decoding auto-detects the EC level (trying `params.error_correction_level`
//! first) and the cipher.  A password must be supplied iff the payload is
//! encrypted; a mismatch in either direction is an error (no silent
//! downgrade).  There is no decode path for the pre-v0.6 format.

use crate::core::{CipherKind, EmbedParams, Result, SteganoError};
use crate::crypto::{self, NONCE_LEN, SALT_LEN, TAG_LEN};
use crate::error_correction::{
    ecc_len_for, rs_block_count, rs_correct_block, rs_decode_stream, rs_encode_block,
    rs_encode_stream, ECC_LENS, RS_BLOCK,
};

/// Wire format version written by this crate.
pub const WIRE_VERSION: u8 = 2;
/// Size of the (unprotected) header in bytes.
pub const HEADER_LEN: usize = 4 + SALT_LEN + NONCE_LEN + 4;

/// Flag: body is AEAD-encrypted.
const FLAG_ENCRYPTED: u8 = 0x01;
/// Flag: header and body are Reed-Solomon protected (`ecc_len > 0`).
const FLAG_EC: u8 = 0x02;
const KNOWN_FLAGS: u8 = FLAG_ENCRYPTED | FLAG_EC;

const OFF_SALT: usize = 4;
const OFF_NONCE: usize = OFF_SALT + SALT_LEN;
const OFF_LEN: usize = OFF_NONCE + NONCE_LEN;

/// Parsed v2 header.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Header {
    flags: u8,
    cipher: CipherKind,
    ecc_len: usize,
    salt: [u8; SALT_LEN],
    nonce: [u8; NONCE_LEN],
    len: u32,
}

impl Header {
    fn encrypted(&self) -> bool {
        self.flags & FLAG_ENCRYPTED != 0
    }

    fn to_bytes(&self) -> [u8; HEADER_LEN] {
        let mut h = [0u8; HEADER_LEN];
        h[0] = WIRE_VERSION;
        h[1] = self.flags;
        h[2] = self.cipher.to_u8();
        h[3] = self.ecc_len as u8;
        h[OFF_SALT..OFF_NONCE].copy_from_slice(&self.salt);
        h[OFF_NONCE..OFF_LEN].copy_from_slice(&self.nonce);
        h[OFF_LEN..].copy_from_slice(&self.len.to_le_bytes());
        h
    }

    /// Parse and validate a header, requiring its `ecc_len` to equal
    /// `expected_ecc` (the parity length it was decoded with).
    fn parse(h: &[u8], expected_ecc: usize) -> Result<Self> {
        let bad = |m: &str| Err(SteganoError::IntegrityError(format!("Invalid header: {m}")));
        if h.len() < HEADER_LEN {
            return bad("truncated");
        }
        if h[0] != WIRE_VERSION {
            return bad("unsupported wire format version");
        }
        let flags = h[1];
        if flags & !KNOWN_FLAGS != 0 {
            return bad("unknown flags");
        }
        let ecc_len = h[3] as usize;
        if ecc_len != expected_ecc || !ECC_LENS.contains(&ecc_len) {
            return bad("EC length mismatch");
        }
        if (flags & FLAG_EC != 0) != (ecc_len != 0) {
            return bad("EC flag inconsistent with EC length");
        }
        let cipher = CipherKind::from_u8(h[2]).ok_or_else(|| {
            SteganoError::IntegrityError(format!("Unknown cipher kind byte: {}", h[2]))
        })?;
        let mut salt = [0u8; SALT_LEN];
        salt.copy_from_slice(&h[OFF_SALT..OFF_NONCE]);
        let mut nonce = [0u8; NONCE_LEN];
        nonce.copy_from_slice(&h[OFF_NONCE..OFF_LEN]);
        let len = u32::from_le_bytes([h[OFF_LEN], h[OFF_LEN + 1], h[OFF_LEN + 2], h[OFF_LEN + 3]]);
        let hdr = Header {
            flags,
            cipher,
            ecc_len,
            salt,
            nonce,
            len,
        };
        if !hdr.encrypted() {
            if h[2] != 0 || salt != [0; SALT_LEN] || nonce != [0; NONCE_LEN] {
                return bad("crypto fields set on unencrypted payload");
            }
        } else if (len as usize) < TAG_LEN {
            return bad("ciphertext shorter than tag");
        }
        Ok(hdr)
    }
}

/// Encode a payload, optionally applying encryption and Reed-Solomon error
/// correction as specified by `params`.
///
/// Returns a self-framing byte sequence that [`decode_payload`] can reverse.
pub fn encode_payload(payload: &[u8], params: &EmbedParams) -> crate::core::Result<Vec<u8>> {
    let ecc_len = ecc_len_for(params.error_correction_level);
    let encrypted = !params.encryption_key.is_empty();

    let body_len = if encrypted {
        payload.len().checked_add(TAG_LEN)
    } else {
        Some(payload.len())
    };
    let len = body_len
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| {
            SteganoError::InvalidParameter("Payload too large for wire format (> u32::MAX)".into())
        })?;

    let mut flags = 0u8;
    if encrypted {
        flags |= FLAG_ENCRYPTED;
    }
    if ecc_len > 0 {
        flags |= FLAG_EC;
    }

    let (cipher, salt, nonce) = if encrypted {
        (
            params.cipher_kind,
            crypto::random_array::<SALT_LEN>(),
            crypto::random_array::<NONCE_LEN>(),
        )
    } else {
        // Canonical zero crypto fields when unencrypted.
        (CipherKind::Aes256Gcm, [0u8; SALT_LEN], [0u8; NONCE_LEN])
    };

    let header = Header {
        flags,
        cipher,
        ecc_len,
        salt,
        nonce,
        len,
    };
    let header_bytes = header.to_bytes();

    let ciphertext;
    let body: &[u8] = if encrypted {
        let key = crypto::derive_key(&params.encryption_key, &salt)?;
        ciphertext = crypto::aead_seal(cipher, &key, &nonce, &header_bytes, payload)?;
        &ciphertext
    } else {
        payload
    };
    debug_assert_eq!(body.len(), len as usize);

    if ecc_len == 0 {
        let mut out = Vec::with_capacity(HEADER_LEN + body.len());
        out.extend_from_slice(&header_bytes);
        out.extend_from_slice(body);
        Ok(out)
    } else {
        let mut out = rs_encode_block(&header_bytes, ecc_len)?;
        out.extend_from_slice(&rs_encode_stream(body, ecc_len)?);
        Ok(out)
    }
}

/// Try to recover and validate the header assuming `ecc_len` parity bytes.
fn try_header(data: &[u8], ecc_len: usize) -> Result<(Header, usize)> {
    let hdr_block = HEADER_LEN + ecc_len;
    if data.len() < hdr_block {
        return Err(SteganoError::IntegrityError(
            "Payload too short for header".to_string(),
        ));
    }
    let header = if ecc_len == 0 {
        Header::parse(&data[..HEADER_LEN], 0)?
    } else {
        let corrected = rs_correct_block(&data[..hdr_block], ecc_len)?;
        Header::parse(&corrected, ecc_len)?
    };
    Ok((header, hdr_block))
}

/// Cheap structural check: does `data` start with a valid v2 payload?
///
/// Requires a header that parses (after RS correction, for any allowed EC
/// level) and whose declared body length fits in the remaining bytes.  No
/// key derivation or decryption is performed, so this cannot tell whether a
/// password is correct — only whether a payload is plausibly present.
pub(crate) fn looks_like_payload(data: &[u8]) -> bool {
    ECC_LENS.into_iter().any(|ecc| {
        let Ok((header, body_off)) = try_header(data, ecc) else {
            return false;
        };
        let len = header.len as usize;
        let need = if ecc == 0 {
            Some(len)
        } else {
            rs_block_count(len, ecc).checked_mul(RS_BLOCK)
        };
        need.is_some_and(|n| n <= data.len() - body_off)
    })
}

/// Decode a payload previously encoded with [`encode_payload`].
///
/// The EC level and cipher are detected from the payload.  `params` supplies
/// the password (which must be present iff the payload is encrypted); its
/// `error_correction_level` is only used as the first EC level to try.
/// Trailing bytes after the encoded payload are ignored.
pub fn decode_payload(data: &[u8], params: &EmbedParams) -> crate::core::Result<Vec<u8>> {
    if data.is_empty() {
        return Err(SteganoError::IntegrityError(
            "Empty codec payload".to_string(),
        ));
    }

    // Locate the header: try the caller's EC level first, then the others.
    let preferred = ecc_len_for(params.error_correction_level);
    let mut found = None;
    let mut last_err = None;
    for ecc in std::iter::once(preferred).chain(ECC_LENS.into_iter().filter(|&e| e != preferred)) {
        match try_header(data, ecc) {
            Ok(h) => {
                found = Some(h);
                break;
            }
            Err(e) => last_err = Some(e),
        }
    }
    let (header, body_off) = match found {
        Some(h) => h,
        None => {
            return Err(last_err.unwrap_or_else(|| {
                SteganoError::IntegrityError("No valid payload header".to_string())
            }))
        }
    };

    // No silent downgrade / no keyless decrypt.
    let have_key = !params.encryption_key.is_empty();
    match (header.encrypted(), have_key) {
        (false, true) => {
            return Err(SteganoError::EncryptionError(
                "A password was supplied but the payload is not encrypted".to_string(),
            ))
        }
        (true, false) => {
            return Err(SteganoError::EncryptionError(
                "Encryption key required for decryption".to_string(),
            ))
        }
        _ => {}
    }

    // Validate the declared length against the available bytes before
    // allocating anything sized by it.
    let len = header.len as usize;
    let rest = &data[body_off..];
    let body = if header.ecc_len == 0 {
        if rest.len() < len {
            return Err(SteganoError::IntegrityError(
                "Payload shorter than declared length".to_string(),
            ));
        }
        rest[..len].to_vec()
    } else {
        rs_decode_stream(rest, len, header.ecc_len)?
    };

    if !header.encrypted() {
        return Ok(body);
    }
    let key = crypto::derive_key(&params.encryption_key, &header.salt)?;
    crypto::aead_open(
        header.cipher,
        &key,
        &header.nonce,
        &header.to_bytes(),
        &body,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ECLevel;

    const LEVELS: [ECLevel; 5] = [
        ECLevel::None,
        ECLevel::Low,
        ECLevel::Medium,
        ECLevel::High,
        ECLevel::Maximum,
    ];

    fn params(key: &[u8], level: ECLevel) -> EmbedParams {
        EmbedParams {
            encryption_key: key.to_vec(),
            error_correction_level: level,
            ..Default::default()
        }
    }

    fn params_cipher(key: &[u8], level: ECLevel, cipher: CipherKind) -> EmbedParams {
        EmbedParams {
            cipher_kind: cipher,
            ..params(key, level)
        }
    }

    #[test]
    fn test_roundtrip_all_levels_both_ciphers() {
        let payload = b"secret payload data";
        for level in LEVELS {
            for cipher in [CipherKind::Aes256Gcm, CipherKind::ChaCha20Poly1305] {
                let p = params_cipher(b"pw", level, cipher);
                let enc = encode_payload(payload, &p).unwrap();
                assert_eq!(enc[0], WIRE_VERSION);
                assert_eq!(
                    decode_payload(&enc, &p).unwrap(),
                    payload,
                    "{level:?} {cipher:?}"
                );
            }
            let p = params(b"", level);
            let enc = encode_payload(payload, &p).unwrap();
            assert_eq!(
                decode_payload(&enc, &p).unwrap(),
                payload,
                "{level:?} plain"
            );
        }
    }

    #[test]
    fn test_empty_and_large_payloads() {
        for level in LEVELS {
            for key in [&b""[..], b"pw"] {
                let p = params(key, level);
                let enc = encode_payload(b"", &p).unwrap();
                assert!(decode_payload(&enc, &p).unwrap().is_empty());
            }
        }
        let big = vec![0xABu8; 5000];
        let p = params(b"pw", ECLevel::Low);
        let enc = encode_payload(&big, &p).unwrap();
        assert_eq!(decode_payload(&enc, &p).unwrap(), big);
    }

    #[test]
    fn test_layout_no_ec() {
        let p = params(b"", ECLevel::None);
        let enc = encode_payload(b"hello", &p).unwrap();
        assert_eq!(enc.len(), HEADER_LEN + 5);
        assert_eq!(enc[0], WIRE_VERSION);
        assert_eq!(enc[1], 0);
        assert_eq!(&enc[HEADER_LEN..], b"hello");

        let p = params(b"pw", ECLevel::None);
        let enc = encode_payload(b"hello", &p).unwrap();
        assert_eq!(enc.len(), HEADER_LEN + 5 + TAG_LEN);
        assert_eq!(enc[1], FLAG_ENCRYPTED);
    }

    #[test]
    fn test_layout_ec() {
        let p = params(b"pw", ECLevel::Medium);
        let enc = encode_payload(&[1u8; 300], &p).unwrap();
        // header block + ceil(316 / 223) = 2 body blocks
        assert_eq!(enc.len(), HEADER_LEN + 32 + 2 * 255);
        assert_eq!(enc[1], FLAG_ENCRYPTED | FLAG_EC);
        assert_eq!(enc[3], 32);
    }

    #[test]
    fn test_ec_auto_detected() {
        let enc = encode_payload(b"data", &params(b"pw", ECLevel::High)).unwrap();
        assert_eq!(
            decode_payload(&enc, &params(b"pw", ECLevel::None)).unwrap(),
            b"data"
        );
    }

    #[test]
    fn test_trailing_bytes_ignored() {
        for level in [ECLevel::None, ECLevel::Low] {
            let p = params(b"pw", level);
            let mut enc = encode_payload(b"data", &p).unwrap();
            enc.extend_from_slice(&[0x5A; 300]);
            assert_eq!(decode_payload(&enc, &p).unwrap(), b"data");
        }
    }

    #[test]
    fn test_wrong_password_is_crypto_error() {
        let enc = encode_payload(b"secret", &params(b"right", ECLevel::Low)).unwrap();
        assert!(matches!(
            decode_payload(&enc, &params(b"wrong", ECLevel::Low)),
            Err(SteganoError::EncryptionError(_))
        ));
    }

    #[test]
    fn test_missing_key_fails() {
        let enc = encode_payload(b"needs a key", &params(b"pw", ECLevel::None)).unwrap();
        assert!(matches!(
            decode_payload(&enc, &params(b"", ECLevel::None)),
            Err(SteganoError::EncryptionError(_))
        ));
    }

    #[test]
    fn test_key_supplied_but_unencrypted_fails() {
        let enc = encode_payload(b"plain", &params(b"", ECLevel::Low)).unwrap();
        assert!(matches!(
            decode_payload(&enc, &params(b"pw", ECLevel::Low)),
            Err(SteganoError::EncryptionError(_))
        ));
    }

    #[test]
    fn test_tamper_header_fields_fails_without_ec() {
        let p = params(b"pw", ECLevel::None);
        let enc = encode_payload(b"attack at dawn", &p).unwrap();

        // Clearing FLAG_ENCRYPTED must not yield attacker-controlled plaintext.
        let mut t = enc.clone();
        t[1] &= !FLAG_ENCRYPTED;
        assert!(decode_payload(&t, &p).is_err());

        // Cipher byte swap (authenticated via AAD).
        let mut t = enc.clone();
        t[2] = CipherKind::ChaCha20Poly1305.to_u8();
        assert!(decode_payload(&t, &p).is_err());

        // Unknown cipher.
        let mut t = enc.clone();
        t[2] = 0x7F;
        assert!(decode_payload(&t, &p).is_err());

        // Length shortened / lengthened.
        for delta in [-1i64, 1] {
            let mut t = enc.clone();
            let l = u32::from_le_bytes(t[OFF_LEN..OFF_LEN + 4].try_into().unwrap());
            let nl = (l as i64 + delta) as u32;
            t[OFF_LEN..OFF_LEN + 4].copy_from_slice(&nl.to_le_bytes());
            t.push(0); // ensure enough bytes for the lengthened case
            assert!(decode_payload(&t, &p).is_err());
        }

        // Salt and nonce.
        for off in [OFF_SALT, OFF_NONCE] {
            let mut t = enc.clone();
            t[off] ^= 1;
            assert!(matches!(
                decode_payload(&t, &p),
                Err(SteganoError::EncryptionError(_))
            ));
        }

        // Ciphertext.
        let mut t = enc.clone();
        t[HEADER_LEN] ^= 1;
        assert!(decode_payload(&t, &p).is_err());

        // Version.
        let mut t = enc;
        t[0] = 1;
        assert!(decode_payload(&t, &p).is_err());
    }

    #[test]
    fn test_header_bit_flips_corrected_with_ec() {
        for level in [
            ECLevel::Low,
            ECLevel::Medium,
            ECLevel::High,
            ECLevel::Maximum,
        ] {
            for key in [&b"pw"[..], b""] {
                let p = params(key, level);
                let enc = encode_payload(b"robust header", &p).unwrap();
                // Flip one bit in every header byte position, one at a time.
                for i in 0..HEADER_LEN {
                    let mut t = enc.clone();
                    t[i] ^= 0x10;
                    assert_eq!(
                        decode_payload(&t, &p).unwrap(),
                        b"robust header",
                        "{level:?} byte {i}"
                    );
                }
                // Several corrupted header bytes at once (within capacity).
                let mut t = enc.clone();
                for b in t.iter_mut().take(ecc_len_for(level) / 2) {
                    *b ^= 0xFF;
                }
                assert_eq!(decode_payload(&t, &p).unwrap(), b"robust header");
            }
        }
    }

    #[test]
    fn test_huge_declared_length_rejected_without_allocation() {
        for level in LEVELS {
            let ecc = ecc_len_for(level);
            let header = Header {
                flags: if ecc > 0 { FLAG_EC } else { 0 },
                cipher: CipherKind::Aes256Gcm,
                ecc_len: ecc,
                salt: [0; SALT_LEN],
                nonce: [0; NONCE_LEN],
                len: u32::MAX,
            };
            let mut crafted = if ecc == 0 {
                header.to_bytes().to_vec()
            } else {
                rs_encode_block(&header.to_bytes(), ecc).unwrap()
            };
            crafted.extend_from_slice(&[0u8; 64]);
            assert!(matches!(
                decode_payload(&crafted, &params(b"", level)),
                Err(SteganoError::IntegrityError(_))
            ));
        }
    }

    #[test]
    fn test_garbage_and_truncated_input() {
        let p = params(b"pw", ECLevel::Medium);
        assert!(decode_payload(&[], &p).is_err());
        assert!(decode_payload(&[0u8; 10], &p).is_err());
        assert!(decode_payload(&[0xFFu8; 600], &p).is_err());
        let enc = encode_payload(&[9u8; 400], &p).unwrap();
        assert!(decode_payload(&enc[..enc.len() - 1], &p).is_err());
    }

    #[test]
    fn test_looks_like_payload() {
        for level in LEVELS {
            for key in [&b""[..], b"pw"] {
                let enc = encode_payload(b"present", &params(key, level)).unwrap();
                assert!(looks_like_payload(&enc), "{level:?}");
                assert!(!looks_like_payload(&enc[..enc.len() - 1]), "{level:?}");
            }
        }
        assert!(!looks_like_payload(&[]));
        assert!(!looks_like_payload(&[0u8; 4096]));
        assert!(!looks_like_payload(&[0xFFu8; 4096]));
    }

    #[test]
    fn test_unencrypted_with_crypto_fields_rejected() {
        let p = params(b"", ECLevel::None);
        let mut enc = encode_payload(b"x", &p).unwrap();
        enc[OFF_SALT] = 1;
        assert!(decode_payload(&enc, &p).is_err());
    }
}
