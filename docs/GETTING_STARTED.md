# Getting started

This guide covers installing stegano, using the `stegano` command-line tool,
and calling the Rust library. For the full API see
[API_REFERENCE.md](API_REFERENCE.md); for what each format handler changes in
a file see [FORMATS.md](FORMATS.md).

Before you use stegano for anything sensitive, read
[the threat model and limitations](../SECURITY.md#threat-model-and-limitations).
In short: every handler writes a fixed marker, stegano's own `detect` command
finds payloads without the password, and several strategies are plainly
visible to anyone who opens the file. Encryption protects the payload's
contents. It does not hide that a payload is there.

## Install

stegano is not published on crates.io. The `stegano` crate on crates.io is an
unrelated project, so do not add `stegano = "x.y"` to your manifest. Use the
git repository instead.

Requirements: a stable Rust toolchain (edition 2021).

### Library

```toml
[dependencies]
stegano = { git = "https://github.com/scttfrdmn/stegano", tag = "v0.7.0" }
# Only needed for the async in-memory API (SteganoEmbedder):
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

### Command-line tool

```sh
cargo install --git https://github.com/scttfrdmn/stegano --tag v0.7.0 stegano
```

Or build it from a checkout:

```sh
git clone https://github.com/scttfrdmn/stegano
cd stegano
cargo build --release --bin stegano   # -> target/release/stegano
```

Build release binaries. The Argon2id key derivation (64 MiB, t = 3) is slow in
debug builds of your own crate.

## CLI quick start

The tool has four subcommands: `embed`, `extract`, `analyze` and `detect`.
`stegano <command> --help` prints the full option list.

```sh
# How much fits?
stegano analyze -i reads.fastq

# Embed secret.txt. The password is read from the environment here.
export STEGANO_PASSWORD='correct horse battery staple'
stegano embed -i reads.fastq -p secret.txt -o reads.stego.fastq

# Extract it again
stegano extract -i reads.stego.fastq -o recovered.txt

# Check for a payload (no password needed). Exit status 0 = found,
# 1 = not found, 3 = error.
stegano detect -i reads.stego.fastq
```

The carrier format comes from the file extension (`.fastq`/`.fq`, `.fasta`/`.fa`,
`.vcf`, `.sam`, `.bam`, `.png`, `.tif`/`.tiff`, `.dcm`/`.dicom`, `.wav`,
`.fits`/`.fit`/`.fts`, `.csv`, `.txt`) and otherwise from the file's magic
bytes. Anything unrecognised is treated as generic binary.

### Passwords

Without a password the payload is stored unencrypted. There are four ways to
supply one, in this order of precedence:

| Option | Notes |
|--------|-------|
| `--password-file <PATH>` | First line of the file. |
| `--password <PASSWORD>` | Visible to other users in `ps` and kept in shell history; prints a warning. |
| `STEGANO_PASSWORD` environment variable | |
| `--password-prompt` | Reads from the terminal without echo; `embed` asks twice. |

### `embed` options

| Option | Default | Meaning |
|--------|---------|---------|
| `-i, --input` | required | Carrier file. |
| `-p, --payload` | required | File to hide. |
| `-o, --output` | required | Output carrier. Written to a temporary file and renamed only on success. |
| `--stealth` | `high` | `low`, `medium`, `high` or `maximum`. Picks the default strategy for PNG/TIFF, WAV and FASTQ (see [FORMATS.md](FORMATS.md)); other formats ignore it. |
| `--ec-level` | `none` | Reed-Solomon level: `none`, `low`, `medium`, `high`, `maximum`. |
| `--strategy` | per format | Format-specific strategy. VCF: `info`, `quality`, `format`, `id`, `multi`. SAM/BAM: `optional`, `comment`, `quality`, `auxiliary`, `multi`. DICOM: `private`, `metadata`, `sequence`, `pixel_lsb`, `multi`. PNG/TIFF: `lsb`, `adaptive`, `edge`, `dct`. WAV: `timedomain`, `frequency`, `phase`, `spread`, `echo`. FASTQ: `conservative`, `balanced`, `aggressive`, `quality`. FASTA: `header`. FITS: `pixel_lsb`, `float_mantissa` (required for float data). Ignored (with a warning) for CSV, text and binary. |
| `--cipher` | `aes256gcm` | `aes256gcm` or `chacha20`. Detected automatically on extract. |
| `--validate` | off | Run the statistical checks described in [VALIDATION_GUIDE.md](VALIDATION_GUIDE.md). Forces the in-memory path. |
| `--no-stream` | off | Force the in-memory path. By default FASTQ, VCF (`info`, `id`, `quality`), SAM/BAM (`auxiliary`, `quality`), CSV, text (fixed `text_bits_per_line`), FITS and WAV (`timedomain`) are streamed with bounded memory. |
| `--gpu` | off | Sets `EmbedParams::use_gpu`. The embedding path does not use it. |

`extract` takes `-i`, `-o`, the password options, `--ec-level`, `--strategy`,
`--no-stream` and `--gpu`. The strategy and cipher are read from the embedded
header, so `--strategy` is not needed on extract.

`--ec-level` without a password: whether a password-less payload went through
the error-correction codec is decided by the caller's EC level, so if you
embedded with a level other than `none` and no password, pass any non-`none`
`--ec-level` to `extract` as well. The actual level is read from the payload.
With a password this does not matter.

`analyze -i <file>` prints the capacity estimate and whether hidden data is
present. `detect -i <file>` reports presence only. Both accept `--validate`.

### Examples

```sh
# BAM, packing the payload into as few records as possible (streamed)
stegano embed -i aln.bam -p notes.txt -o aln.stego.bam --strategy auxiliary --password-prompt

# VCF, payload in the ID column (streamed)
stegano embed -i calls.vcf -p notes.txt -o calls.stego.vcf --strategy id --password-file pw.txt

# PNG with ChaCha20-Poly1305 and medium Reed-Solomon
stegano embed -i image.png -p notes.txt -o image.stego.png --cipher chacha20 --ec-level medium --password-prompt

# DICOM, payload in a private data element
stegano embed -i ct.dcm -p notes.txt -o ct.stego.dcm --strategy private --password-prompt
```

## Library quick start

### In-memory API

`SteganoEmbedder` routes a `CarrierData` to the right handler.
`stegano::carrier::build_carrier` detects the format from the path and bytes
and fills in image dimensions and audio sample rate, as the CLI does.

```rust,no_run
use stegano::carrier::build_carrier;
use stegano::{EmbedParams, SteganoEmbedder};

#[tokio::main]
async fn main() -> stegano::Result<()> {
    let embedder = SteganoEmbedder::new();
    let params = EmbedParams::new(b"correct horse battery staple".to_vec());

    let mut carrier = build_carrier("image.png", std::fs::read("image.png")?);
    let capacity = embedder.estimate_capacity(&carrier)?;
    println!("about {} bytes available", capacity.recommended_bytes());

    embedder.embed(&mut carrier, b"secret", &params).await?;
    std::fs::write("image.stego.png", &carrier.raw_data)?;

    let stego = build_carrier("image.stego.png", std::fs::read("image.stego.png")?);
    let recovered = embedder.extract(&stego, &params).await?;
    assert_eq!(recovered, b"secret");
    Ok(())
}
```

`EmbedParams::new` uses stealth `High`, error correction `Medium` and
AES-256-GCM. The CLI and the bindings default to error correction `None`
instead (`stegano::carrier::default_params`). With a password, either side
can extract the other's output.

### Streaming API

For FASTQ, VCF, SAM/BAM, CSV, text, FITS and WAV (`timedomain`),
`stegano::stream` processes the carrier one record, line or block at a time, so memory use does not grow with the file size. It is synchronous
and needs no async runtime.

```rust,no_run
use stegano::stream::{embed_file, extract_file, is_streamable};
use stegano::{DataFormat, EmbedParams};

fn main() -> stegano::Result<()> {
    let params = EmbedParams::new(b"correct horse battery staple".to_vec());
    assert!(is_streamable(&DataFormat::FASTQ, &params));

    // Writes a temporary file next to the output and renames it on success.
    embed_file(DataFormat::FASTQ, "reads.fastq", "reads.stego.fastq", b"secret", &params)?;

    let recovered = extract_file(DataFormat::FASTQ, "reads.stego.fastq", &params)?;
    assert_eq!(recovered, b"secret");
    Ok(())
}
```

Strategies are selected through `format_specific`, using the same names as
the CLI's `--strategy`:

```rust,no_run
use stegano::serde_json::json;
use stegano::stream::embed_file;
use stegano::{DataFormat, EmbedParams};

fn main() -> stegano::Result<()> {
    let mut params = EmbedParams::new(b"correct horse battery staple".to_vec());
    params.format_specific.insert("sam_strategy".into(), json!("auxiliary"));
    embed_file(DataFormat::BAM, "aln.bam", "aln.stego.bam", b"secret", &params)?;
    Ok(())
}
```

The streaming and in-memory APIs produce identical output for the same
encoded payload and read each other's files.

## Next steps

- [FORMATS.md](FORMATS.md): what each strategy modifies, capacity, and which
  formats stream.
- [API_REFERENCE.md](API_REFERENCE.md): types, builders, handlers and bindings.
- [WIRE_FORMAT.md](WIRE_FORMAT.md): the encrypted payload format.
- [VALIDATION_GUIDE.md](VALIDATION_GUIDE.md): the statistical detectors.
- `cargo doc --open`: the full rustdoc.
