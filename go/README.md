# stegano Go bindings

Idiomatic Go package wrapping the [stegano](https://github.com/scttfrdmn/stegano) steganography library via CGO.

## Requirements

- Go 1.22+
- CGO enabled (default)
- The stegano Rust library (`libstegano.so` / `libstegano.dylib`, built with
  `cargo build --release`)

## Using from another module

The C header (`stegano.h`) ships inside this module, so `go get` works
without a checkout of the repository. Only the compiled Rust library has to
be supplied.

```sh
# 1. Build the shared library once, at the same version as the Go module.
git clone https://github.com/scttfrdmn/stegano && cd stegano
git checkout v0.7.0            # match the go/vX.Y.Z tag you depend on
cargo build --release          # -> target/release/libstegano.{so,dylib}
LIBDIR="$(pwd)/target/release"

# 2. In your project, add the module.
cd /path/to/your/project
go get github.com/scttfrdmn/stegano/go@v0.7.0   # or @<commit-sha>

# 3. Point cgo at the library for building, and the loader for running.
export CGO_LDFLAGS="-L$LIBDIR"
export LD_LIBRARY_PATH="$LIBDIR"     # Linux
export DYLD_LIBRARY_PATH="$LIBDIR"   # macOS
go build ./...
```

Module versions are tagged `go/vX.Y.Z` (the module lives in the `go/`
subdirectory), so `@v0.7.0` resolves to the `go/v0.7.0` tag. v0.7.0 is the
first release usable this way (earlier releases read the header from outside
the module). Use a library built from the
same release: the header in the module and the symbols in
`libstegano` must match. Instead of setting `LD_LIBRARY_PATH` /
`DYLD_LIBRARY_PATH` you can install the library somewhere the loader already
searches (for example `/usr/local/lib`) or add an rpath, e.g.
`CGO_LDFLAGS="-L$LIBDIR -Wl,-rpath,$LIBDIR"`.

## Developing in this repository

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

// Embed a payload. The format is auto-detected from magic bytes.
// Without WithPassword the payload is stored in plaintext (not encrypted).
modified, err := stegano.Embed(carrier, []byte("secret message"),
    stegano.WithPassword("encryption-key"))

// Extract it back.
payload, err := stegano.Extract(modified,
    stegano.WithPassword("encryption-key"))

// Inspect capacity before embedding.
info, err := stegano.EstimateCapacity(carrier)
fmt.Printf("Format: %s  Total: %d bits  Recommended: %d bits\n",
    info.Format, info.TotalBits, info.RecommendedBits)

// Work on files directly. No size limit: FASTQ, VCF (info/id/quality) and SAM/BAM
// (auxiliary/quality) are streamed in constant memory; others are read whole.
// outPath is written atomically and left untouched on error.
err = stegano.EmbedFile("reads.fastq", "reads.stego.fastq", []byte("secret"),
    stegano.WithPassword("encryption-key"))
payload, err = stegano.ExtractFile("reads.stego.fastq",
    stegano.WithPassword("encryption-key"))

// Pass any handler option through, e.g. uncompressed BAM output.
err = stegano.EmbedFile("aln.bam", "aln.stego.bam", payload,
    stegano.WithFormatSpecific("bam_compression", "none"))

// Detect format only.
format, err := stegano.DetectFormat(carrier)
fmt.Println(format) // e.g. "PNG"

// Library version.
fmt.Println(stegano.Version()) // crate version, e.g. "0.7.0"
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
| `FormatMicroscopy` | Microscopy images (handled as PNG/TIFF) |
| `FormatDICOM` | DICOM medical images |
| `FormatWAV` | WAV audio |
| `FormatBinarySignal` | Raw 16-bit little-endian samples |
| `FormatCSVTimeseries` | CSV (last-digit parity of numeric cells; stays valid CSV) |
| `FormatFITS` | FITS astronomical images |
| `FormatBinary` | Generic binary (fallback) |
| `FormatText` | Plain UTF-8 text (trailing whitespace; visible text unchanged) |

## Options

| Option | Description |
|--------|-------------|
| `WithPassword(string)` | Encryption password. Empty or omitted = no encryption: the payload is stored in plaintext. |
| `WithFormat(Format)` | Override auto-detected format. |
| `WithStrategy(string)` | Format-specific strategy, sent under the key the handler reads. PNG/TIFF (`imagery_strategy`): `lsb\|adaptive\|edge\|dct`. VCF (`vcf_strategy`): `info\|quality\|format\|id\|multi`. SAM/BAM (`sam_strategy`): `optional\|comment\|quality\|auxiliary\|multi`. DICOM (`dicom_strategy`): `private\|metadata\|sequence\|pixel_lsb\|multi`. WAV (`signal_strategy`): `timedomain\|frequency\|phase\|spread\|echo`. FASTA/FASTQ: `genomics_strategy`. FITS (`fits_strategy`): `pixel_lsb\|float_mantissa`. Other formats return `ErrInvalidParameter`. Pass the same strategy to `Extract`. |
| `WithCipher(string)` | Encryption cipher: `aes256gcm` (default) or `chacha20`. Requires `WithPassword`; auto-detected on `Extract`. |
| `WithStealth(string)` | `low\|medium\|high\|maximum` (default `high`). |
| `WithECLevel(string)` | Reed-Solomon level `none\|low\|medium\|high\|maximum` (default `none`). Without a password, `Extract` needs a non-`none` level too if one was used for `Embed`. |
| `WithFormatSpecific(key string, value any)` | Raw `format_specific` key passed to the handler, e.g. `("bam_compression", "none")`, `("sam_strategy", "auxiliary")` or, for FITS float data, `("fits_strategy", "float_mantissa")`. `value` must be JSON-encodable. Typed options (`WithStrategy`, `WithCipher`, ...) win for the same key. |

`EmbedFile` and `ExtractFile` take the same options. Without `WithFormat` they
detect the format from the file name, then from the file's first 4 KiB.

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

- The C declarations come from `go/stegano.h`, generated by cbindgen from
  `src/ffi.rs` (identical to `include/stegano.h`; CI checks both are current).
  Do not edit it by hand.
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
