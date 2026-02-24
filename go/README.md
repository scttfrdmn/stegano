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
fmt.Println(stegano.Version()) // "0.5.1"
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
| `WithPassword(string)` | AES-256-GCM encryption password. Empty = no encryption. |
| `WithFormat(Format)` | Override auto-detected format. |

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

- The FFI uses `StealthLevel::Low` and AES-256-GCM by default.
- Carrier bytes are never modified in place; `Embed` always returns a new slice.
- `stegano_free` is called automatically via `defer` after `Extract`.
- Thread-safe: the underlying Rust FFI uses a shared Tokio runtime (`OnceLock`).
