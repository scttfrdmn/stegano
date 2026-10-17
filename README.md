# Stegano

[![Version](https://img.shields.io/badge/version-0.8.0-blue.svg)](CHANGELOG.md)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Stegano is a Rust library and CLI for embedding payloads in scientific data
files (genomics, imaging, signals, astronomy), with C, Go, Python and
WebAssembly bindings. Each format handler writes files that the standard tools
for that format still accept.

**Encryption happens only when you supply a password.** With a password, the
payload is encrypted with AES-256-GCM or ChaCha20-Poly1305 under an
Argon2id-derived key. Without one, the payload is stored in plaintext inside
the carrier. This applies to every interface: the CLI, the Rust API, C/FFI,
Go, Python and WASM. Reed-Solomon error correction is optional in both cases.

## Status

Stegano is pre-1.0. Read this before relying on it:

- **Every format handler writes valid files (since v0.6.0).** CI checks each
  handler's output with the standard tool for that format: Pillow, `wave`,
  astropy, Biopython, bcftools, samtools and pydicom. If a standard tool
  rejects an output file, that is a bug.
- **The wire format changed in v0.6.0** ([#38](https://github.com/scttfrdmn/stegano/issues/38),
  [docs/WIRE_FORMAT.md](docs/WIRE_FORMAT.md)). Data embedded with 0.5.x
  cannot be extracted with 0.6.x or later.
- **The GPU, ML and monitoring modules are experimental.** The embedding path
  does not use them ([#40](https://github.com/scttfrdmn/stegano/issues/40)),
  and the `--gpu` flag and `use_gpu` parameter have no effect on embedding.

## Limitations

In short (the full list is in
[SECURITY.md](SECURITY.md#threat-model-and-limitations)):

- **Payloads are detectable.** Bits are embedded sequentially and each handler
  writes a plaintext marker (such as `IMSJ`, `GNSJ`, `SBM` or `STEG=`), so
  steganalysis, or simply looking for the marker, reveals that a payload is
  present. Encryption protects the payload's content, not the fact that it
  exists. Stealth hardening is tracked in
  [#50](https://github.com/scttfrdmn/stegano/issues/50) but is not scheduled.
- **No password means no confidentiality and no authentication.** Anyone
  can read a password-less payload, and there is no cryptographic check that
  it was not modified.
- **Error correction repairs byte errors, nothing more.** Re-encoding the
  carrier (lossy compression, resampling, conversion to another format)
  destroys the payload.

## Supported formats

| Domain    | Formats                     | CLI extensions                    |
|-----------|-----------------------------|-----------------------------------|
| Genomics  | FASTA, FASTQ, VCF, SAM, BAM | `.fa .fasta .fq .fastq .vcf .sam .bam` |
| Imaging   | PNG, TIFF, DICOM            | `.png .tif .tiff .dcm .dicom`     |
| Astronomy | FITS                        | `.fits .fit .fts`                 |
| Signals   | WAV, raw 16-bit samples     | `.wav`                            |
| Tabular   | CSV                         | `.csv`                            |
| Text      | Plain UTF-8 text            | `.txt`                            |
| Generic   | Binary                      | any other                         |

The CLI picks the format from the file extension and falls back to magic
bytes. `MICROSCOPY` is handled like PNG/TIFF. Raw binary signals
(`BinarySignal`) are treated as headerless 16-bit samples. CSV and text are
parsed as text and stay valid. CSV numeric cells change by at most one unit
in their last printed digit. Text gets trailing whitespace only, which is
visible in editors that show whitespace.

### Strategies

Each handler reads its strategy from one `format_specific` key. The CLI
`--strategy` option, Go's `WithStrategy`, and the JSON options of the C `*_v2`
and `*_file` functions set the same keys. "Streamable" means that the file
APIs (CLI `embed`/`extract`, `stegano::stream`, `stegano_embed_file`, Go
`EmbedFile`, Python `embed_file`) process the file in constant memory.

| Format | Key | Strategies (default first) | Streamable |
|--------|-----|----------------------------|------------|
| FASTQ | `genomics_strategy` | from stealth level, or `conservative`, `balanced`, `aggressive` (quality-score LSBs) | yes |
| FASTA | `genomics_strategy` | `header` (hex suffix on header lines; visible, not covert) | no |
| VCF | `vcf_strategy` | `info`, `format` (alias `sample`), `id`, `quality`, `multi` | `info`, `id`, `quality` |
| SAM, BAM | `sam_strategy` | `optional`, `auxiliary`, `quality`, `comment`, `multi`; BAM output also takes `bam_compression` = `bgzf` (default) or `none` | `auxiliary`, `quality` |
| DICOM | `dicom_strategy` | `private`, `metadata`, `sequence`, `pixel_lsb`, `multi` | no |
| PNG, TIFF | `imagery_strategy` | from stealth level, or `lsb`, `adaptive`, `edge`, `dct` | no |
| WAV | `signal_strategy` | from stealth level, or `timedomain`, `frequency`, `phase`, `spread`, `echo` | `timedomain` (or `--stealth low`) |
| FITS | `fits_strategy` | `pixel_lsb` (integer data), `float_mantissa` (opt-in, float data) | yes |
| CSV | `csv_strategy` | `digit` (parity of the last digit of each unquoted numeric cell with at least `csv_min_significant_digits` significant digits, default 2; header and text cells untouched; `csv_delimiter` sets the delimiter) | yes |
| Text | `text_strategy` | `whitespace` (k trailing spaces/tabs per line, k = 1..64 chosen automatically or set with `text_bits_per_line`) | with a fixed `text_bits_per_line` |
| Binary | none | LSB of every byte | no |

Streaming extraction reads any streamable format, whatever strategy the
payload was embedded with (WAV: `timedomain` payloads only; the CLI falls
back to the in-memory path for the others). Without a strategy, PNG/TIFF, WAV and FASTQ choose
one from the stealth level (`--stealth`, default `high`). For float FITS
data, pass `--strategy float_mantissa` (or `fits_strategy` through the library
or a binding). For channel layouts and capacity formulas, see [docs/FORMATS.md](docs/FORMATS.md).

## Building

```bash
git clone https://github.com/scttfrdmn/stegano.git
cd stegano
cargo build --release        # binary at target/release/stegano
cargo test --lib
```

This crate is not on crates.io. The `stegano` crate there is an unrelated
project. To use this library, add a git dependency:

```toml
[dependencies]
stegano = { git = "https://github.com/scttfrdmn/stegano" }
```

Optional Cargo features (none are on by default): `python`, `wasm`,
`benchmarks`, and `experimental`. `experimental` enables the unsupported
`gpu`, `ml` and `monitoring` modules. The embedding pipeline does not use
them, and their APIs may change or be removed. `gpu` (wgpu), `metal` and
`cuda` imply `experimental`.

## CLI usage

```bash
# Embed a payload (password read from the first line of a file)
stegano embed -i carrier.png -p secret.txt -o out.png --password-file ~/.stegano-pw

# Extract it (password from the environment)
STEGANO_PASSWORD="$(cat ~/.stegano-pw)" stegano extract -i out.png -o recovered.txt

# Or type the password at a prompt (not echoed; embed asks twice)
stegano extract -i out.png -o recovered.txt --password-prompt

# Estimate capacity
stegano analyze -i carrier.png

# Check whether a file contains an embedded payload
stegano detect -i out.png
```

### Passwords

If no password source is given, the payload is **not encrypted**. The
password is taken from the first of these that is present:

1. `--password-file PATH`: the first line of the file.
2. `--password VALUE`: works, but the password is visible to other users in
   `ps` and stays in your shell history, so the CLI prints a warning.
3. The `STEGANO_PASSWORD` environment variable (an empty value counts as
   unset).
4. `--password-prompt`: reads the password from the terminal.

`--password` cannot be combined with `--password-file` or
`--password-prompt`, and `--password-file` cannot be combined with
`--password-prompt`. If `STEGANO_PASSWORD` is set, it is used and
`--password-prompt` does not prompt.

### Options

Useful `embed` options:

- `--stealth <maximum|high|medium|low>` (default `high`)
- `--ec-level <none|low|medium|high|maximum>` (default `none`)
- `--cipher <aes256gcm|chacha20>` (default `aes256gcm`; detected automatically on extract)
- `--strategy <name>`: a format-specific strategy (see [Strategies](#strategies)):
  - VCF: `info|quality|format|id|multi`
  - SAM/BAM: `optional|comment|quality|auxiliary|multi`
  - DICOM: `private|metadata|sequence|pixel_lsb|multi`
  - PNG/TIFF: `lsb|adaptive|edge|dct`
  - WAV: `timedomain|frequency|phase|spread|echo`
  - FASTA/FASTQ: `header` / `conservative|balanced|aggressive`
- `--validate`: runs the statistical validation suite on the result.

`extract` accepts the same `--strategy` values. If you passed a strategy to
`embed`, pass the same one to `extract`. Without a password, if you embedded
with an `--ec-level` other than `none`, give `extract` any non-`none`
`--ec-level` too; the actual level is read from the payload. Run
`stegano <command> --help` for the full list of options.

`detect` exit status: `0` = a payload was found, `1` = no payload found,
`3` = the file could not be inspected (I/O error, unsupported format). Exit
status `2` is reserved for usage errors.

### Streaming

`embed` and `extract` stream large carriers automatically. When the format
(taken from the extension, or from the magic bytes of the first few KiB) and
the strategy are streamable (see the table above), the file is processed in
constant memory and is never read whole. The output goes to a temporary file
that is renamed into place only on success. `extract` streams any streamable
format, whatever strategy the payload was embedded with (except WAV, where
only `timedomain` payloads stream and the others fall back to the in-memory
path), and stops as soon as the payload is complete. Everything else, and `--validate`, uses the
in-memory path. `--no-stream` forces the in-memory path. Both paths produce
interchangeable files, and the command prints which one it used.

## Library quick start

```rust,no_run
use stegano::*;

#[tokio::main]
async fn main() -> Result<()> {
    let mut carrier = CarrierData::new(DataFormat::PNG, std::fs::read("carrier.png")?);

    // An empty password (`EmbedParams::new(Vec::new())`) means no encryption.
    let password = std::env::var("STEGANO_PASSWORD").expect("set STEGANO_PASSWORD");
    let params = EmbedParams::new(password.into_bytes())
        .with_stealth_level(StealthLevel::High)
        .with_ec_level(ECLevel::Medium);

    let embedder = SteganoEmbedder::new();
    println!("capacity: {} bits", embedder.estimate_capacity(&carrier)?.recommended_bits);

    embedder.embed(&mut carrier, b"secret research data", &params).await?;
    std::fs::write("out.png", &carrier.raw_data)?;

    let recovered = embedder.extract(&carrier, &params).await?;
    assert_eq!(recovered, b"secret research data");
    Ok(())
}
```

`EmbedParams::new` defaults to `ECLevel::Medium`. The CLI and all the
bindings default to `none` (`stegano::carrier::DEFAULT_EC_LEVEL`). If you mix
them, set the level explicitly. With a password, extraction detects the level
on its own. For large files, `stegano::stream::{embed_file, extract_file}`
work on paths and stream when they can.

The registered examples build with `cargo build --examples`. Run one with
`cargo run --example <name>`, for example `cargo run --example basic_demo`.
They are in [`examples/`](examples/).

## Bindings

### C / C++

`cargo build --release` produces a `cdylib` (`libstegano.so`,
`libstegano.dylib` or `stegano.dll`) in `target/release/`. The header is
[`include/stegano.h`](include/stegano.h), generated by cbindgen from
[`src/ffi.rs`](src/ffi.rs). It exports:

| Function | Purpose |
|----------|---------|
| `stegano_init` | Initialise the library (optional). |
| `stegano_version` | Crate version as a static string (do not free). |
| `stegano_detect_format` | Detect the format of a buffer. |
| `stegano_detect_file_format` | Detect the format of a file from its name and first 4 KiB. |
| `stegano_estimate_capacity` | Fill a `CCapacityInfo` for a buffer. |
| `stegano_embed` | Embed in place, with defaults (fails with `OutputSizeChanged` if the output length would differ). |
| `stegano_extract` | Extract with defaults. |
| `stegano_embed_v2` | Embed into a new buffer, with JSON options. |
| `stegano_extract_v2` | Extract with JSON options. |
| `stegano_embed_file` | Embed file to file (streams when possible; atomic output). |
| `stegano_extract_file` | Extract from a file (streams when possible). |
| `stegano_verify_integrity` | Check whether a buffer contains a stegano payload. |
| `stegano_free` | Free a buffer returned through a `uint8_t **` out-parameter. |

A null or empty password means no encryption. The JSON options take
`stealth`, `ec_level`, `cipher` and any of the strategy keys above.

### Go

A CGO wrapper over the C ABI (`github.com/scttfrdmn/stegano/go`) with `Embed`,
`Extract`, `EmbedFile`, `ExtractFile`, `EstimateCapacity`, `DetectFormat` and
`Version`. See [go/README.md](go/README.md).

```bash
cargo build --release
cd go && CGO_LDFLAGS="-L$(pwd)/../target/release" \
  DYLD_LIBRARY_PATH="$(pwd)/../target/release" \
  LD_LIBRARY_PATH="$(pwd)/../target/release" go test ./...
```

### Python

Built with [maturin](https://www.maturin.rs/). `pyproject.toml` enables the
`python` feature.

```bash
pip install maturin
maturin develop --release
python -c "import stegano; print(stegano.version())"
```

Module functions:

- `embed(carrier, payload, password="", stealth="high", ec_level="none", cipher="aes256gcm", format_specific=None) -> bytes`
- `extract(carrier, password="", ec_level="none", cipher="aes256gcm", format_specific=None) -> bytes`
- `embed_file(in_path, out_path, payload, password=None, ...) -> dict` (keys `bytes_embedded`, `streamed`)
- `extract_file(path, password=None, ...) -> bytes`
- `estimate_capacity(carrier) -> dict` (keys `total_bits`, `recommended_bits`, `format`)
- `version() -> str`

`embed` and `extract` detect the format from the bytes. The `*_file`
functions use the file name and the first bytes, stream when the format
allows, and write the output atomically. The `Stegano` class offers the same
`embed`, `extract` and `estimate_capacity` as methods:

```python
import stegano
s = stegano.Stegano()
out = s.embed(carrier, b"secret", password=pw, format_specific={"sam_strategy": "auxiliary"})
assert s.extract(out, password=pw) == b"secret"
```

An empty or `None` password means no encryption. Errors are raised as
`RuntimeError` (`OSError` for I/O errors in the file functions).

### JavaScript / npm

The WebAssembly bindings are published as
[`stegano-wasm`](https://www.npmjs.com/package/stegano-wasm). One package works
in browsers, bundlers and Node.js (ESM and CommonJS) and includes TypeScript
types:

```bash
npm install stegano-wasm
```

```js
import init, { stegano_embed, stegano_extract } from 'stegano-wasm';
await init(); // loads the .wasm (a no-op in Node)
const stego = await stegano_embed(carrier, payload, password, '', '', ''); // '' = default
const out = await stegano_extract(stego, password, '', '');
```

An empty password means no encryption. See [npm/README.md](npm/README.md)
for the full API and the supported formats. To build the package from source
you need [wasm-pack](https://rustwasm.github.io/wasm-pack/):

```bash
./scripts/build-wasm.sh          # assembles pkg/ (web + node builds)
node npm/test/run.mjs            # packs pkg/ and runs the smoke tests
```

## Documentation

- [docs/README.md](docs/README.md): documentation index
- [docs/GETTING_STARTED.md](docs/GETTING_STARTED.md): installation and first steps
- [docs/API_REFERENCE.md](docs/API_REFERENCE.md): library API
- [docs/FORMATS.md](docs/FORMATS.md): format handlers, strategies and capacity
- [docs/WIRE_FORMAT.md](docs/WIRE_FORMAT.md): payload encoding and cryptography
- [docs/VALIDATION_GUIDE.md](docs/VALIDATION_GUIDE.md): statistical validation and steganalysis
- [SECURITY.md](SECURITY.md): threat model, limitations and vulnerability reporting
- [CONTRIBUTING.md](CONTRIBUTING.md): development workflow
- [CHANGELOG.md](CHANGELOG.md): release history

## License

Licensed under the [Apache License, Version 2.0](LICENSE). See also [NOTICE](NOTICE).

Copyright 2025-2026 Scott Friedman.
