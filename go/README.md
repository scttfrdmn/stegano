# stegano Go bindings

Idiomatic Go package wrapping the [stegano](https://github.com/scttfrdmn/stegano) steganography library via CGO.

## Requirements

- Go 1.22+
- CGO enabled (default)
- The stegano Rust library compiled to `target/release/`

## Build

```sh
# 1. Compile the Rust library
cd /path/to/stegano
cargo build --release

# 2. Export linker paths
export CGO_LDFLAGS="-L$(pwd)/target/release"
export DYLD_LIBRARY_PATH="$(pwd)/target/release"   # macOS
export LD_LIBRARY_PATH="$(pwd)/target/release"      # Linux

# 3. Test
cd go && go test ./...
```

## Usage

```go
import stegano "github.com/scttfrdmn/stegano/go"

// Embed a secret payload. Format is auto-detected from magic bytes.
modified, err := stegano.Embed(carrier, []byte("secret message"),
    stegano.WithPassword("encryption-key"))

// Extract it back.
payload, err := stegano.Extract(modified,
    stegano.WithPassword("encryption-key"))

// Inspect capacity before embedding.
info, err := stegano.EstimateCapacity(carrier)
fmt.Printf("Format: %s  Total: %d bits  Recommended: %d bits\n",
    info.Format, info.TotalBits, info.RecommendedBits)

// Detect format only.
fmt, err := stegano.DetectFormat(carrier)

// Library version.
fmt.Println(stegano.Version()) // crate version, e.g. "0.6.0"
```

## Supported formats

| Constant | Format |
|----------|--------|
| `FormatFASTA` | FASTA genomic sequences |
| `FormatFASTQ` | FASTQ with quality scores |
| `FormatVCF` | Variant Call Format |
| `FormatSAM` | SAM alignment records |
| `FormatBAM` | BAM (binary SAM) |
| `FormatPNG` | PNG images |
| `FormatTIFF` | TIFF images |
| `FormatDICOM` | DICOM medical images |
| `FormatWAV` | WAV audio |
| `FormatFITS` | FITS astronomical images |
| `FormatBinary` | Generic binary (fallback) |

## Options

| Option | Description |
|--------|-------------|
| `WithPassword(string)` | Encryption password. Empty = no encryption. |
| `WithFormat(Format)` | Override auto-detected format. |
| `WithStrategy(string)` | Format-specific strategy, sent under the key the handler reads. PNG/TIFF (`imagery_strategy`): `lsb\|adaptive\|edge\|dct`. VCF (`vcf_strategy`): `info\|quality\|format\|id\|multi`. SAM/BAM (`sam_strategy`): `optional\|comment\|quality\|auxiliary\|multi`. DICOM (`dicom_strategy`): `private\|metadata\|sequence\|pixel_lsb\|multi`. WAV (`signal_strategy`): `timedomain\|frequency\|phase\|spread\|echo`. FASTA/FASTQ: `genomics_strategy`. Other formats return `ErrInvalidParameter`. Pass the same strategy to `Extract`. |
| `WithCipher(string)` | Encryption cipher: `aes256gcm` (default) or `chacha20`. Requires `WithPassword`; auto-detected on `Extract`. |
| `WithStealth(string)` | `low\|medium\|high\|maximum` (default `high`). |
| `WithECLevel(string)` | Reed-Solomon level `none\|low\|medium\|high\|maximum` (default `none`). Without a password, `Extract` needs a non-`none` level too if one was used for `Embed`. |

## Error handling

All functions return a typed `*stegano.Error` on failure:

```go
modified, err := stegano.Embed(carrier, payload)
if err != nil {
    var serr *stegano.Error
    if errors.As(err, &serr) && serr.Code == stegano.ErrInsufficientCapacity {
        // carrier too small for payload
    }
}
```

## Notes

- The C declarations come from the cbindgen-generated `include/stegano.h`
  (via `#cgo CFLAGS: -I${SRCDIR}/../include`), so build from a checkout of
  the repository.
- Defaults are shared with the CLI, C FFI, Python and WASM bindings:
  `high` stealth, `none` error correction, AES-256-GCM. (Before v0.6 the Go
  binding used `low` stealth and `medium` EC; to read old password-less
  carriers pass `WithECLevel("medium")`.)
- `Embed` returns the complete embedded carrier, which can be longer than the
  input (SAM, BAM, VCF, DICOM). The input slice is never modified.
- Library-allocated buffers are freed automatically.
- Thread-safe: the underlying Rust FFI uses one shared Tokio runtime.

## License

Apache-2.0. See [`LICENSE`](../LICENSE) in the repository root.
