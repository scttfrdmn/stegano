# API reference

An overview of the public Rust API in v0.8.1 and of the language bindings.
Run `cargo doc --open` for the complete, authoritative rustdoc. Format
behaviour (strategy names, what changes in a file, capacity) is in
[FORMATS.md](FORMATS.md). The payload byte layout is in
[WIRE_FORMAT.md](WIRE_FORMAT.md).

Most items are re-exported at the crate root (`use stegano::*;` brings in
`stegano::core::*`, `SteganoEmbedder`, `DataHandler`, `CryptoLayer`,
`SecurePayload`, `CapacityEstimator`, `StatisticalValidationSuite`,
`ValidationReport` and `serde_json`).

## Contents

- [Core types](#core-types)
- [EmbedParams](#embedparams)
- [CipherKind, ECLevel, StealthLevel](#cipherkind-eclevel-stealthlevel)
- [Results and errors](#results-and-errors)
- [SteganoEmbedder and handlers](#steganoembedder-and-handlers)
- [Streaming (`stegano::stream`)](#streaming-steganostream)
- [Carrier construction and defaults (`stegano::carrier`)](#carrier-construction-and-defaults-steganocarrier)
- [Lower-level modules](#lower-level-modules)
- [Validation (`stegano::validation`)](#validation-steganovalidation)
- [Cargo features](#cargo-features)
- [Bindings](#bindings)

## Core types

Defined in `stegano::core`.

### `DataFormat`

```rust,ignore
// Excerpt (not compilable on its own): the variants of stegano::core::DataFormat.
pub enum DataFormat {
    FASTA, FASTQ, VCF, SAM, BAM,          // genomics
    TIFF, PNG, MICROSCOPY, DICOM,          // imagery
    WAV, BinarySignal, CsvTimeseries,      // signals
    BINARY, TEXT,                          // generic
    FITS,                                  // astronomy
}
```

Helpers: `is_genomics()`, `is_imagery()`, `is_signal()`, `extension()`.
`MICROSCOPY` is handled as PNG/TIFF.

### `CarrierData`

| Field | Type | Meaning |
|-------|------|---------|
| `format` | `DataFormat` | Selects the handler. |
| `raw_data` | `Vec<u8>` | The file bytes. Rewritten in place by `embed`. |
| `metadata` | `HashMap<String, serde_json::Value>` | Free-form; not used by the handlers. |
| `dimensions` | `Option<Dimensions>` | Image geometry, used by validation. |
| `sampling_rate` | `Option<f64>` | Used for raw signal carriers (default 44 100 Hz). |

`CarrierData::new(format, data)` takes the format first and leaves
`dimensions` and `sampling_rate` unset. `stegano::carrier::build_carrier` and
`carrier_from_bytes` fill them in from the file header and are what the CLI
and bindings use. Other methods: `size()`, `is_empty()`, `add_metadata()`,
`get_metadata()`.

`Dimensions { width, height, depth: Option<u32>, channels: Option<u32> }`
with `new_2d`, `new_3d` and `total_elements`.

## EmbedParams

| Field | Type | `EmbedParams::new(key)` |
|-------|------|-------------------------|
| `encryption_key` | `Vec<u8>` | `key`. A password, stretched with Argon2id. Empty means no encryption. Redacted from `Debug`, never serialized. |
| `error_correction_level` | `ECLevel` | `Medium` |
| `stealth_level` | `StealthLevel` | `High` |
| `use_gpu` | `bool` | `false`. Not read by any handler. |
| `enable_validation` | `bool` | `false` |
| `format_specific` | `HashMap<String, serde_json::Value>` | empty |
| `cipher_kind` | `CipherKind` | `Aes256Gcm` |

`EmbedParams::default()` is `EmbedParams::new(vec![])`.

Builders:

| Method | Effect |
|--------|--------|
| `EmbedParams::new(key)` | Defaults above. |
| `EmbedParams::with_error_correction(key, level)` | As `new`, with the given EC level. |
| `.with_stealth_level(level)` | Sets `stealth_level`. |
| `.with_ec_level(level)` | Sets `error_correction_level`. |
| `.with_chacha20()` | Sets `cipher_kind = ChaCha20Poly1305`. |
| `.with_validation()` | Sets `enable_validation` (in-memory `SteganoEmbedder` only). |
| `.with_gpu()` | Sets `use_gpu`. No effect on embedding. |

```rust
use stegano::serde_json::json;
use stegano::{ECLevel, EmbedParams, StealthLevel};

let mut params = EmbedParams::new(b"correct horse battery staple".to_vec())
    .with_stealth_level(StealthLevel::Medium)
    .with_ec_level(ECLevel::Low)
    .with_chacha20();
params.format_specific.insert("vcf_strategy".into(), json!("id"));
assert_eq!(params.stealth_level, StealthLevel::Medium);
```

`format_specific` keys read by the handlers: `genomics_strategy`,
`vcf_strategy`, `sam_strategy`, `bam_compression`, `imagery_strategy`,
`dicom_strategy`, `dicom_private_group`, `signal_strategy`, `fits_strategy`.
Their values are listed per format in [FORMATS.md](FORMATS.md). Unknown
strategy names are rejected with `SteganoError::InvalidParameter`.

## CipherKind, ECLevel, StealthLevel

**`CipherKind`**: `Aes256Gcm` (default, byte 0) and `ChaCha20Poly1305` (byte 1),
with `to_u8`/`from_u8`. Both use a 256-bit key derived with Argon2id
(m = 64 MiB, t = 3, p = 1) and a random salt and nonce per payload. The cipher
is recorded in the payload header and detected on extract. See
[WIRE_FORMAT.md](WIRE_FORMAT.md).

**`ECLevel`**: Reed-Solomon parity per 255-byte codeword.

| Level | Parity bytes | `overhead_percent()` (nominal) |
|-------|--------------|-------------------------------|
| `None` | 0 | 0.0 |
| `Low` | 16 | 0.1 |
| `Medium` | 32 | 0.25 |
| `High` | 64 | 0.5 |
| `Maximum` | 128 | 1.0 |

The real overhead is set by the codeword layout in
[WIRE_FORMAT.md](WIRE_FORMAT.md): any level other than `None` pads the body to
whole 255-byte codewords, so even a short payload needs about 300 bytes or
more of capacity.

**`StealthLevel`**: `Maximum`, `High`, `Medium`, `Low`. It selects the default
strategy for PNG/TIFF, WAV and FASTQ when no strategy key is given (see
[FORMATS.md](FORMATS.md)); the other handlers ignore it.
`capacity_multiplier()` returns 0.1 / 0.3 / 0.6 / 1.0. That number does not
describe detectability, and no handler uses it to scale embedding.

## Results and errors

- `EmbedResult { success, bytes_embedded, carrier_modified, performance_stats, validation_report }`.
- `PerformanceStats { embedding_time_ms, encryption_time_ms, gpu_acceleration_used, memory_usage_bytes }`,
  with `total_time_ms()` and `throughput_bps(n)`. `gpu_acceleration_used` is always false.
- `CapacityInfo { total_bits, recommended_bits, steganographic_ratio, format_specific_info }`,
  with `recommended_bytes()`, `can_fit(payload_len)`, `utilization_percent(payload_len)`.
  `recommended_bits` is the handler's estimate of usable payload space.
  Handlers differ in how much framing they subtract, so leave a margin. The
  codec header is 36 bytes, encryption adds a 16-byte tag, and error correction
  adds more (see above).
- `SteganoError`: `UnsupportedFormat`, `InsufficientCapacity { needed, available }`,
  `EncryptionError`, `GpuError`, `IntegrityError`, `InvalidParameter`, `IoError`,
  `SerializationError`. Methods `is_recoverable()` and `category()`.
- `stegano::Result<T>` is `Result<T, SteganoError>`.

A wrong password is reported as an error (AEAD authentication failure), never
as wrong bytes.

## SteganoEmbedder and handlers

`SteganoEmbedder` (in `stegano::embedding`, re-exported at the root) is the
async in-memory entry point. It picks the handler for `carrier.format`:

| Method | |
|--------|-|
| `SteganoEmbedder::new()` | |
| `async embed(&mut CarrierData, &[u8], &EmbedParams) -> Result<EmbedResult>` | Rewrites `carrier.raw_data`. Runs validation if `enable_validation`. |
| `async extract(&CarrierData, &EmbedParams) -> Result<Vec<u8>>` | Strategy is read from the carrier. |
| `estimate_capacity(&CarrierData) -> Result<CapacityInfo>` | |
| `verify_integrity(&CarrierData) -> Result<bool>` | Structural presence check; no password or key derivation. |

The futures are `Send`; any Tokio runtime works.

### `DataHandler` trait and `HandlerRegistry`

Every format handler in `stegano::handlers` implements:

```rust,ignore
// Excerpt of stegano::handlers::DataHandler (an #[async_trait] trait).
pub trait DataHandler: Send + Sync {
    fn detect_format(&self, data: &[u8]) -> Option<DataFormat>;
    fn estimate_capacity(&self, carrier: &CarrierData) -> Result<CapacityInfo>;
    async fn embed(&self, carrier: &mut CarrierData, payload: &[u8], params: &EmbedParams)
        -> Result<EmbedResult>;
    async fn extract(&self, carrier: &CarrierData, params: &EmbedParams) -> Result<Vec<u8>>;
    fn verify_integrity(&self, carrier: &CarrierData) -> Result<bool>;
}
```

| Module | Handler | Formats |
|--------|---------|---------|
| `handlers::genomics` | `GenomicsHandler` | FASTA, FASTQ |
| `handlers::vcf` | `VcfHandler` | VCF |
| `handlers::sam_bam` | `SamBamHandler` | SAM, BAM |
| `handlers::imagery` | `ImageryHandler` | PNG, TIFF, MICROSCOPY |
| `handlers::dicom` | `DicomHandler` | DICOM |
| `handlers::signals` | `SignalHandler` | WAV, BinarySignal, CsvTimeseries |
| `handlers::fits` | `FitsHandler` | FITS |
| `handlers::generic` | `GenericHandler` | BINARY, TEXT |

`HandlerRegistry::new()` holds one of each; `get_handler(&format)`,
`get_handler_by_data(&bytes)` and `get_handlers()` look them up.

```rust
use stegano::handlers::HandlerRegistry;
use stegano::{CarrierData, DataFormat};

let registry = HandlerRegistry::new();
let handler = registry.get_handler(&DataFormat::BINARY).expect("built in");
let carrier = CarrierData::new(DataFormat::BINARY, vec![0u8; 4096]);
let capacity = handler.estimate_capacity(&carrier)?;
assert!(capacity.recommended_bits > 0);
assert!(!handler.verify_integrity(&carrier)?);
# Ok::<(), stegano::SteganoError>(())
```

## Streaming (`stegano::stream`)

Synchronous, bounded-memory embedding and extraction for FASTQ, VCF,
SAM/BAM, CSV, text, FITS and WAV (`timedomain`). The strategies that stream are listed in
[FORMATS.md](FORMATS.md#streaming).

| Function | |
|----------|-|
| `is_streamable(&DataFormat, &EmbedParams) -> bool` | Whether `embed_stream` accepts this format and strategy. |
| `embed_stream(format, reader: impl BufRead, writer: impl Write, payload, &params) -> Result<EmbedResult>` | Streams the carrier from `reader` to `writer`. |
| `extract_stream(format, reader: impl BufRead, &params) -> Result<Vec<u8>>` | Stops reading once the payload frame is complete. |
| `embed_file(format, in_path, out_path, payload, &params) -> Result<EmbedResult>` | Atomic: temporary file plus rename on success. `in_path` may equal `out_path`. |
| `extract_file(format, path, &params) -> Result<Vec<u8>>` | |

The payload is encoded once in memory; only the carrier is streamed. A single
pass cannot know the capacity in advance, so `embed_stream` reports
`InsufficientCapacity` (or a malformed-carrier error) only at the end, after
writing the whole carrier. On any error, discard what was written.
`embed_file` does that for you. An unsupported format or strategy returns
`InvalidParameter` before anything is read.

```rust
use std::io::Cursor;
use stegano::stream::{embed_stream, extract_stream};
use stegano::{DataFormat, EmbedParams};

let mut fastq = Vec::new();
for i in 0..400 {
    fastq.extend_from_slice(format!("@r{i}\n{}\n+\n{}\n", "ACGT".repeat(25), "I".repeat(100)).as_bytes());
}

let params = EmbedParams::new(Vec::new()); // no password, EC Medium
let mut out = Vec::new();
embed_stream(DataFormat::FASTQ, Cursor::new(&fastq), &mut out, b"secret", &params)?;
assert_eq!(out.len(), fastq.len());
let recovered = extract_stream(DataFormat::FASTQ, Cursor::new(&out), &params)?;
assert_eq!(recovered, b"secret");
# Ok::<(), stegano::SteganoError>(())
```

## Carrier construction and defaults (`stegano::carrier`)

Shared by the CLI and every binding, so all front ends detect formats and
apply defaults the same way.

| Item | |
|------|-|
| `build_carrier(path, bytes) -> CarrierData` | Extension first, then content. Fills `dimensions` and `sampling_rate`. |
| `carrier_from_bytes(format, bytes)` / `carrier_from_content(bytes)` | Known format / content-only detection. |
| `detect_format(path, bytes)`, `detect_format_from_extension(path)`, `detect_format_from_magic(bytes)`, `detect_format_from_content(bytes)` | |
| `detect_image_dimensions`, `detect_png_dimensions`, `detect_tiff_dimensions`, `detect_wav_sample_rate` | Header parsers. |
| `default_params(key) -> EmbedParams` | Front-end defaults: stealth `High`, EC **`None`**, AES-256-GCM. |
| `DEFAULT_STEALTH_LEVEL`, `DEFAULT_EC_LEVEL`, `DEFAULT_CIPHER_KIND` (+ `_NAME` strings) | |
| `params_from_names(password, stealth, ec_level, cipher)` | As used by Python/WASM. |
| `parse_stealth_level`, `parse_ec_level`, `parse_cipher` | Case-insensitive name parsers. |
| `strategy_key(&DataFormat) -> Option<&str>` | The `format_specific` key for a format's strategy (`None` for CSV, text, binary). |

## Lower-level modules

- `stegano::codec`: `encode_payload(payload, &params)` and
  `decode_payload(bytes, &params)` implement [WIRE_FORMAT.md](WIRE_FORMAT.md).
  Every handler stores exactly these bytes inside its own frame.
- `stegano::crypto`: `derive_key` (Argon2id), `aead_seal`/`aead_open`,
  `KdfParams`, and the older `CryptoLayer`/`SecurePayload` wrapper. The
  handlers go through `codec`, not `CryptoLayer`.
- `stegano::error_correction`: `ecc_len_for(ECLevel)`, `RS_BLOCK = 255`.
- `stegano::capacity::CapacityEstimator`, `stegano::integrity::IntegrityChecker`:
  small helpers (LSB/frequency capacity arithmetic; BLAKE3, CRC32 and XOR
  checksums). The handlers do not depend on them.
- `stegano::cli`: the argument parser and command handlers behind the
  `stegano` binary.
- `stegano::ffi` (non-wasm targets): the C ABI described under
  [Bindings](#bindings).

## Validation (`stegano::validation`)

Optional statistical checks: classical tests (chi-square, Kolmogorov-Smirnov,
runs, Anderson-Darling, Ljung-Box) and LSB steganalysis (Westfeld-Pfitzmann
pairs-of-values, Dumitrescu-Wu-Wang sample pairs analysis), plus ROC/AUC and
k-fold helpers. Entry points are `StatisticalValidationSuite::new()`,
`comprehensive_validation(original, stego, &format)` and
`advanced_steganalysis()`. With `EmbedParams::with_validation()`,
`SteganoEmbedder::embed` attaches a `ValidationReport`. See
[VALIDATION_GUIDE.md](VALIDATION_GUIDE.md) for what the scores mean.

## Cargo features

| Feature | Adds |
|---------|------|
| (default) | Everything above, the CLI and the C ABI. |
| `python` | PyO3 extension module (`src/python.rs`), built with maturin. |
| `wasm` | wasm-bindgen exports (`src/wasm.rs`). |
| `benchmarks` | Criterion benchmarks (`cargo bench --features benchmarks`). |
| `experimental` | The `gpu`, `ml` and `monitoring` modules. Unsupported, not used by the embedding path, and may change or disappear in any release. |
| `gpu`, `metal`, `cuda` | Backends for the experimental `gpu` module (imply `experimental`). |

## Bindings

All bindings use the shared defaults from `stegano::carrier` (stealth `high`,
EC `none`, AES-256-GCM), so a file embedded through one front end can be
extracted through any other.

### C

Header: [`include/stegano.h`](../include/stegano.h) (generated by cbindgen;
CI checks it is current). Build `libstegano` with `cargo build --release`.
Functions: `stegano_init`, `stegano_version`, `stegano_detect_format`,
`stegano_detect_file_format`, `stegano_estimate_capacity`, `stegano_embed`,
`stegano_extract`, `stegano_embed_v2` / `stegano_extract_v2` (take a
`format_specific` JSON string), `stegano_embed_file` / `stegano_extract_file`
(path-based, streaming when possible, atomic output), `stegano_verify_integrity`
and `stegano_free`. Errors are `SteganoErrorCode` values; a Rust panic is
caught and reported as `SteganoErrorCode_Panic`. Formats are `CDataFormat`
(`CDataFormat_Fasta = 0` ... `CDataFormat_Fits = 14`).

### Go

Module `github.com/scttfrdmn/stegano/go` (cgo over the C ABI). Functions
`Embed`, `Extract`, `EmbedFile`, `ExtractFile`, `EstimateCapacity`,
`DetectFormat`, `Version`; options `WithPassword`, `WithFormat`,
`WithStrategy`, `WithCipher`, `WithStealth`, `WithECLevel`,
`WithFormatSpecific`. Setup and examples: [go/README.md](../go/README.md).

### Python

Build with maturin (`pyproject.toml` enables the `python` feature); the module
is `stegano`. Functions, from `src/python.rs`:

| Function | Returns |
|----------|---------|
| `embed(carrier, payload, password="", stealth="high", ec_level="none", cipher="aes256gcm", format_specific=None)` | `bytes` |
| `extract(carrier, password="", ec_level="none", cipher="aes256gcm", format_specific=None)` | `bytes` |
| `embed_file(in_path, out_path, payload, password=None, stealth="high", ec_level="none", cipher="aes256gcm", format_specific=None)` | `dict` with `bytes_embedded`, `streamed` |
| `extract_file(path, password=None, ec_level="none", cipher="aes256gcm", format_specific=None)` | `bytes` |
| `estimate_capacity(carrier)` | `dict` |
| `version()` | `str` |

The format is detected from the bytes (or, for the file functions, the path).
`format_specific` is a `dict` such as `{"sam_strategy": "auxiliary"}`.

### WebAssembly / npm

Package `stegano-wasm`, built from `src/wasm.rs` with the `wasm` feature.
Exports `stegano_embed` and `stegano_extract` (return Promises),
`stegano_estimate_capacity` and `stegano_version`. Usage:
[npm/README.md](../npm/README.md).
