# Stegano

[![Version](https://img.shields.io/badge/version-0.6.0-blue.svg)](CHANGELOG.md)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Stegano is a Rust library and CLI for embedding encrypted payloads in scientific
data files (genomics, imaging, signals, astronomy). Payloads are encrypted with
AES-256-GCM or ChaCha20-Poly1305 and can optionally be protected with
Reed-Solomon error correction before being embedded by a format-specific handler.

## Status

Stegano is pre-1.0 and under active development. Please read this before relying on it:

- **Every format handler writes valid files (v0.6.0).** CI checks each handler's output
  against the standard tool for that format: Pillow, `wave`, astropy, Biopython,
  bcftools, samtools and pydicom. If a standard tool rejects an output file, that is a bug.
- **Payloads are detectable.** Embedding is still sequential with plaintext handler
  markers, so steganalysis such as Sample Pairs Analysis or pairs-of-values (included in
  `validation`) can detect it. Keyed embedding positions are planned
  ([#50](https://github.com/scttfrdmn/stegano/issues/50)). The encryption protects
  confidentiality; it does not hide that a payload exists.
- **The wire format changed in v0.6.0** ([#38](https://github.com/scttfrdmn/stegano/issues/38),
  `docs/WIRE_FORMAT.md`). Data embedded with 0.5.x cannot be extracted with 0.6.x.
- **The GPU, ML and monitoring modules are experimental** and are not used by
  the embedding path ([#40](https://github.com/scttfrdmn/stegano/issues/40)).
  The `--gpu` flag and `use_gpu` parameter do not currently speed up embedding.

## Supported formats

| Domain    | Formats                     | CLI extensions                    |
|-----------|-----------------------------|-----------------------------------|
| Genomics  | FASTA, FASTQ, VCF, SAM, BAM | `.fa .fasta .fq .fastq .vcf .sam .bam` |
| Imaging   | PNG, TIFF, DICOM            | `.png .tif .tiff .dcm .dicom`     |
| Astronomy | FITS                        | `.fits .fit .fts`                 |
| Signals   | WAV, CSV time series        | `.wav .csv`                       |
| Generic   | Binary, text                | `.txt`                            |

The CLI picks the format from the file extension first and falls back to magic
bytes. See [docs/SCIENTIFIC_FORMATS_GUIDE.md](docs/SCIENTIFIC_FORMATS_GUIDE.md)
for per-format strategies.

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
`gpu`, `ml` and `monitoring` modules, which the embedding pipeline does not
use and whose APIs may change or be removed. `gpu` (wgpu), `metal` and `cuda`
imply `experimental`.

## CLI usage

```bash
# Embed a payload
stegano embed -i carrier.png -p secret.txt -o out.png --password 'passphrase'

# Extract it
stegano extract -i out.png -o recovered.txt --password 'passphrase'

# Estimate capacity
stegano analyze -i carrier.png

# Check whether a file contains an embedded payload (exit 0 = found, 1 = not found)
stegano detect -i out.png
```

Useful `embed` options:

- `--stealth <maximum|high|medium|low>` (default `high`)
- `--ec-level <none|low|medium|high|maximum>` (default `none`)
- `--cipher <aes256gcm|chacha20>` (default `aes256gcm`; detected automatically on extract)
- `--strategy <name>` for a format-specific strategy:
  - VCF: `info|quality|sample|id|multi`
  - SAM/BAM: `optional|comment|quality|auxiliary|multi`
  - DICOM: `private|metadata|sequence|pixel_lsb|multi`
  - PNG/TIFF: `lsb|adaptive|edge|dct`
  - WAV: `timedomain|frequency|phase|spread|echo`
- `--validate` runs the statistical validation suite on the result.

`extract` accepts the same `--strategy` values. If you passed a strategy to
`embed`, you must pass the same one to `extract`. Run `stegano <command> --help`
for the full list of options.

## Library quick start

```rust,no_run
use stegano::*;

#[tokio::main]
async fn main() -> Result<()> {
    let mut carrier = CarrierData::new(DataFormat::PNG, std::fs::read("carrier.png")?);

    let params = EmbedParams::new(b"passphrase".to_vec())
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

More examples are in [`examples/`](examples/). Some of them do not compile
against the current API yet ([#36](https://github.com/scttfrdmn/stegano/issues/36)).

## Bindings

**C / C++.** `cargo build --release` produces a `cdylib` (`libstegano.so`,
`libstegano.dylib` or `stegano.dll`) in `target/release/`. The exported
functions are `stegano_init`, `stegano_detect_format`, `stegano_estimate_capacity`,
`stegano_embed`, `stegano_extract`, `stegano_embed_v2`, `stegano_extract_v2`,
`stegano_verify_integrity`, `stegano_free` and `stegano_version`. See
[`src/ffi.rs`](src/ffi.rs) for signatures.

**Go.** This is a CGO wrapper over the C ABI. See [go/README.md](go/README.md).

```bash
cargo build --release
CGO_LDFLAGS="-L$(pwd)/target/release" go test ./go/...
```

**Python.** Built with [maturin](https://www.maturin.rs/) using the `python` feature:

```bash
pip install maturin
maturin develop --release
python -c "import stegano; print(stegano.version())"
```

The module exposes `embed`, `extract`, `estimate_capacity` and `version`.

**JavaScript / npm.** The WebAssembly bindings are published as
[`stegano-wasm`](https://www.npmjs.com/package/stegano-wasm). One package works
in browsers, bundlers and Node.js (ESM and CommonJS) and includes TypeScript types:

```bash
npm install stegano-wasm
```

```js
import init, { stegano_embed, stegano_extract } from 'stegano-wasm';
await init(); // loads the .wasm (a no-op in Node)
const stego = await stegano_embed(carrier, payload, 'password', '', '', ''); // '' = default
const out = await stegano_extract(stego, 'password', '', '');
```

See [npm/README.md](npm/README.md) for the full API and the supported formats.
To build the package from source you need [wasm-pack](https://rustwasm.github.io/wasm-pack/):

```bash
./scripts/build-wasm.sh          # assembles pkg/ (web + node builds)
node npm/test/run.mjs            # packs pkg/ and runs the smoke tests
```

## Documentation

- [docs/GETTING_STARTED.md](docs/GETTING_STARTED.md): installation and first steps
- [docs/API_REFERENCE.md](docs/API_REFERENCE.md): library API
- [docs/SCIENTIFIC_FORMATS_GUIDE.md](docs/SCIENTIFIC_FORMATS_GUIDE.md): format handlers and strategies
- [docs/VALIDATION_GUIDE.md](docs/VALIDATION_GUIDE.md): statistical validation suite
- [CHANGELOG.md](CHANGELOG.md): release history
- [SECURITY.md](SECURITY.md): reporting vulnerabilities
- [CONTRIBUTING.md](CONTRIBUTING.md): development workflow

Older planning, roadmap and session documents are kept for reference in
[docs/archive/](docs/archive/). They are historical and not maintained.

## License

Licensed under the [Apache License, Version 2.0](LICENSE). See also [NOTICE](NOTICE).

Copyright 2025-2026 Scott Friedman.
