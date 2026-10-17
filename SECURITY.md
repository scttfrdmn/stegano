# Security Policy

## Supported versions

Stegano is pre-1.0. Security fixes are made on the latest minor release
line and, where practical, backported to the previous one.

| Version | Supported |
| ------- | --------- |
| 0.8.x   | Yes       |
| 0.7.x   | Yes       |
| < 0.6   | No. The wire format changed in 0.6.0, and older releases used weaker key derivation. |

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub:
**[Report a vulnerability](https://github.com/scttfrdmn/stegano/security/advisories/new)**
(Security tab → "Report a vulnerability"). This opens a private security
advisory that only the maintainer can see. Do not open a public issue for a
vulnerability.

If you cannot use GitHub, email scttfrdmn@gmail.com with a subject starting
with `SECURITY:`.

Please include:

- the affected version or commit, and the interface (CLI, Rust API, C/FFI,
  Go, Python, WASM);
- steps or a minimal input file to reproduce the problem;
- what you think the impact is.

This is a project maintained by one person. You will get an acknowledgement
as soon as possible, normally within a few days. The fix and the advisory
will be coordinated with you, and you will be credited unless you prefer not
to be.

Reports of "a payload can be detected" are expected behaviour, not
vulnerabilities (see below). Ways to recover a payload's plaintext without
the password, to forge an encrypted payload, or to crash or corrupt memory
through the C ABI or a malformed carrier are in scope.

## Cryptography

When a password is supplied, the payload is encoded as described in
[docs/WIRE_FORMAT.md](docs/WIRE_FORMAT.md):

- **Key derivation:** Argon2id with m = 64 MiB, t = 3 passes, p = 1 lane, and a
  random 16-byte salt for every payload.
- **Encryption:** AES-256-GCM (default) or ChaCha20-Poly1305, with a random
  12-byte nonce and a 16-byte authentication tag.
- **Authenticated header:** the 36-byte payload header (version, flags,
  cipher, error-correction level, salt, nonce, length) is the AEAD associated
  data. Changing any header field or ciphertext byte makes decryption fail;
  it never yields altered plaintext.
- **No downgrade:** extraction requires a password exactly when the payload is
  encrypted. Clearing the "encrypted" flag does not make the library return
  attacker-chosen plaintext.
- **Secrets in memory:** `EmbedParams` redacts the password from `Debug`
  output and never serializes it. Derived keys are zeroized after use.

The primitives come from the RustCrypto crates `argon2`, `aes-gcm` and
`chacha20poly1305`. Stegano does not implement any cryptographic primitive
itself. The code has not had an independent security audit.

## Threat model and limitations

Stegano is built to keep a payload **confidential** (with a password) and to
keep the carrier a **valid file** that standard tools still accept. It is
**not** built to hide that a payload exists.

- **Payloads are detectable.** Payload bits are written sequentially from the
  start of each carrier's capacity, not at key-dependent positions. Every
  handler also writes a plaintext marker and length before the payload: for
  example `IMSJ` (PNG/TIFF), `GNSJ` (FASTA/FASTQ), `SBM` (SAM/BAM), `STEG=`
  INFO keys (VCF), `STGD`/`STEGANO` (DICOM), `FTSJ` (FITS) and `SGSJ`
  (signals). Anyone who knows the format can find the marker, and
  steganalysis such as Sample Pairs Analysis or the pairs-of-values test
  detects sequential LSB embedding. Some channels are visible to anyone who
  looks: FASTA header suffixes (`stg=`), extra VCF QUAL digits, SAM `zs:Z:`
  tags and DICOM private elements.
- **The `validation` module is there to measure this.** It includes Sample
  Pairs Analysis, pairs-of-values, chi-square and other detectors (see
  [docs/VALIDATION_GUIDE.md](docs/VALIDATION_GUIDE.md)). Use it to assess how
  visible an embedding is in your data. Its scores are measurements, not a
  guarantee of undetectability.
- **Stealth hardening is not scheduled.** Keyed embedding positions and the
  removal of plaintext markers are tracked in
  [#50](https://github.com/scttfrdmn/stegano/issues/50), but this work is not
  on the current roadmap. Do not count on it landing.
- **No password means plaintext.** Without a password (an empty or absent
  password in any interface: CLI, Rust, C, Go, Python or WASM), the payload
  is stored unencrypted inside the carrier. Anyone can read it, and nothing
  cryptographically protects it against modification.
- **Integrity and error correction have a narrow scope.** With a password,
  the AEAD tag detects any change to the payload or its header. Without one
  there is no integrity check. Optional Reed-Solomon error correction repairs
  a limited number of corrupted bytes in the embedded payload, the header
  included. It does not survive re-encoding the carrier: lossy compression,
  resampling, gain changes, re-saving an image in another format, or
  re-sorting or re-compressing alignment files will destroy the payload.
  `detect` / `stegano_verify_integrity` only check whether a well-formed
  stegano frame is present. They do not verify the password or the
  payload's authenticity.
- **Changes to scientific data.** LSB strategies alter sample values (by at
  most one unit in the least significant bit, or one mantissa LSB for FITS
  float data). Whether that is acceptable is up to you and your data.
- **Out of scope:** side channels (timing, memory use), password strength and
  storage, access control on the files, and the security of the transport
  you use to move them. On the CLI, prefer `--password-file`,
  `STEGANO_PASSWORD` or `--password-prompt` over `--password`, which is
  visible in `ps` and shell history.
- **Experimental modules** (`gpu`, `ml`, `monitoring`, enabled by the
  `experimental` feature) are unsupported and not part of the embedding path.

## Development practices

What is actually done today:

- **CI on every pull request and on `main`:** tests on Linux and macOS
  (`cargo test --all-targets` and `cargo test --doc`), `cargo clippy
  --all-targets -- -D warnings`, a build and test with the `experimental`
  feature, a wasm32 check, the npm package smoke tests, the Go binding tests,
  and a check that the generated C headers are up to date.
- **Format-validity suite:** `tests/format_validity/run.py` embeds with the
  release CLI and checks every output with the standard tool for its format
  (Pillow, `wave`, astropy, Biopython, bcftools, samtools, pydicom).
- **Secret scanning:** the repository history was scanned with gitleaks
  before the repository was made public.
- **FFI hardening:** every exported C function validates its arguments and
  runs inside `catch_unwind` (the release profile uses `panic = "unwind"`), so
  a panic is returned as an error code instead of unwinding into the caller.
- **npm releases** are published from CI with npm trusted publishing (OIDC),
  without a long-lived token.

There is no fuzzing, no formal or third-party security review, and no
signing of release artifacts. `deny.toml` is provided for `cargo deny`, but
dependency auditing (`cargo audit` / `cargo deny`) is not run in CI.
Contributions in any of these areas are welcome.
