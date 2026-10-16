# Payload Wire Format v2

This document specifies the byte format produced by `stegano::codec::encode_payload`
and consumed by `stegano::codec::decode_payload` (v0.6.0 and later). Every format
handler that applies encryption or error correction embeds these bytes in its
carrier. It replaces the v0.5 format. **There is no decoder for the old format.**
To recover data embedded with v0.5.x, extract it with v0.5.x.

## Overview

```text
header (36 bytes, all little-endian):
  off len field
  0   1   version   = 0x02
  1   1   flags     bit0 = ENCRYPTED, bit1 = EC; any other bit set => reject
  2   1   cipher    0 = AES-256-GCM, 1 = ChaCha20-Poly1305 (0 when unencrypted)
  3   1   ecc_len   RS parity bytes per block: 0, 16, 32, 64 or 128
  4   16  salt      Argon2id salt (all zero when unencrypted)
  20  12  nonce     AEAD nonce    (all zero when unencrypted)
  32  4   len       u32 body length in bytes

body (len bytes):
  ENCRYPTED: ciphertext || 16-byte AEAD tag   (len = plaintext_len + 16)
  otherwise: plaintext
```

### Framing on the carrier

| `ecc_len` | Bytes emitted |
|-----------|---------------|
| 0  | `header (36)` followed by `body (len)` |
| >0 | `RS(header)` (36 + ecc_len bytes, a shortened RS codeword), followed by `RS(body)`, which is `ceil(len / (255 - ecc_len))` full 255-byte codewords. The final data chunk is zero-padded. |

`ecc_len` maps from `ECLevel` as follows:

| ECLevel | ecc_len | Correctable bytes per codeword |
|---------|---------|--------------------------------|
| None    | 0   | 0  |
| Low     | 16  | 8  |
| Medium  | 32  | 16 |
| High    | 64  | 32 |
| Maximum | 128 | 64 |

The header has its own RS codeword with the same parity length as the body. A
corrupted header byte is therefore corrected at any EC level other than `None`.

## Encryption

* **KDF:** Argon2id (v0x13), with a random 16-byte salt for each payload and a
  32-byte output.
  * Parameters: **m = 65536 KiB (64 MiB), t = 3, p = 1** (`crypto::PRODUCTION_KDF`).
  * These parameters are fixed by wire version 2 and are not stored in the
    payload. Changing them requires a new version byte.
  * The crate's own unit tests (`cfg(test)`) use cheaper parameters. Every other
    build, including integration tests and downstream crates, uses the
    production parameters.
* **Key input:** the password is always `EmbedParams::encryption_key` passed
  through Argon2id. No key length is treated as a raw key.
* **AEAD:** AES-256-GCM or ChaCha20-Poly1305 with a 96-bit random nonce and a
  128-bit tag.
* **Associated data:** all 36 header bytes, as decoded. The AEAD therefore
  authenticates the version, flags, cipher, EC length, salt, nonce and length.
  Tampering with any of them makes decryption fail.
* **No keyless hash:** there is no unkeyed digest of the ciphertext. The AEAD
  tag provides integrity, and a party without the password cannot check whether
  hidden data is present by recomputing a hash.

## Decoding rules

1. **Find the header.** Try `ecc_len = ecc_len_for(params.error_correction_level)`
   first, then the other permitted values. A candidate is accepted when all of
   the following hold:
   * The header bytes (after RS correction, if `ecc_len > 0`) have `version = 2`.
   * No unknown flag bits are set.
   * The `ecc_len` field equals the candidate.
   * The EC flag is set exactly when `ecc_len > 0`.
   * The cipher byte is known.
   * For unencrypted payloads, the cipher, salt and nonce fields are zero.
   * For encrypted payloads, `len >= 16`.

   Because the EC level is detected this way, a payload decodes even if the
   reader's `params.error_correction_level` differs from the writer's.
2. **Check the key policy.**
   * A password is supplied but `ENCRYPTED` is clear: `EncryptionError`. An
     attacker cannot strip encryption and substitute plaintext.
   * No password is supplied but `ENCRYPTED` is set: `EncryptionError`.
3. **Check lengths before allocating.** The decoder confirms that the remaining
   input holds at least `len` bytes (no EC) or `ceil(len / (255 - ecc_len)) × 255`
   bytes (EC). Only then does it allocate anything sized by `len`. A crafted
   `len = u32::MAX` returns `IntegrityError` without allocating.
4. **Decode the body.** Apply RS correction to each codeword, then truncate to `len`.
5. **Decrypt.** Derive the key with `Argon2id(password, salt)` and open the AEAD
   with the header as associated data. If the password is wrong or the payload
   was tampered with, the result is `EncryptionError`.

The decoder ignores any bytes after the encoded payload, such as carrier
padding.

## Limits

* `len` is a u32. Encoding a body longer than `u32::MAX` bytes returns
  `InvalidParameter`; lengths are never silently truncated.

## Secrets handling

* Derived keys are held in `zeroize::Zeroizing` buffers. `CryptoLayer` stores
  its password or raw key in zeroizing storage.
* `EmbedParams` implements `Debug` manually and shows `encryption_key` as
  `<redacted>`. The field has `#[serde(skip)]`, so it is never serialized and is
  always empty after deserialization.

## `CryptoLayer::serialize_payload` (standalone helper)

`CryptoLayer` is a lower-level helper and is separate from the codec format:

```text
[version=2][cipher][salt 16][nonce 12][len u32 LE][ciphertext || tag]
```

The first 34 bytes are its AEAD associated data. A layer created from a
password derives its key per payload as `Argon2id(password, salt)`. A layer
created with `new_from_key` uses its 32-byte key directly and ignores the salt
for key derivation, though the salt is still authenticated.
