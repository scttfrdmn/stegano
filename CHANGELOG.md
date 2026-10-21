# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- FASTQ quality-alphabet check ([#108](https://github.com/scttfrdmn/stegano/issues/108)):
  `stegano::validation::fastq_quality` builds a file's Phred histogram, infers or takes the
  binning scheme (Illumina RTA3 4-bin, Illumina 8-bin, or a custom list), counts quality values
  that are off the bins and records where they are, and can compare the histogram with an
  unmodified reference (chi-square, KL). `stegano analyze` prints the check for every FASTQ input
  and adds `--quality-bins <SCHEME|LIST>` and `--quality-reference <FILE>`.
  `cli::commands::handle_analyze_with` takes the new options.

### Documentation
- `docs/FORMATS.md`: the FASTQ quality strategy can be detected on binned data, because modified
  characters move off their bins (measured in
  [#107](https://github.com/scttfrdmn/stegano/issues/107)). Also adds a capacity table per
  alphabet. `docs/VALIDATION_GUIDE.md` documents the new detector.

## [0.8.0] - 2026-10-17

New handlers for CSV and plain-text carriers, streaming for FITS and WAV, a computed validation
score, dependency security upgrades, and rewritten documentation. The repository is now public
under Apache-2.0. The payload wire format is unchanged from 0.6.0/0.7.0.

**Breaking:** `ValidationReport::overall_score` / `ValidationSummary::overall_score` are now
`Option<f64>` and `overall_score()` returns `Option<u8>` (#96). CSV and `.txt` carriers are now
embedded by dedicated handlers; files produced by the old (corrupting) byte-LSB path are not
readable by them.

### Added
- CSV `csv_min_significant_digits` ([#103](https://github.com/scttfrdmn/stegano/issues/103)):
  a numeric cell is used only if its mantissa has at least this many significant digits
  (leading zeros do not count, trailing zeros do: `0.70` has 2, `0.007` has 1; an all-zero
  mantissa counts as 1). The default is **2**, so single-digit cells such as `1e-5`, `3`, `0.7`
  and `0` are never modified and no value changes by more than 10% of itself. Set 1 for the
  previous behaviour (every numeric cell). The threshold is stored in the `CSVJ` envelope's
  strategy byte (files written before this change have 1, which is threshold 1), and extraction
  detects it, so no parameter is needed to extract and older files still extract. Both the
  in-memory and streaming paths honour it and stay byte-identical. Capacity drops by the number
  of single-digit cells; `estimate_capacity` reports the default threshold and lists the cell
  count for every threshold in `eligible_cells_by_min_significant_digits`.
- FITS and WAV streaming ([#80](https://github.com/scttfrdmn/stegano/issues/80)).
  `stegano::stream`, the CLI, `stegano_embed_file` and the bindings now process large
  astronomy and audio files with bounded memory. The output is byte-identical to the
  in-memory handlers' output for the same encoded payload, and each path reads the other's files.
  - **FITS** (`stream::fits`, both `fits_strategy` values, `pixel_lsb` and `float_mantissa`):
    headers are read one 2880-byte block at a time, parsed and validated with the same code as
    the in-memory handler, and copied through. The target HDU's data is processed in 180 KiB
    chunks (`BLANK`, NaN and Inf samples are skipped as before). Other HDUs, padding and trailing
    bytes are copied unchanged.
  - **WAV** (`stream::wav`, `signal_strategy = "timedomain"` or `--stealth low` only): non-`data`
    chunks (`LIST` before or after the data, etc.) are copied through. The RIFF and chunk size
    fields stay valid because the file length does not change. Samples are processed in 256 KiB
    frame-aligned buffers, with the same PCM 8/16/24/32, extensible and float rules as in memory.
    `frequency` and `phase` are not streamable: they use channel-major bit order across 1024-frame
    FFT chunks, which a single pass cannot reproduce for multichannel audio. `spread` and `echo`
    have the same order and also self-verify against the whole quantised output. Stream
    extraction of a non-`timedomain` WAV payload returns the "not streamable" error, so the CLI
    falls back to in-memory extraction.
  - Measured with the release CLI on a 1.07 GB three-HDU FITS (empty primary, int16 IMAGE,
    BINTABLE) and a 1.5 GB 16-bit stereo WAV with `LIST` chunks before and after `data`, with a
    password: about 71 MB peak RSS for embed and extract (mostly Argon2id). Extraction read
    256 KiB (FITS) and 512 KiB (WAV). The outputs pass astropy `verify('exception')` and open
    in `wave`/soundfile with unchanged parameters.

### Fixed
- `ValidationReport::overall_score` is now computed, so `stegano embed --validate` no longer
  always prints "Overall Score: 0/10" ([#96](https://github.com/scttfrdmn/stegano/issues/96)).
  `finalize()` sets it from the pairs-of-values and Sample Pairs Analysis results with
  `validation::detection_resistance_score`: `10 × (1 − mean suspicion)`, where PoV contributes
  1 if it flags the carrier and SPA contributes its estimated embedding rate. It is a heuristic
  covering only those two LSB-replacement detectors, not a security guarantee. Breaking:
  the field (and `ValidationSummary::overall_score`) is now `Option<f64>`, and
  `overall_score()` returns `Option<u8>`. They are `None` when neither detector ran.
  `comprehensive_validation` now calls `finalize()`, and the embedding path also stores the
  PoV, SPA and audio results in `steganalysis_results`. The CLI line is now
  `Detection-resistance score: N/10 (heuristic)`, or `n/a`.
- The CLI's `--strategy` flag now applies to FITS files, mapping to `fits_strategy`
  (`pixel_lsb` or `float_mantissa`), so float (`BITPIX` -32/-64) FITS data can be embedded
  from the CLI with `--strategy float_mantissa`. Unknown values are rejected with an error
  ([#95](https://github.com/scttfrdmn/stegano/issues/95)).
- CSV (`.csv`, `DataFormat::CsvTimeseries`) and plain-text (`.txt`, `DataFormat::TEXT`) carriers
  are no longer corrupted on embed ([#97](https://github.com/scttfrdmn/stegano/issues/97)).
  CSV used to go to the signal handler as raw 16-bit samples, and text to the generic byte-LSB
  handler, so visible characters changed (`,` became `-`). Both now have format-aware handlers:
  - **CSV** (`handlers::csv`, `csv_strategy` = `digit`): RFC 4180 parsing (quoted fields with
    embedded delimiters, quotes and line breaks; `csv_delimiter` overrides `,`). Each unquoted
    numeric cell (`12`, `-3.140`, `1e-5`) carries one bit in the parity of its last mantissa
    digit, so **numeric values change by at most one unit in their last printed place**. The
    header row, non-numeric and quoted cells, delimiters, line endings and the final newline stay
    byte-identical. Capacity is one bit per eligible cell. Streamable.
  - **Text** (`handlers::text`, `text_strategy` = `whitespace`): `k` trailing spaces/tabs per
    line (space = 0, tab = 1). Existing trailing whitespace on the lines used is normalised
    first, so the extractor cannot desynchronise. `k` is chosen automatically (1..=64) or set
    with `text_bits_per_line`. Every line is unchanged after stripping trailing whitespace, line
    endings are kept and ASCII stays ASCII, but the payload is visible in editors that show
    whitespace. Binary or non-UTF-8 input is rejected. Streaming embed needs a fixed
    `text_bits_per_line`; streaming extract works for any `k`.
  - The CLI `--strategy` now maps to `csv_strategy` / `text_strategy`. `BinarySignal` still uses
    the signal handler, and `BINARY` still uses the generic handler. The signal handler now
    rejects formats other than WAV and `BinarySignal`.
  - `tests/format_validity` covers CSV (Python `csv` module) and text (lines equal after
    `rstrip`).

### Security
- Upgraded dependencies to clear Dependabot alerts
  ([#98](https://github.com/scttfrdmn/stegano/issues/98)):
  - pyo3 0.22.6 → 0.29.3 (optional, `--features python`): GHSA-36hh-v3qg-5jq4,
    GHSA-chgr-c6px-7xpp, GHSA-pph8-gcv7-4qj5. The Python API is unchanged; the bindings were
    ported to the 0.29 API (`PyBytes::new` / `PyDict::new`, `Python::detach`).
  - bytes 1.10.1 → 1.12.1: GHSA-434x-w66g-qw3r (integer overflow in `BytesMut::reserve`).
  - rand 0.8.5 → 0.8.8 and 0.9.2 → 0.9.5 (the latter via the proptest dev-dependency):
    GHSA-cq8v-f236-94qc.
- `pyproject.toml` no longer sets `PYO3_USE_ABI3_FORWARD_COMPATIBILITY`, because pyo3 0.29
  supports CPython up to 3.15.

### Changed
- npm publishing uses npm trusted publishing (OIDC); `NPM_TOKEN` is only a fallback
  ([#89](https://github.com/scttfrdmn/stegano/pull/89)). The workflow publishes the packed
  `./dist` tarball and can re-publish an existing release tag via the `tag` input
  ([#90](https://github.com/scttfrdmn/stegano/pull/90)).

### Documentation
- README: states that payloads are encrypted only when a password is given (plaintext
  otherwise, in every interface); adds a limitations summary, the CLI password sources and their
  precedence, `detect` exit code 3, a per-format strategy/streaming table, the C function list,
  the Python file API and `Stegano` class, and the `EmbedParams::new` (EC Medium) vs CLI/bindings
  (EC none) default.
- SECURITY.md rewritten: supported versions 0.7.x/0.6.x, the actual cryptography (Argon2id
  64 MiB/t=3/p=1, AES-256-GCM or ChaCha20-Poly1305 with an authenticated header), a
  "Threat model and limitations" section, and only the development practices that are actually
  in place. Reports go through GitHub private vulnerability reporting.
- CONTRIBUTING.md: correct clone URL, the real CI jobs (fmt is non-blocking), the `experimental`
  feature, building each binding, the format-validity suite and the release steps.
- Rustdoc and CLI help: `CipherKind` key derivation is Argon2id (not Blake3); VCF/SAM/BAM/DICOM
  are no longer marked "planned"; `--gpu` / `use_gpu` are described as having no effect on
  embedding; the FFI `panic = "unwind"` note, error codes, `CDataFormat` values and
  `CCapacityInfo`/`CEmbedResult` fields are documented (C headers regenerated).

## [0.7.0] - 2026-10-17

Streaming for large genomics files, a Go module other projects can use, and the `stegano-wasm`
npm package. The wire format is unchanged from 0.6.0.

Measured peak memory: about 70 MB to embed or extract on a 1–2 GB FASTQ, VCF or BAM, almost all of
it Argon2id's 64 MiB. A 1 GB FASTQ through the CLI took 71 MB streaming vs 5.19 GB with
`--no-stream`. Streaming extraction reads only the first 256–512 KiB.

### Added
- npm package `stegano-wasm` ([#34](https://github.com/scttfrdmn/stegano/issues/34)). It contains
  the web (ESM, `await init()`) and Node.js (CommonJS and ESM) builds, with TypeScript types and
  conditional `exports`. `scripts/build-wasm.sh` assembles it in `pkg/`, smoke tests run from the
  packed tarball in CI (`npm/test/`), and pushing a `v*` tag publishes it (`.github/workflows/npm-publish.yml`, needs the `NPM_TOKEN`
  secret). npm provenance is used only when the repository is public.
- BAM: `format_specific["bam_compression"]` = `"bgzf"` (default, unchanged) or `"none"` to write an
  uncompressed `BAM\1` stream ([#74](https://github.com/scttfrdmn/stegano/issues/74)). Extraction
  auto-detects either form; unknown values are rejected with `InvalidParameter`.
- Go module can be used from other repositories with
  `go get github.com/scttfrdmn/stegano/go@<version>` ([#73](https://github.com/scttfrdmn/stegano/issues/73)).
  cbindgen now also writes `go/stegano.h` inside the module, and the cgo directive is
  `-I${SRCDIR}` instead of `-I${SRCDIR}/../include`. The CI header check covers both copies.
  Releases also push a `go/vX.Y.Z` tag. See "Using from another module" in `go/README.md`.
- Streaming embed/extract for large carriers ([#75](https://github.com/scttfrdmn/stegano/issues/75),
  part of [#30](https://github.com/scttfrdmn/stegano/issues/30)): new `stegano::stream` module with
  `embed_stream(format, reader, writer, payload, &params)`, `extract_stream(format, reader, &params)`,
  `is_streamable`, and the `embed_file` / `extract_file` helpers. Only the carrier is streamed:
  the payload is encoded once (wire format v2, unchanged) and memory use stays bounded by one
  carrier record, whatever the file size. Extraction stops reading once the payload frame is
  complete. FASTQ quality-score strategies are supported for now. The output is byte-identical
  to the in-memory `GenomicsHandler`'s, and the two APIs can read each other's output. Other
  formats and strategies return `InvalidParameter`. `embed_stream` can only detect insufficient
  capacity at EOF, after the whole output has been written. That output is incomplete and must
  be discarded. `embed_file` handles this by writing to a temporary file and renaming it only
  on success.
- VCF streaming ([#77](https://github.com/scttfrdmn/stegano/issues/77), part of
  [#30](https://github.com/scttfrdmn/stegano/issues/30)): `stegano::stream` now handles
  `DataFormat::VCF` with `vcf_strategy` = `info` (default), `id` and `quality`, one line at a
  time. The `##INFO=<ID=STEG,...>` declaration is still written before `#CHROM`. LF/CRLF and all
  untouched lines are kept byte for byte, and the output is byte-identical to the in-memory
  `VcfHandler`'s. Streaming extraction auto-detects the channel (it also reads files written with
  the in-memory `format`/`multi` strategies) and stops once the payload frame is complete.
  `format` (alias `sample`) and `multi` need the whole file and return `InvalidParameter` for
  streaming embed. Embedding into a 1.09 GB VCF uses a few MB on top of Argon2id's 64 MiB (measured with test KDF
  parameters at about 7 MB); `bcftools view` accepts the output.
- SAM/BAM streaming ([#76](https://github.com/scttfrdmn/stegano/issues/76), part of
  [#30](https://github.com/scttfrdmn/stegano/issues/30)): `stegano::stream` now handles
  `DataFormat::SAM` (line by line; header lines pass through) and `DataFormat::BAM` (BGZF block
  by block, records parsed across block boundaries, or raw `BAM\1` input) with `sam_strategy` =
  `auxiliary` or `quality`. BAM output is re-blocked like the in-memory compressor and ends with
  the BGZF EOF block, or is written raw with `bam_compression` = `none`. For the same encoded
  payload the output is byte-identical to `SamBamHandler`'s, and each API reads the other's
  output. Streaming extraction also reads files written with the in-memory `optional`,
  `comment` and `multi` strategies, and stops once the envelope is complete. `optional`,
  `comment` and `multi` embedding need the whole file and return `InvalidParameter` for
  streaming. The `sam_bam` internals the stream reuses are now crate-visible helpers; the
  in-memory behaviour is unchanged. Embedding 16 KiB into a 2.16 GB BAM peaked at about
  69 MiB RSS (mostly Argon2); extraction read only the first 256 KiB. `samtools quickcheck`
  and `samtools view -c` accept the output.
- Streaming in the CLI, C FFI, Go and Python ([#78](https://github.com/scttfrdmn/stegano/issues/78),
  part of [#30](https://github.com/scttfrdmn/stegano/issues/30)). Each call checks
  `stream::is_streamable`, so any format registered with `stegano::stream` is picked up
  automatically. Extraction streams any format that has a stream handler, whatever strategy the
  payload was embedded with.
  - CLI: `embed` and `extract` stream when the format and strategy allow it. The format comes from
    the extension, or from the magic bytes of the first 4 KiB, so the file is never read whole.
    `--no-stream` forces the in-memory path, and `--validate` also uses it. The command prints
    which path it used. Output files are now always written atomically.
  - C FFI: new `stegano_embed_file(in_path, out_path, payload, payload_len, format, password,
    format_specific_json, result_out)`, `stegano_extract_file(path, format, password,
    format_specific_json, output_out, output_len_out)` and
    `stegano_detect_file_format(path, format_out)`. They take paths, use `size_t` lengths and
    have no 32-bit size limit. They stream when possible and otherwise read the file into memory.
    Embedding writes to a temporary file and renames it over the output only on success.
  - Go: `EmbedFile(inPath, outPath, payload, opts...)`, `ExtractFile(path, opts...)`, and
    `WithFormatSpecific(key, value)`. `WithFormatSpecific` merges any handler key, such as
    `bam_compression`, into the options JSON for every call.
  - Python: `embed_file(in_path, out_path, payload, password=None, ..., format_specific=None)`
    returns `{"bytes_embedded", "streamed"}`. `extract_file(path, password=None, ...,
    format_specific=None)` is the extract counterpart. Both release the GIL. `embed`/`extract`
    (module and `Stegano` class) also accept `format_specific: dict | None`.

### Fixed
- WebAssembly: every embed panicked (`unreachable`) because `std::time::Instant::now()` is not
  implemented on `wasm32-unknown-unknown`. The handlers' timing statistics now use an internal
  clock that reads `Date.now()` on wasm32.

## [0.6.0] - 2026-10-16

**This release breaks compatibility.** It changes the payload wire format and fixes every format
handler so its output is a valid file of the carrier's format. Data embedded with 0.5.x
**cannot** be extracted with 0.6.x. Re-embed from your original payloads.

### Security
- **Payload wire format v2** ([#38](https://github.com/scttfrdmn/stegano/issues/38)):
  - Argon2id key derivation (m=64 MiB, t=3, p=1) with a random 16-byte salt per payload. This
    replaces the single BLAKE3 hash with a fixed salt, and the 32-byte raw-key shortcut is gone.
  - The whole 36-byte header (version, flags, cipher, EC length, salt, nonce, length) is
    AEAD associated data.
  - Removed the unkeyed BLAKE3 field, which let anyone detect a payload without the key.
  - A password supplied for an unencrypted payload, or missing for an encrypted one, is an error
    (no silent downgrade).
  - The header is protected by Reed-Solomon, and lengths are checked before any allocation.
  - Keys are zeroized, and `EmbedParams` Debug output redacts the key.
  - See `docs/WIRE_FORMAT.md`.
- FFI panics are caught and returned as an error code. The release profile now uses
  `panic = "unwind"` ([#62](https://github.com/scttfrdmn/stegano/issues/62)).
- CLI passwords can come from `--password-file`, `STEGANO_PASSWORD` or a tty prompt.
  `--password` still works but prints a warning that it is visible to other users
  ([#39](https://github.com/scttfrdmn/stegano/issues/39)).

### Fixed: format handlers now write valid files
- **PNG/TIFF** ([#42](https://github.com/scttfrdmn/stegano/issues/42)):
  - Images are decoded to pixels and only sample LSBs change. Before, bits were written over
    the PNG signature, chunk CRCs and zlib stream.
  - Fixed the AdaptiveLSB/EdgeBased encoder/decoder desync and the DCT clamping failure.
  - Capacity estimates are exact.
- **WAV** ([#43](https://github.com/scttfrdmn/stegano/issues/43)):
  - New RIFF chunk parser covering PCM 8/16/24/32 and float, any channel count. Headers and
    other chunks stay byte-identical.
  - No more panics on low sample rates.
  - Spread spectrum and echo use a keyed PN sequence and check at embed time that the payload
    can be extracted.
- **DICOM** ([#44](https://github.com/scttfrdmn/stegano/issues/44)):
  - Explicit-VR length layout follows PS3.5.
  - Supports implicit VR LE and explicit VR BE.
  - An unmodified file is written back byte-identical, and elements are never silently
    dropped.
  - Pixel LSB changes each sample by at most 1.
  - Private block is ordered, with a private creator element.
- **FITS** ([#45](https://github.com/scttfrdmn/stegano/issues/45)):
  - Parses HDUs (BITPIX/NAXIS) and embeds only in the sample LSBs of the first integer image
    HDU. Other headers and HDUs are untouched.
  - Float data needs an explicit opt-in.
- **SAM/BAM** ([#46](https://github.com/scttfrdmn/stegano/issues/46)):
  - BGZF read/write.
  - Valid 2-character `zs:Z:` tags.
  - `*` qualities left untouched.
  - Strict, bounds-checked parsing that never drops records.
- **VCF** ([#47](https://github.com/scttfrdmn/stegano/issues/47)):
  - Unchanged lines pass through untouched, so QUAL values are no longer rounded.
  - New INFO/FORMAT keys are declared in the header.
  - Fixed a UTF-8 slicing panic, and payloads of more than 100 chunks no longer truncate.
- **FASTQ/FASTA** ([#48](https://github.com/scttfrdmn/stegano/issues/48)):
  - Fixed the quality-skip desync on low qualities (`#`, `!`).
  - FASTA capacity is enforced.
  - CRLF and trailing records are preserved.
- **Self-describing extraction:** each handler stores a strategy marker and decodes once.
- **Generic `verify_integrity`:** no longer reports a payload in all-zero carriers
  ([#60](https://github.com/scttfrdmn/stegano/issues/60)).
- **`InsufficientCapacity`:** always reported in bits
  ([#61](https://github.com/scttfrdmn/stegano/issues/61)).

### Fixed: bindings and CLI ([#39](https://github.com/scttfrdmn/stegano/issues/39))
- **FFI output:** `stegano_embed_v2` returns a newly allocated output buffer and its length, so
  carriers that grow (SAM, VCF, DICOM) are no longer truncated. v1 `stegano_embed` returns
  `OutputSizeChanged` instead of truncating.
- **FFI inputs:** the format is a validated `uint32_t` (invalid values used to be undefined
  behaviour), and the `cipher` option is honoured.
- **C header:** generated with cbindgen into `include/stegano.h`, and CI checks it is up to
  date.
- **Go:** `WithCipher` now works, and `WithStrategy` sends the right key for each format.
- **Shared carrier builder:** new `src/carrier.rs` sets sample rate and dimensions for the CLI,
  FFI, Python and WASM alike, and the defaults are the same everywhere.
- **CLI:**
  - The DICOM strategy key is correct.
  - `extract --ec-level`.
  - Consistent exit codes.

### Fixed: validation ([#41](https://github.com/scttfrdmn/stegano/issues/41))
- **New detectors:** the Westfeld–Pfitzmann pairs-of-values attack and Sample Pairs Analysis.
- **Corrected statistics:**
  - The pixel-pair statistic is no longer constant.
  - The chi-square critical value is one-tailed.
  - ROC/AUC and cross-validation use a real detector score.

### Changed
- **BREAKING:** `gpu`, `ml` and `monitoring` are behind the off-by-default `experimental` feature.
  `wgpu` is no longer a default dependency (181 → 131 crates). The wasm32 build works
  ([#40](https://github.com/scttfrdmn/stegano/issues/40)).
- **BREAKING:** the fake XOR-parity `IntegritySystem` was removed.
- **BREAKING:** `with_encryption` constructors on handlers were removed. All encryption goes
  through the codec.
- **CI** (GitHub Actions; [#35](https://github.com/scttfrdmn/stegano/issues/35),
  [#36](https://github.com/scttfrdmn/stegano/issues/36)): tests, clippy `-D warnings`, a
  wasm32 check, Go tests, the C header check, and an external format-validity suite.

### Also included (developed after 0.5.1)
### Added
- **Extended FFI v2** (`src/ffi.rs`): `stegano_embed_v2` and `stegano_extract_v2` accept a
  `format_specific_json` parameter (null-terminated UTF-8 JSON object) that is parsed into
  `HashMap<String, serde_json::Value>` and forwarded as `EmbedParams::format_specific`.  This
  allows CGO/C callers to select SAM `auxiliary`, DICOM `metadata`/`sequence`, WAV `echo`, and
  other format-specific strategies without requiring a Rust rebuild.  Resolves
  [#31](https://github.com/scttfrdmn/stegano/issues/31).
- **Go bindings: `WithStrategy` and `WithCipher` options** (`go/stegano.go`):
  `WithStrategy(string)` passes the chosen strategy as the correct JSON key for the carrier
  format (`sam_strategy`, `dicom_strategy`, `signal_strategy`).  `WithCipher("chacha20")`
  selects ChaCha20-Poly1305 encryption.  Both options automatically dispatch to the v2 FFI;
  unchanged call sites continue to use the v1 path.  2 new tests — 19 Go tests total.
- **CLI help improvements** (`src/cli/mod.rs`): `--strategy` help text now lists `auxiliary`
  for SAM/BAM, `echo` for WAV, and the full DICOM strategy list
  (`private|metadata|sequence|pixel_lsb`).

### Changed
- **README headline** updated to reflect v0.5.1 (was incorrectly showing v0.5.0).
- **Relicensed to Apache-2.0.** `LICENSE` now contains the Apache License 2.0 text, a `NOTICE`
  file was added, `Cargo.toml`/`pyproject.toml` declare `Apache-2.0`, and the contradictory
  "proprietary and confidential" source headers were replaced with
  `SPDX-License-Identifier: Apache-2.0`. Resolves [#37](https://github.com/scttfrdmn/stegano/issues/37).
- **Docs reorganized.** README rewritten to describe actual capabilities and current status
  (experimental GPU/ML modules, upcoming v0.6.0 format and wire-format changes). Historical
  planning, roadmap, business and session documents moved from the repo root to
  `docs/archive/`; `CONTRIBUTING.md` moved to the repo root.
- `cobertura.xml` is no longer tracked; `scripts/build/build.sh` removed in favour of
  `scripts/build-wasm.sh`.

## [0.5.1] - 2026-02-23

### Added
- **WebAssembly bindings** (`src/wasm.rs`, feature `wasm`):
  `stegano_embed`, `stegano_extract`, `stegano_estimate_capacity`, and `stegano_version`
  exported via wasm-bindgen.  `WasmCapacityInfo` struct carries capacity metadata.  All
  internal helpers are tested natively; WASM exports are `async fn` that resolve as JS
  Promises.  Build with `wasm-pack build --target web --no-default-features --features wasm`.
  12 new tests — 428 total.  Resolves
  [#25](https://github.com/scttfrdmn/stegano/issues/25).

## [0.5.0] - 2026-02-23

### Added
- **SAM/BAM `AuxiliaryData` strategy** (`src/handlers/sam_bam.rs`):
  Embeds payload as hex-encoded `Xs:Z:<chunk>` optional fields across SAM alignment records,
  using the SAM-spec user-private tag range. Capacity: `alignment_count × 255` bytes.
  `embed_auxiliary_data` / `extract_auxiliary_data` helper methods.  Resolves
  [#22](https://github.com/scttfrdmn/stegano/issues/22).
- **DICOM `UnusedMetadata` strategy** (`src/handlers/dicom.rs`):
  Embeds up to 120 bytes in the DICOM preamble (128 bytes mandated unused by the spec).
  Layout: `STEG` magic (4 bytes) + LE length (4 bytes) + payload.  Resolves
  [#20](https://github.com/scttfrdmn/stegano/issues/20).
- **DICOM `SequencePadding` strategy** (`src/handlers/dicom.rs`):
  Embeds payload in a `(FFFC,FFFC)` Data Set Trailing Padding element — a standard DICOM
  tag that compliant readers are required to ignore.  Large capacity (file-size limited).
  Resolves [#21](https://github.com/scttfrdmn/stegano/issues/21).
- **EchoHiding accessible via `format_specific`** (`src/handlers/signals.rs`):
  `signal_strategy = "echo"` in `EmbedParams::format_specific` selects `EchoHiding`
  without relying on a `StealthLevel` mapping.  Resolves
  [#23](https://github.com/scttfrdmn/stegano/issues/23).
- **`run_performance_analysis`** (`src/validation.rs`):
  Builds a labeled cover/stego byte dataset from `original` vs `stego`, computes ROC
  analysis (AUC, Youden's index) and 5-fold cross-validation metrics.  Resolves
  [#24](https://github.com/scttfrdmn/stegano/issues/24).
- **`run_domain_validation`** (`src/validation.rs`):
  Format-aware validation: genomics (printable-ASCII ratio for SAM/FASTQ/VCF; BAM magic),
  imagery/medical (DICOM `DICM` prefix, TIFF/PNG magic), audio (normalised RMS quality
  score for WAV/BinarySignal).  Resolves [#24](https://github.com/scttfrdmn/stegano/issues/24).
- **Criterion benchmark suite** (`benches/`):
  Enabled `[[bench]]` entries for `embedding_performance`, `statistical_validation_benchmarks`,
  and `ml_optimization_benchmarks`.  Compile with `--features benchmarks`.  Resolves
  [#27](https://github.com/scttfrdmn/stegano/issues/27).

### Changed
- `DicomEmbedStrategy::UnusedMetadata` and `SequencePadding` are now fully implemented
  (previously returned `InvalidParameter` errors).
- `SamBamEmbedStrategy::AuxiliaryData` is now fully implemented for SAM format.
- Capacity estimate for `SamBamEmbedStrategy::AuxiliaryData` on SAM: returns
  `alignment_count × MAX_OPTIONAL_FIELD_SIZE` (was 0).

### Fixed
- Clippy: removed `* 1` no-op multiplier in `fits.rs`, replaced manual `div_ceil`,
  replaced loop-index patterns with iterator `enumerate()`, removed redundant `usize`
  casts, replaced `map().flatten()` with `and_then()`, replaced `unwrap_or_else(||…)` with
  `unwrap_or(…)` where the value is `Copy`, added `Range::contains` for range checks.
- 416 tests passing (up from 346 in v0.3.13, 411 in v0.4.0).

## [0.4.0] - 2026-02-23

### Added
- **ChaCha20-Poly1305 encryption** (`src/crypto/mod.rs`, `src/core.rs`, `src/cli/`):
  New `CipherKind` enum (`Aes256Gcm` = 0, `ChaCha20Poly1305` = 1) stored as a prefix byte
  in the serialised payload so extraction auto-detects the cipher.  Added `--cipher chacha20`
  flag on the `embed` subcommand; `EmbedParams::with_chacha20()` builder.  Serialisation
  format updated from 48 → 49 bytes minimum.  Resolves
  [#17](https://github.com/scttfrdmn/stegano/issues/17).
- **DCT frequency-domain embedding** (`src/handlers/imagery.rs`):
  Implemented 8-point DCT-II / QIM steganography (`dct8`, `idct8`, `embed_dct`,
  `extract_dct_bits`).  `StealthLevel::Maximum` now dispatches to `DCTFrequency` instead of
  the unimplemented `EdgeBased` strategy.  Added `imagery_strategy = "dct"` key in
  `format_specific` to override per-embed.  Removed the `PVD` variant (slot intentionally
  unused, byte value 3 reserved).  Resolves
  [#19](https://github.com/scttfrdmn/stegano/issues/19).
- **FITS astronomical format handler** (`src/handlers/fits.rs`):
  Pixel-LSB steganography in the FITS primary HDU data section.  Handler locates the data
  section by scanning 80-byte header cards for the `END` card and aligning to the next
  2880-byte block boundary.  Magic: `FTSJ` + 4-byte LE payload length + strategy byte +
  codec mode byte.  Supports `.fits`, `.fit`, `.fts` extensions; magic detection via
  `SIMPLE  ` prefix.  9 unit tests.  Resolves
  [#18](https://github.com/scttfrdmn/stegano/issues/18).
- **Python bindings via PyO3** (`src/python.rs`, `pyproject.toml`):
  Optional `python` feature (`--features python`) exposes `stegano.Stegano` Python class
  and module-level `embed()`, `extract()`, `estimate_capacity()`, `version()` functions.
  Format auto-detected from carrier bytes; sync wrappers around the Tokio async API.
  Build with `maturin develop --features python`; requires
  `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1` on Python ≥ 3.14.  10 new unit tests.  Resolves
  [#16](https://github.com/scttfrdmn/stegano/issues/16).

### Changed
- `EmbedParams` gains a `cipher_kind: CipherKind` field (default `Aes256Gcm`, serde
  default) — all existing struct literals updated with `cipher_kind: Default::default()`.
- `ImageEmbedStrategy` is now `#[non_exhaustive]`; `Maximum` stealth maps to `DCTFrequency`.
- `DataFormat::FITS` added; `CDataFormat::Fits = 14` in FFI; `get_handler` updated.
- `pyproject.toml` added at repository root for maturin-based Python wheel builds.

## [0.3.21] - 2026-02-23

### Added
- **`--ec-level` CLI flag** (`src/cli/mod.rs`, `src/cli/commands.rs`, `src/core.rs`):
  New `--ec-level <none|low|medium|high|maximum>` flag on the `embed` subcommand controls
  Reed-Solomon error-correction strength. Default is `none` (no RS overhead). Warns when
  `ECLevel::Low` is used with a payload smaller than ~200 bytes (RS block is always 255
  bytes). Implemented via new `EmbedParams::with_ec_level()` builder method. Resolves
  [#13](https://github.com/scttfrdmn/stegano/issues/13).
- **`--strategy` CLI flag** (`src/cli/mod.rs`, `src/cli/commands.rs`):
  New `--strategy <name>` flag on both `embed` and `extract` subcommands selects the
  format-specific embedding strategy. Valid values per format:
  - VCF: `info`, `quality`, `sample`, `id`, `multi`
  - SAM/BAM: `optional`, `comment`, `quality`, `multi`
  - PNG/TIFF/MICROSCOPY/DICOM: `lsb`, `adaptive`, `edge`
  - WAV: `timedomain`, `frequency`, `phase`, `spread`
  - FASTA/FASTQ: `codon`, `quality`, `composition`
  Resolves [#14](https://github.com/scttfrdmn/stegano/issues/14).
- **`detect` subcommand** (`src/cli/mod.rs`, `src/cli/commands.rs`, `src/bin/stegano.rs`):
  New `stegano detect -i <file> [--validate]` subcommand checks for embedded data.
  Exits 0 if hidden data is detected, 1 if not — designed for shell scripting.
  `--validate` runs the full statistical validation suite on the carrier first.
  Resolves [#15](https://github.com/scttfrdmn/stegano/issues/15).
- **13 new unit tests** for `parse_ec_level` (all variants + case-insensitivity + defaults)
  and `apply_strategy` (all mapped formats + overwrite behaviour + unmapped noop).

### Fixed
- `#[allow(dead_code)]` on `SignalEmbedStrategy::from_u8` and `ImageEmbedStrategy::from_u8`
  (both added in v0.3.19/v0.3.20) — `pub(crate)` alone does not suppress the warning for
  associated functions; explicit allow attribute is required.

## [0.3.20] - 2026-02-23

### Fixed
- **Signals extraction: stored strategy byte not validated** (`src/handlers/signals.rs`):
  `embed()` stored the embedding strategy as byte 8 of the `"SGSJ"` header
  (`strategy.to_u8()`), but `extract()` ignored it entirely and dispatched purely from
  `params.stealth_level`. This meant that if the caller used a different `stealth_level`
  on extraction than on embedding — e.g., embed with `StealthLevel::High` (PhaseModulation)
  and extract with `StealthLevel::Medium` (FrequencyDomain) — the wrong strategy was used
  and extraction returned corrupted data or an error with no helpful message.

  Fix (mirrors v0.3.19 imagery fix):
  - Added `SignalEmbedStrategy::from_u8(v: u8) -> Option<Self>`.
  - `extract_with_strategy` now validates `header_bytes[8] == strategy.to_u8()` after the
    magic check; mismatches return an error.
  - `extract()` now tries all five strategies in order (`TimeDomainLSB`, `FrequencyDomain`,
    `PhaseModulation`, `SpreadSpectrum`, `EchoHiding`) instead of dispatching from
    `stealth_level`, making signal extraction fully self-describing.

- **FASTQ partial embedding surfaces an error instead of silently returning** (`src/handlers/genomics.rs`):
  `embed_fastq_qualities()` may embed fewer bits than requested when quality chars near
  ASCII 33 ('!') are "skipped" (clearing their LSBs would push the value below the valid
  quality range [33–126]). In that case the function returns `Ok` with `result.success ==
  false`. The caller in `embed()` never checked this flag, always proceeding as if the
  full payload was embedded. On extraction, the truncated payload produces a corrupted
  length header and garbled or empty output.

  Fix: check `result.success` after `embed_fastq_qualities()`; return
  `Err(InsufficientCapacity)` if `false`. The capacity estimation (`estimate_fastq_quality_capacity`)
  still counts all quality chars (slightly optimistic), but any near-capacity case where
  unembeddable chars reduce actual capacity is now surfaced as an error rather than silent
  data loss.

### Tests
- `test_vcf_multistrategy_roundtrip`: first roundtrip test for `vcf_strategy = "multi"`.
  Verifies embed→extract with MultiStrategy (info→sample_format→variant_id fallback chain)
  correctly recovers the payload.
- `test_sam_multistrategy_roundtrip`: first roundtrip test for `sam_strategy = "multi"`.
  Verifies embed→extract with MultiStrategy (optional_fields→comment_lines fallback chain)
  correctly recovers the payload.
- **Test count: 378 → 380**

## [0.3.19] - 2026-02-23

### Fixed
- **Imagery extraction: strategy tag never validated** (`src/handlers/imagery.rs`):
  `embed()` serializes the embedding strategy as a single byte at header offset 8
  (`strategy.to_u8()`), but `extract_with_strategy()` never read it back — there was
  no `from_u8()` method and the byte was silently ignored. The try-all-strategies loop
  in `extract()` relied purely on the 4-byte `"IMSJ"` magic to discriminate between
  strategies, leaving a theoretical (probability ~1/2^32) window where the wrong
  strategy could decode a false-positive magic match and return corrupt payload data
  with no error.

  Fix: added `ImageEmbedStrategy::from_u8(v: u8) -> Option<Self>` and inserted a
  strategy-tag validation step in `extract_with_strategy` immediately after the magic
  check. If `header_bytes[8] != strategy.to_u8()` the probe returns an error, ensuring
  the try-all loop advances to the correct strategy.

### Removed
- **`src/main.rs`** dead-code placeholder (`println!("Hello, world!")`). The real CLI
  entry point is `src/bin/stegano.rs`; the placeholder was never compiled because the
  explicit `[[bin]]` declaration in `Cargo.toml` takes precedence over Cargo's default
  `src/main.rs` auto-discovery.

## [0.3.18] - 2026-02-23

### Fixed
- **CLI image/audio carrier construction** (`src/cli/commands.rs`):
  All three command handlers (`embed`, `extract`, `analyze`) previously constructed
  `CarrierData` with `dimensions: None` and `sampling_rate: None`. Imagery handlers
  (`PNG`, `TIFF`) require `dimensions` to be set and will return
  `SteganoError::InvalidParameter("Missing image dimensions")` when it is absent;
  audio handlers (`WAV`) similarly require `sampling_rate`. The CLI was therefore
  entirely non-functional for image and audio file types despite the library itself
  working correctly.

  Fix: introduced a `build_carrier(path, raw_data) -> CarrierData` shared helper
  that detects the file format (extension-first, magic-byte fallback) and then
  parses the relevant file header to populate `dimensions` (PNG IHDR, TIFF IFD)
  and `sampling_rate` (WAV fmt chunk). All three handler functions now call
  `build_carrier` instead of constructing `CarrierData` directly. Format detection
  also gained support for `.dcm` / `.dicom` → `DataFormat::DICOM`.

### Tests
- 27 new unit tests in `src/cli/commands.rs` (`cli::commands::tests` module):
  - `test_format_from_extension_*` — extension detection for PNG, TIFF (.tiff/.tif),
    DICOM (.dcm/.dicom), WAV, and all genomics formats (FASTA/FA, FASTQ/FQ, VCF, SAM, BAM)
  - `test_magic_*` — magic-byte detection for PNG, TIFF LE, TIFF BE, WAV, FASTA,
    FASTQ, SAM header, VCF header, and unknown data
  - `test_png_dimensions_*` — PNG IHDR parsing (basic, too-short, wrong magic, zero rejected)
  - `test_tiff_dimensions_*` — TIFF IFD parsing (little-endian, too-short, wrong magic)
  - `test_wav_sample_rate_*` — WAV fmt-chunk parsing (44100, 48000, non-WAV format ignored,
    too-short data)
  - `test_parse_stealth_level` — all stealth levels including case-insensitivity and default

## [0.3.17] - 2026-02-22

### Fixed
- **Imagery capacity estimation overflow** (`src/handlers/imagery.rs`):
  `estimate_image_capacity()` computed `total_pixels * 3` bits for SimpleLSB (and similar
  dimension-based formulas for other strategies). For a grayscale image (`raw_data.len() ==
  total_pixels`), this overestimates by 3×. The inflated estimate passes the capacity guard in
  `embed_lsb()` without error, but `embed_lsb_pixels()` silently breaks early when it exhausts
  `image_data` — returning `Ok` while writing only a fraction of the payload. Extraction then
  returns garbled or truncated data with no error surfaced.

  Fix: all three non-unimplemented strategies now cap their computed `capacity_bits` by
  `carrier.raw_data.len()`, since LSB methods store at most 1 bit per carrier byte.

### Tests
- `test_capacity_estimate_bounded_by_raw_data`: regression test with a 10,000-byte grayscale
  carrier (100×100 pixels, 1 byte/pixel). Verifies all per-stealth-level estimates are ≤
  `raw_data.len()` bits (10,000), not the inflated 30,000 that dimensions alone would imply.

## [0.3.16] - 2026-02-22

### Fixed
- **DICOM codec integration** (`src/handlers/dicom.rs`):
  `embed()` passed `payload` directly to strategy functions without calling
  `codec::encode_payload()`; `extract()` returned raw bytes without calling
  `codec::decode_payload()`. Both `encryption_key` and `error_correction_level` in
  `EmbedParams` were silently ignored for all DICOM embedding strategies. Now guarded by
  `use_codec` (non-empty key or non-None EC level), completing the codec integration fix
  started in v0.3.15 for VCF, SAM, and BAM. Also fixes `encryption_time_ms` in the
  returned `EmbedResult` (was always hardcoded to `0`).

### Documentation
- `ImageEmbedStrategy::DCTFrequency` and `PVD` now carry doc comments explicitly stating
  they are not yet implemented and will return `SteganoError::InvalidParameter` at runtime.

### Tests
- `test_dicom_private_elements_with_encryption_roundtrip`: embed+extract with AES-256-GCM
  key using PrivateDataElements strategy; also verifies a wrong key returns an error.

## [0.3.15] - 2026-02-22

### Fixed
- **AdaptiveLSB extract/embed asymmetry** (`src/handlers/imagery.rs`):
  `extract_with_strategy()` read every byte sequentially (identical to SimpleLSB) while
  `embed_lsb_pixels()` only writes to bytes where local variation > 10. Roundtrip would
  silently corrupt any image with smooth regions (variation ≤ 10), because the extractor
  consumed bits from positions the embedder had skipped. Both header and payload reading
  blocks now apply the same threshold check.
- **Extract strategy order fixed** (`src/handlers/imagery.rs`):
  `extract()` tried `[AdaptiveLSB, SimpleLSB, EdgeBased]` but `embed()` maps
  `Low → SimpleLSB`, `Medium → AdaptiveLSB`, `High/Max → EdgeBased`. Reversed to
  `[SimpleLSB, AdaptiveLSB, EdgeBased]` to avoid an unnecessary failed AdaptiveLSB attempt
  on the common case (Low stealth = SimpleLSB).
- **VCF codec integration** (`src/handlers/vcf.rs`):
  `embed()` passed `payload` directly to strategy functions without calling
  `codec::encode_payload()`; `extract()` returned raw bytes without calling
  `codec::decode_payload()`. Both `encryption_key` and `error_correction_level` in
  `EmbedParams` were silently ignored for all VCF embedding strategies. Now guarded by
  `use_codec` (non-empty key or non-None EC level), matching the pattern in imagery.rs.
- **SAM/BAM codec integration** (`src/handlers/sam_bam.rs`):
  Same bug — `encryption_key` and `error_correction_level` were ignored for SAM and BAM
  embed/extract. Both handlers now apply `codec::encode_payload`/`decode_payload` when the
  codec parameters are non-default.

### Tests
- `test_adaptive_lsb_roundtrip_with_smooth_patches`: image with 50% flat bytes (variation=0)
  and 50% LCG-random bytes; verifies roundtrip with `StealthLevel::Medium` (AdaptiveLSB).
- `test_vcf_info_embedding_with_encryption_roundtrip`: embed+extract with AES-256-GCM key
  in VCF INFO fields; also verifies a wrong key returns an error.
- `test_sam_optional_fields_with_encryption_roundtrip`: same for SAM optional fields.

## [0.3.14] - 2026-02-22

### Fixed
- **Capacity estimation mismatch for imagery** (`src/handlers/imagery.rs`):
  `estimate_capacity()` always reported `AdaptiveLSB` (2 bits/pixel, Medium stealth) regardless
  of the strategy that `embed()` would actually use. Selecting `StealthLevel::High` or `Maximum`
  invokes `EdgeBased` (~0.3 bits/pixel), which has only 15% of AdaptiveLSB's capacity.
  Users relying on the estimate would receive `InsufficientCapacity` errors at embed time.
  Fix: `format_specific_info` now includes `capacity_{low,medium,high,maximum}_stealth_bits`
  so callers can check capacity for their intended stealth level.
- **Capacity estimation mismatch for signals** (`src/handlers/signals.rs`):
  Same issue — `estimate_capacity()` always reported `FrequencyDomain` (Medium stealth).
  `SpreadSpectrum` (Maximum stealth) can have 40× less capacity. All four stealth-level
  estimates now present in `format_specific_info`.

### Removed
- **Dead code** (`src/handlers/imagery.rs`): Removed `ImageEmbedStrategy::from_u8()`.
  The strategy byte is written into the embedded header by `to_u8()` but was never read back
  during extraction, so the round-trip deserialization this function implied never existed.

### Documentation
- `DicomEmbedStrategy::UnusedMetadata` and `SequencePadding` now carry doc comments
  explicitly stating they are not yet implemented and will return an error at runtime.

### Tests
- Extended `test_capacity_estimation` in both imagery and signals to assert:
  - All per-stealth-level capacity entries are present in `format_specific_info`
  - Capacities are ordered correctly (low ≥ medium ≥ high ≥ maximum)

## [0.3.13] - 2026-02-22

### Fixed
- **StealthLevel::Maximum now uses SpreadSpectrum** (`src/handlers/signals.rs`): Both
  `embed()` and `extract()` mapped `StealthLevel::Maximum` to `PhaseModulation`, making
  it identical to `StealthLevel::High`. Now correctly maps to `SpreadSpectrum`.
- **DICOM `create_minimal_dicom()` corrupt Transfer Syntax UID length** (`src/handlers/dicom.rs`):
  The helper claimed length `0x1A` (26 bytes) for a Transfer Syntax UID value that is
  18 bytes. The DICOM parser over-read 8 extra bytes, consuming the next element's tag
  header and breaking subsequent element parsing. Fixed to `0x12` (18 bytes).
- **SAM/BAM `AuxiliaryData` unreachable via public API** (`src/handlers/sam_bam.rs`):
  The `sam_strategy` string dispatch had no entry for `"auxiliary"`. Added
  `"auxiliary" => SamBamEmbedStrategy::AuxiliaryData` to both embed and extract dispatch.

### Removed
- **Dead code** (`src/handlers/signals.rs`): Removed unused `SignalEmbedStrategy::from_u8()`
  method (annotated `#[allow(dead_code)]`).
- **Unused dev-dependencies** (`Cargo.toml`): Removed `cargo-tarpaulin = "0.27"` and
  `coverage-helper = "0.2"` from `[dev-dependencies]` — neither is used in any test.

### Tests
- Added `test_adaptive_lsb_embedding_roundtrip` (imagery): verifies AdaptiveLSB embed/extract round-trip
- Added `test_vcf_variant_id_embedding_roundtrip` (vcf): verifies VariantIdSteganography ("id" strategy)
- Added `test_empty_payload_all_ec_levels` (codec): empty payload round-trips for ECLevel::None/Medium/High/Maximum
- Added `test_sam_auxiliary_data_embed_error` and `test_sam_auxiliary_data_extract_error` (sam_bam)
- Added `test_dicom_unused_metadata_embed_error`, `test_dicom_unused_metadata_extract_error` (dicom)
- Added `test_dicom_sequence_padding_embed_error`, `test_dicom_sequence_padding_extract_error` (dicom)
- Total: **346 tests passing** (up from 337)

## [0.3.12] - 2026-02-22

### Fixed
- **DICOM PixelDataLSB serialisation bug** (`src/handlers/dicom.rs`): `embed_pixel_data_lsb`
  modified `dicom.pixel_data.pixel_data` in memory but `serialize_dicom` only serialised
  the original `element.value` from `data_elements`. After any embed call, the modified
  pixel bytes are now synced back to the matching data element before serialisation, so
  extract round-trips correctly.
- **Signal capacity estimates** (`src/handlers/signals.rs`): `SpreadSpectrum` incorrectly
  used `duration_seconds * 100` (bits) with 512-bit overhead; corrected to
  `sample_count / chip_length` where `chip_length = sampling_rate / 1000`.
  `EchoHiding` incorrectly used `duration_seconds * 50` with 256-bit overhead; corrected
  to `sample_count / segment_length` where `segment_length = sampling_rate / 10`.
  Both now use the correct 80-bit header overhead.
- **Imagery header overhead** (`src/handlers/imagery.rs`): `AdaptiveLSB` overhead was 96 bits
  (should be 80); `EdgeBased` overhead was 128 bits (should be 80). All strategies now
  correctly report the actual 10-byte (80-bit) header.
- **Imagery gradient overflow** (`src/handlers/imagery.rs`): `embed_edge_based` and
  `extract_edge_bits` computed `gx * gx + gy * gy` using `i16` arithmetic, which overflows
  when pixel differences exceed ±181. Now uses `i32` before squaring.
- **VCF O(n×10000) extraction** (`src/handlers/vcf.rs`): `extract_info_field_strategy`
  iterated 0..10000 candidate keys per variant to find steganographic INFO fields.
  Replaced with direct key filtering using `strip_prefix`, making extraction O(fields).
- **FFI version string** (`src/ffi.rs`): Updated from `"0.3.11"` to `"0.3.12"`.

### Added
- **SpreadSpectrum roundtrip test** (`src/handlers/signals.rs`):
  `test_spread_spectrum_embedding_roundtrip` verifies the full embed→extract cycle using
  a silent (zero) carrier, ensuring the PN-correlation SNR is deterministically sufficient.
- **EchoHiding roundtrip test** (`src/handlers/signals.rs`):
  `test_echo_hiding_embedding_roundtrip` verifies the full embed→extract cycle using a
  white-noise-like carrier (Xorshift32 PRNG) with negligible baseline autocorrelation.
- **EdgeBased imagery roundtrip test** (`src/handlers/imagery.rs`):
  `test_edge_based_embedding_roundtrip` verifies the full embed→extract cycle using a
  pseudo-random LCG carrier that produces very high pixel gradients (nearly every interior
  pixel exceeds the 20.0 threshold).
- **DICOM PixelDataLSB roundtrip test** (`src/handlers/dicom.rs`):
  `test_dicom_pixel_lsb_roundtrip` verifies the full embed→extract cycle; also served as
  the regression test that caught the serialisation bug above.

## [0.3.11] - 2026-02-22

### Fixed
- **FFI version string** (`src/ffi.rs`): Updated from `"0.3.9"` to `"0.3.11"`.
- **CLI hardcoded default key** (`src/cli/commands.rs`): `handle_embed` and `handle_extract`
  previously fell back to `b"default_key"` when no `--password` was supplied, silently
  encrypting with a known key. Now falls back to an empty key (no encryption) so the
  behaviour is honest and non-deceptive.
- **SAM/BAM `AuxiliaryData` silent fallback** (`src/handlers/sam_bam.rs`): Both the embed
  and extract dispatch arms previously caught `AuxiliaryData` with a wildcard and silently
  used `OptionalFields`. They now return `InvalidParameter` explicitly, consistent with the
  other unimplemented strategies.

### Added
- **SAM/BAM `QualityScores` roundtrip test** (`src/handlers/sam_bam.rs`):
  `test_sam_quality_scores_embedding_roundtrip` verifies the quality-score strategy
  end-to-end, including biological-integrity check after embedding.

### Changed (Clippy / code quality)
- `src/handlers/vcf.rs`: Removed unused constant `CUSTOM_ID_PREFIX`.
- `src/codec.rs`: Replaced `% 255 != 0` with `.is_multiple_of(255)`.
- `src/handlers/sam_bam.rs`: Replaced `repeat(0u8).take(n)` with `repeat_n(0u8, n)`;
  replaced manual `(x + 1) / 2` with `.div_ceil(2)`.
- `src/ml/models.rs`: Fixed over-indented doc list item.
- `src/monitoring/telemetry.rs`: Removed redundant `as i64` casts on `tv_sec` fields.
- `src/monitoring/mod.rs`: Added `#[allow(clippy::await_holding_lock)]` on `start`/`stop`
  (sequential single-task calls; deadlock risk is accepted and documented).
- `src/validation.rs`: Replaced four `.max(0.0).min(1.0)` chains with `.clamp(0.0, 1.0)`;
  replaced `% 2 != 0` with `.is_multiple_of(2)`; added `Default` impls for
  `PerformanceAnalysis`, `ValidationReport`, and `TestDatasetManager`.
- `README.md`: Updated test badge to `333+`.

## [0.3.10] - 2026-02-22

### Fixed
- **FFT Hermitian symmetry** (`src/handlers/signals.rs`): `embed_fft_magnitude` and
  `embed_fft_phase` previously modified both a frequency bin and its conjugate independently,
  breaking Hermitian symmetry. The IFFT real part then produced `(Z[k] + conj(Z[N-k])) / 2`
  instead of `Z[k]`, scrambling the embedded parity. Both functions now only modify
  positive-frequency bins (`[N/4..N/2)` for magnitude, `[N/8..N/4)` for phase) and explicitly
  set the conjugate bin to `conj(X[k])`, ensuring the IFFT output is real and extraction
  faithfully recovers the encoded bits.
- **FFT capacity accounting** (`src/handlers/signals.rs`): `estimate_signal_capacity` for
  `FrequencyDomain` now correctly counts `chunk_size / 4` usable bits per chunk (the
  positive-frequency half only) instead of `chunk_size / 2`.
- **Quality-score valid range** (`src/handlers/sam_bam.rs`): `extract_quality_scores` now
  filters to characters in `34..=126` (was `33..=126`). ASCII 33 (`!`) with its LSB cleared
  becomes 32 (space), an invalid PHRED character; excluding it prevents corrupt quality strings.
- **Imagery DCT/PVD capacity** (`src/handlers/imagery.rs`): `estimate_image_capacity` for
  `DCTFrequency` and `PVD` strategies now returns `(0, 0)` instead of a bogus non-zero estimate,
  correctly reflecting that these strategies are not yet implemented.
- **DICOM UnusedMetadata/SequencePadding capacity** (`src/handlers/dicom.rs`): Same fix —
  `estimate_dicom_capacity` returns `0` for these unimplemented strategies.
- **FFI version string** (`src/ffi.rs`): `stegano_version()` now returns `"0.3.10"` (was stale
  `"0.2.2"`).
- **FFI error mapping** (`src/ffi.rs`): `SteganoError::SerializationError` now maps to
  `SteganoErrorCode::DataCorrupted` instead of `InvalidParameter`.
- **Frequency capacity formula** (`src/capacity/mod.rs`): `estimate_frequency_capacity` now
  uses `signal_length / 2` (was `/4`), correctly reflecting that half the frequency bins are
  usable. Test assertions updated accordingly.

### Added
- **VCF sample-FORMAT roundtrip test** (`src/handlers/vcf.rs`): New
  `test_vcf_sample_format_embedding_roundtrip` covers the `format` strategy end-to-end.
- **Signal FFT roundtrip tests** (`src/handlers/signals.rs`): New
  `test_frequency_domain_embedding_roundtrip` and `test_phase_modulation_embedding_roundtrip`
  verify the FFT magnitude and phase strategies produce correct roundtrips.

## [0.3.9] - 2026-02-22

### Fixed
- **VCF quality-score self-framing** (`src/handlers/vcf.rs`): `embed_quality_strategy` now prepends
  a 4-byte little-endian length header before encoding payload bits; `extract_quality_strategy`
  reads that header to determine exact payload length, eliminating over-read from extra variants.
- **VCF variant-ID ordering** (`src/handlers/vcf.rs`): Every chunk now carries an embedded sequence
  index (`STEG{i:04}_<chunk>`). Extraction sorts collected chunks by index before concatenating,
  making round-trips correct even if variant order changes between embed and extract.
- **VCF sample-FORMAT ordering** (`src/handlers/vcf.rs`): Custom FORMAT fields use per-variant
  indexed tags (`S{i:02}`) instead of a static `STEG` tag. Extraction sorts by numeric index,
  ensuring correct bit ordering.
- **Codec RS overflow** (`src/codec.rs`): `rs_encode` now uses `checked_mul(255).and_then(…)`
  when computing the output buffer capacity, returning `InvalidParameter` instead of panicking on
  implausibly large payloads.
- **Imagery DCT/PVD silent fallback** (`src/handlers/imagery.rs`): `embed_lsb` and
  `extract_with_strategy` for `DCTFrequency` and `PVD` variants now return an explicit
  `InvalidParameter` error instead of silently falling back to SimpleLSB. `AdaptiveLSB` is
  correctly handled using the same sequential pixel-LSB path as `SimpleLSB`.
- **DICOM UnusedMetadata / SequencePadding silent fallback** (`src/handlers/dicom.rs`): Both embed
  and extract now return an explicit `InvalidParameter` error for these unimplemented strategy
  variants instead of silently falling back to `PrivateDataElements`.
- **Signal extraction per-strategy dispatch** (`src/handlers/signals.rs`): `extract_with_strategy`
  now dispatches to dedicated extraction functions for each strategy
  (`extract_fft_magnitude_bits`, `extract_fft_phase_bits`, `extract_spread_spectrum_bits`,
  `extract_echo_hiding_bits`) instead of always using the time-domain LSB path for non-time-domain
  embeds.
- **FFT magnitude quantization** (`src/handlers/signals.rs`): `embed_fft_magnitude` switched from
  a multiplicative modifier (not invertible) to parity quantization against a fixed step size
  (`MAG_STEP = 0.01`), making `extract_fft_magnitude_bits` reliably recover the embedded bits.

## [0.3.8] - 2026-02-22

### Fixed
- **DICOM PixelDataLSB** (`src/handlers/dicom.rs`): `embed_pixel_data_lsb` now prepends a 4-byte
  little-endian length header before the payload bits; `extract_pixel_data_lsb` reads that header
  to self-discover payload length instead of using a hardcoded `estimated_length = 100`.
- **DICOM parse limit** (`src/handlers/dicom.rs`): Removed the artificial 1000-element cap in
  `parse_dicom()` that silently truncated large DICOM files.
- **SAM/BAM QualityScores** (`src/handlers/sam_bam.rs`): Same self-framing fix as DICOM —
  `embed_quality_scores` prepends a 4-byte length header and `extract_quality_scores` reads it,
  eliminating the hardcoded `estimated_length = 100`.
- **BAM format** (`src/handlers/sam_bam.rs`): BAM embed and extract are now fully implemented.
  Embed appends an `@CO STEGANO_DATA:<hex>` line to the BAM header text; extract recovers it.
  `serialize_bam()` added to round-trip a `BamFile` struct back to binary. The 100-alignment
  parse cap in `parse_bam()` is removed.
- **VCF quality-score encoding** (`src/handlers/vcf.rs`): Replaced the unreliable IEEE 754
  decimal-place approach with a lossless integer scheme: 3 bits are packed into the low 3 bits of
  the rounded integer quality score (`qual = (qual & !7) | triplet`). Serialization changed from
  `{:.3}` (clipped decimal) to `{}` (integer), ensuring bits survive the write/re-read cycle.
- **FFI Tokio runtime** (`src/ffi.rs`): Replaced per-call `Runtime::new()` with a single shared
  `OnceLock<Runtime>` (`TOKIO_RT`) initialized once on first use, eliminating the resource leak.
- **Stale integration test assertions** (`tests/test_0_1_1_fixes.rs`, `tests/test_basic_functionality.rs`):
  Fixed hardcoded version assertions ("0.2.1") and carrier sizes that became too small once
  Reed-Solomon error correction became functional.

## [0.3.7] - 2026-02-22

### Added
- **`src/codec.rs`** — New self-framing encode/decode pipeline that wires
  `EmbedParams::encryption_key` and `EmbedParams::error_correction_level` to
  real, functional implementations:
  - `encode_payload(payload, params)`: AES-256-GCM encryption (from key)
    followed by Reed-Solomon error correction (from ECLevel), returns a
    self-framing byte blob that `decode_payload` can reverse without external
    metadata.
  - `decode_payload(data, params)`: RS decoding then AES-GCM decryption; the
    same params must be used as were supplied to `encode_payload`.
  - `normalize_key(key)`: hashes any-length keys to exactly 32 bytes via Blake3
    so AES-256-GCM always receives a valid key.
  - 21 new unit tests cover no-op, encryption-only, each ECLevel, combined
    encrypt+EC, short/long/empty keys, empty payloads, large payloads, wrong
    key decryption failure, and missing key failure.
- **ECLevel→RS mapping**: `ECLevel::Low`→16 ECC bytes, `Medium`→32, `High`→64,
  `Maximum`→128 per 255-byte RS block. Arbitrary-length payloads are chunked
  into fixed 255-byte RS blocks with zero-padding; the original length is
  stored in a 4-byte header so the decoder can trim padding.

### Changed
- **All handlers now respect `params.encryption_key` and
  `params.error_correction_level`** (`generic.rs`, `imagery.rs`,
  `signals.rs`, `genomics.rs`):
  - When `encryption_key` is non-empty or `error_correction_level != None`,
    `embed()` calls `codec::encode_payload` before embedding the bytes and
    `extract()` calls `codec::decode_payload` after extraction.
  - `imagery.rs` and `signals.rs`: the header `is_encrypted` flag byte is
    now a `codec_mode` byte — 0 = raw, 1 = legacy `self.crypto` password
    path (backward-compatible), 2 = new codec pipeline — so existing embedded
    data created with `with_encryption()` still extracts correctly.
- **Capacity estimates corrected**: `imagery.rs` SimpleLSB header overhead
  64 bits → 80 bits (10-byte header); `signals.rs` TimeDomainLSB header
  overhead 128 bits → 80 bits, matching the actual 10-byte header format.
- **7 unused dependencies removed** from `Cargo.toml`: `wide`, `ndarray`,
  `image`, `tiff`, `bio`, `csv`, `wav` — none were imported anywhere in the
  source tree.

### Fixed
- `embedding.rs` tests `test_embedding_with_validation` and
  `test_embedding_without_validation`: carrier size increased from 1024 to
  8192 bytes to accommodate the RS overhead introduced by the now-functional
  `ECLevel::Medium` default in `EmbedParams::new`.
- `generic.rs` test `test_empty_payload_embedding`: carrier size increased
  from 100 to 4096 bytes for the same reason.

## [0.3.3] - 2026-02-22

### Fixed
- **Statistical tests** (`src/validation.rs`): Chi-square p-value corrected to one-tailed Fisher
  approximation — removed erroneous `.abs()` and `2.0 *` that were converting small chi-square
  values to incorrectly small p-values. O(n²) naive DFT in `simple_fft()` replaced with
  `rustfft` O(n log n). K-fold cross-validation closure signature changed from `-> Result<f64>`
  (accuracy) to `-> Result<Vec<f64>>` (predicted scores); fold AUC now computed via Mann-Whitney
  U statistic rather than a 0.5-threshold accuracy proxy.
- **Distribution and outlier analysis** (`src/ml/analysis.rs`): Replaced five stub
  implementations with real algorithms:
  - `calculate_descriptive_stats()` now computes mean, std_dev, min, max, median, Q1/Q3,
    skewness, and kurtosis for all four tracked metrics
  - `DistributionAnalyzer::analyze()` performs KS goodness-of-fit against Normal, Exponential,
    and Uniform distributions; best fit is the distribution with the smallest KS statistic
  - `OutlierAnalyzer::analyze()` dispatches to IQR-fence, Z-score (|z|>threshold), or
    modified Z-score (MAD-based) detection depending on `self.method`
  - `SamplePairsDetector::evaluate()` implements the SP (Sample Pairs) histogram analysis
    used in classic LSB steganalysis
  - `HistogramDetector::evaluate()` computes Total Variation Distance (TVD = L₁/2) between
    original and stego byte-frequency histograms
- **Process telemetry** (`src/monitoring/telemetry.rs`): Replaced `rand::random` fakes with
  real `libc::getrusage(RUSAGE_SELF)` calls. `collect_memory_sample()` reads current and peak
  RSS; `collect_cpu_sample()` computes delta CPU% from two snapshots 50 ms apart. Handles
  macOS (`ru_maxrss` in bytes) and Linux (`ru_maxrss` in KiB, VmRSS from `/proc/self/status`)
  differences. `#[cfg(not(unix))]` stubs return errors on non-Unix targets.
- **GPU model CPU fallback** (`src/ml/cuda_models.rs`): `forward_gpu()` CPU path previously
  returned `vec![0.5; output_size]` regardless of input. Now calls `cpu_forward()`, a real
  dense+sigmoid forward pass through all model layers. `predict_optimal_params()` refactored
  to reuse `cpu_forward()` rather than duplicating the same loop inline.

### Changed
- Test count increased from 261 to 309 (+48 tests covering all fixed implementations)

## [0.3.2] - 2026-02-21

### Added
- **CUDA Acceleration**: NVIDIA GPU backend via `cudarc 0.12` behind the optional `cuda` feature flag
  - `CudaAccelerator` with `parallel_lsb_embed()` and `parallel_lsb_extract()` powered by NVRTC-compiled kernels
  - Kernels compiled at runtime so the driver JITs them to the native GPU architecture (forward-compatible with Blackwell sm_120 and beyond)
  - `GpuBackend::Cuda` variant wired into `best_available()` with priority Metal > CUDA > WGPU > CPU
  - Enable with `--features cuda`; stub struct on non-CUDA targets keeps all other builds unaffected

### Fixed
- **Cross-platform build**: Dependencies placed after `[target.'cfg(target_os = "macos")'.dependencies]` in `Cargo.toml` were silently treated as macOS-only by TOML, causing 64 compilation errors on Linux. Moved all non-macOS-specific deps (`log`, `clap`, `hex`, `libc`, `bio`, `csv`, `wav`, `reed-solomon`, etc.) above the platform-specific section.

## [0.3.1] - 2026-02-21

### Added
- **Validation Dashboard**: `ValidationReport` now implements `fmt::Display`, rendering a human-readable text dashboard with per-test results (statistic, p-value, pass/fail) and recommendations
- **JSON Export**: `ValidationReport::to_json_string()` convenience method wraps `serde_json::to_string_pretty` for machine-readable report output
- **Validation Summary API**: `ValidationReport::summary()` returns a compact `ValidationSummary` struct with `tests_run`, `passed`, `overall_score`, and `status` fields
- **Community Infrastructure**: GitHub issue templates for bug reports and feature requests; blank-issue guard with link to discussions
- **Validation User Guide**: `docs/VALIDATION_GUIDE.md` covering how to enable validation, interpret dashboard output, and understand scoring

### Changed
- **Parallel Classical Tests**: The five independent classical statistical tests (chi-square, KS, runs, Anderson-Darling, autocorrelation) in `run_classical_tests()` now execute concurrently using `rayon::join` chains, reducing wall-clock validation time on multi-core systems

### Fixed
- Version badge in README updated to 0.3.1
- Test count badge updated to 261+

## [0.3.0] - 2025-01-12

### Added
- **Statistical Validation Framework**: Comprehensive 11/10 excellence level validation suite
  - Chi-square goodness of fit tests with p-value calculations
  - Kolmogorov-Smirnov distribution comparison tests
  - Runs tests for randomness assessment
  - Anderson-Darling normality tests
  - Autocorrelation analysis with Ljung-Box statistics
  - ROC analysis and AUC calculations for detection performance
  - Pixel pair analysis for image steganalysis
  - Wavelet-based structural analysis for advanced steganalysis
  - Cross-validation framework for model performance
  - Publication-ready statistical reports
- **High-Level Embedding API**: New `SteganoEmbedder` unified interface
  - Integrated validation pipeline with automatic reporting
  - Format-specific validation tailored to data types
  - Performance metrics and timing analysis
- **CLI Validation Integration**: Enhanced command-line interface
  - `--validate` flag for all operations
  - Real-time validation scoring and recommendations
  - Statistical analysis integration in analyze command
  - Enhanced error reporting and user feedback
- **Modern Security & Supply Chain Tools**
  - `cargo-audit` integration for vulnerability scanning
  - `cargo-deny` configuration for license compliance and policy enforcement
  - Automated security auditing via GitHub Actions
  - Comprehensive security documentation (SECURITY.md)
  - Supply chain monitoring with dependency analysis
  - Security audit script with reporting capabilities
- **CI/CD Improvements**
  - Reduced notification spam (95% reduction in GitHub notifications)
  - Daily security audits instead of constant monitoring
  - Optimized workflow scheduling for better resource usage
- **Development Practices**
  - Semantic Versioning (SemVer 2.0) compliance
  - Keep a Changelog format adoption
  - Developer guidelines documentation
- **Enhanced Handlers**: Format-specific embedding strategies for advanced steganography
  - Enhanced signals handler with 5 sophisticated embedding techniques
  - Enhanced imagery handler with 5 advanced embedding strategies  
  - FFT-based frequency domain embedding for audio signals
  - Edge-based embedding for imperceptible image steganography
  - Comprehensive strategy selection based on stealth levels
  - Improved capacity estimation for different embedding strategies

### Changed
- **Core API Extensions**: Enhanced `EmbedParams` and `EmbedResult`
  - Added `enable_validation` field to `EmbedParams` for validation control
  - Added `validation_report` field to `EmbedResult` for comprehensive reporting
  - Added `.with_validation()` convenience method for easy enabling
- **CI/CD Workflow Optimization**
  - `ci-monitoring.yml`: Reduced from every 30 minutes to daily at 9 AM UTC (weekdays only)
  - `status-dashboard.yml`: Reduced from hourly to twice daily (9 AM, 6 PM weekdays)
  - Added `[skip ci]` tags to prevent cascading workflows
- **Project Metadata**
  - Updated author email to scttfrdmn@gmail.com
  - Updated license specification to MIT format
  - Enhanced project description and documentation
- **Version Bump**: Updated from 0.2.2 to 0.3.0 for major feature release
- **Handler Improvements**: Support for multiple embedding strategies per format
  - Better error handling for insufficient capacity scenarios
  - Improved test coverage for enhanced handlers

### Fixed
- **Compilation Issues**: Resolved all validation integration compilation errors
  - Fixed missing `enable_validation` fields in 20+ EmbedParams struct initializations
  - Fixed missing `validation_report` fields in 5+ EmbedResult struct initializations
  - Ensured backward compatibility with existing API usage
- **CI/CD Notification Spam**: Eliminated excessive GitHub workflow notifications
  - Reduced workflow execution frequency by 95%
  - Improved notification relevance and reduced noise
- **Handler Fixes**: Multiple technical improvements
  - SecurePayload serialization/deserialization in handlers
  - Capacity calculation errors in bit/byte conversions
  - Type safety for strategy enum conversions
  - Literal overflow issues in LSB manipulation
  - Metal GPU backend compilation on non-macOS platforms
- **Documentation**: Updated collaboration and security documentation
  - Fixed contact information throughout project
  - Enhanced security policy and vulnerability reporting procedures

### Security
- **Enhanced Security Framework**: Multi-layered protection improvements
  - Added automated vulnerability scanning with daily audits
  - Implemented license compliance monitoring
  - Created comprehensive security policy documentation
  - Added supply chain security monitoring
  - Enhanced dependency management with cargo-deny policies
- **Cryptographic Improvements**: Maintained existing strong encryption
  - AES-256-GCM and ChaCha20-Poly1305 encryption options
  - Blake3 key derivation with configurable parameters
  - Reed-Solomon error correction with distributed recovery
  - Post-quantum readiness architecture
- **Secure Payload Handling**: Enhanced security with proper encryption payload handling

## [0.1.0] - 2025-01-03

### Added
- Metal GPU acceleration support for macOS (M4 Pro optimized)
- Comprehensive development rules and quality gates
- 85% code coverage requirement with extensive test suite
- Advanced steganography techniques for scientific data
- Multi-format support (FASTA, FASTQ, images, signals)
- AES-256-GCM encryption with authenticated payloads
- GPU-accelerated operations for high-performance processing
- Extensive documentation and API reference
- Cross-platform compatibility with fallback implementations

### Security
- BLAKE3 cryptographic hashing for integrity verification
- Authenticated encryption with automatic key derivation
- Secure payload serialization with tamper detection
- Memory-safe Rust implementation throughout