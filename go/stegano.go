//go:build cgo

// Package stegano provides idiomatic Go bindings for the stegano steganography
// library via CGO.
//
// # Build requirements
//
// The stegano Rust library must be compiled before using this package:
//
//	cd /path/to/stegano && cargo build --release
//
// Then set linker paths so CGO can find the library:
//
//	export CGO_LDFLAGS="-L$(pwd)/target/release"
//	export DYLD_LIBRARY_PATH="$(pwd)/target/release"   # macOS
//	export LD_LIBRARY_PATH="$(pwd)/target/release"      # Linux
//
// # Usage
//
//	import stegano "github.com/scttfrdmn/stegano/go"
//
//	// Embed a secret payload into a carrier (format auto-detected).
//	modified, err := stegano.Embed(carrier, []byte("secret"), stegano.WithPassword("key"))
//
//	// Extract it back.
//	payload, err := stegano.Extract(modified, stegano.WithPassword("key"))
//
//	// Inspect capacity before embedding.
//	info, err := stegano.EstimateCapacity(carrier)
//	fmt.Printf("Format: %s, Total bits: %d\n", info.Format, info.TotalBits)
package stegano

/*
#cgo LDFLAGS: -lstegano
#cgo darwin LDFLAGS: -framework Security -framework Foundation

#include <stdlib.h>
#include <stdint.h>
#include <stdbool.h>

typedef enum {
    SteganoSuccess               = 0,
    SteganoInvalidParameter      = 1,
    SteganoInsufficientCapacity  = 2,
    SteganoCryptographicFailure  = 3,
    SteganoFormatNotSupported    = 4,
    SteganoDataCorrupted         = 5,
    SteganoGpuError              = 6,
    SteganoIoError               = 7,
    SteganoUnknown               = 999,
} SteganoErrorCode;

typedef enum {
    DataFormatFasta          = 0,
    DataFormatFastq          = 1,
    DataFormatVcf            = 2,
    DataFormatSam            = 3,
    DataFormatBam            = 4,
    DataFormatPng            = 5,
    DataFormatTiff           = 6,
    DataFormatMicroscopy     = 7,
    DataFormatDicom          = 8,
    DataFormatWav            = 9,
    DataFormatBinarySignal   = 10,
    DataFormatCsvTimeseries  = 11,
    DataFormatBinary         = 12,
    DataFormatText           = 13,
    DataFormatFits           = 14,
} CDataFormat;

typedef struct {
    bool     success;
    uint32_t bytes_embedded;
    bool     carrier_modified;
    uint32_t embedding_time_ms;
    uint32_t memory_usage_bytes;
} CEmbedResult;

typedef struct {
    uint32_t total_bits;
    uint32_t recommended_bits;
    double   steganographic_ratio;
} CCapacityInfo;

extern SteganoErrorCode stegano_init();
extern SteganoErrorCode stegano_detect_format(
    const unsigned char* data, uint32_t data_len,
    CDataFormat* format_out);
extern SteganoErrorCode stegano_estimate_capacity(
    const unsigned char* data, uint32_t data_len,
    CDataFormat format, CCapacityInfo* info_out);
extern SteganoErrorCode stegano_embed(
    unsigned char* carrier_data, uint32_t carrier_len,
    const unsigned char* payload_data, uint32_t payload_len,
    CDataFormat format, const char* password,
    CEmbedResult* result_out);
extern SteganoErrorCode stegano_extract(
    const unsigned char* carrier_data, uint32_t carrier_len,
    CDataFormat format, const char* password,
    unsigned char** payload_out, uint32_t* payload_len_out);
extern void        stegano_free(void* ptr);
extern const char* stegano_version();
*/
import "C"

import (
	"fmt"
	"math"
	"unsafe"
)

// ---------------------------------------------------------------------------
// Format constants
// ---------------------------------------------------------------------------

// Format identifies the carrier data format.
type Format int32

const (
	FormatFASTA         Format = 0
	FormatFASTQ         Format = 1
	FormatVCF           Format = 2
	FormatSAM           Format = 3
	FormatBAM           Format = 4
	FormatPNG           Format = 5
	FormatTIFF          Format = 6
	FormatMicroscopy    Format = 7
	FormatDICOM         Format = 8
	FormatWAV           Format = 9
	FormatBinarySignal  Format = 10
	FormatCSVTimeseries Format = 11
	FormatBinary        Format = 12
	FormatText          Format = 13
	FormatFITS          Format = 14
)

// String returns a human-readable name for the format.
func (f Format) String() string {
	switch f {
	case FormatFASTA:
		return "FASTA"
	case FormatFASTQ:
		return "FASTQ"
	case FormatVCF:
		return "VCF"
	case FormatSAM:
		return "SAM"
	case FormatBAM:
		return "BAM"
	case FormatPNG:
		return "PNG"
	case FormatTIFF:
		return "TIFF"
	case FormatMicroscopy:
		return "Microscopy"
	case FormatDICOM:
		return "DICOM"
	case FormatWAV:
		return "WAV"
	case FormatBinarySignal:
		return "BinarySignal"
	case FormatCSVTimeseries:
		return "CSVTimeseries"
	case FormatBinary:
		return "Binary"
	case FormatText:
		return "Text"
	case FormatFITS:
		return "FITS"
	default:
		return fmt.Sprintf("Format(%d)", int(f))
	}
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

// ErrorCode is the numeric code returned by FFI operations.
type ErrorCode int

const (
	ErrInvalidParameter     ErrorCode = 1
	ErrInsufficientCapacity ErrorCode = 2
	ErrCryptographic        ErrorCode = 3
	ErrFormatNotSupported   ErrorCode = 4
	ErrDataCorrupted        ErrorCode = 5
	ErrGPU                  ErrorCode = 6
	ErrIO                   ErrorCode = 7
	ErrUnknown              ErrorCode = 999
)

// Error is returned when a stegano FFI operation fails.
type Error struct {
	Code    ErrorCode
	Message string
}

func (e *Error) Error() string {
	return fmt.Sprintf("stegano: %s (code %d)", e.Message, e.Code)
}

func codeToError(code C.SteganoErrorCode, op string) *Error {
	descriptions := map[C.SteganoErrorCode]string{
		C.SteganoInvalidParameter:     "invalid parameter",
		C.SteganoInsufficientCapacity: "insufficient carrier capacity for payload",
		C.SteganoCryptographicFailure: "cryptographic failure",
		C.SteganoFormatNotSupported:   "format not supported",
		C.SteganoDataCorrupted:        "data corrupted or no payload found",
		C.SteganoGpuError:             "GPU error",
		C.SteganoIoError:              "I/O error",
		C.SteganoUnknown:              "unknown error",
	}
	desc, ok := descriptions[code]
	if !ok {
		desc = fmt.Sprintf("error code %d", int(code))
	}
	return &Error{Code: ErrorCode(code), Message: fmt.Sprintf("%s: %s", op, desc)}
}

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

type options struct {
	password string
	format   *Format // nil = auto-detect
}

// Option configures an Embed or Extract call.
type Option func(*options)

// WithPassword sets the AES-256-GCM encryption password.
// An empty string disables encryption (default).
func WithPassword(password string) Option {
	return func(o *options) { o.password = password }
}

// WithFormat overrides the auto-detected carrier format.
// Use this when the carrier bytes cannot be reliably auto-detected
// (e.g. raw binary data that should be treated as SAM).
func WithFormat(format Format) Option {
	return func(o *options) { o.format = &format }
}

func applyOptions(opts []Option) options {
	o := options{}
	for _, opt := range opts {
		opt(&o)
	}
	return o
}

// ---------------------------------------------------------------------------
// CapacityInfo
// ---------------------------------------------------------------------------

// CapacityInfo describes the steganographic capacity of a carrier.
type CapacityInfo struct {
	// TotalBits is the total number of bits available for payload.
	TotalBits int
	// RecommendedBits is a conservative upper bound for reliable embedding.
	RecommendedBits int
	// SteganographicRatio is the fraction of carrier bytes used for embedding.
	SteganographicRatio float64
	// Format is the detected or specified carrier format.
	Format Format
}

// ---------------------------------------------------------------------------
// Initialisation (called once via init())
// ---------------------------------------------------------------------------

func init() {
	if code := C.stegano_init(); code != C.SteganoSuccess {
		// Non-fatal: the library still functions; init() is a no-op in v0.5.1.
		_ = code
	}
}

// ---------------------------------------------------------------------------
// Size conversion helper
// ---------------------------------------------------------------------------

func toUint32(n int, label string) (C.uint32_t, error) {
	if n < 0 {
		return 0, fmt.Errorf("stegano: %s cannot be negative (%d)", label, n)
	}
	if n > math.MaxUint32 {
		return 0, fmt.Errorf("stegano: %s (%d) exceeds uint32 max", label, n)
	}
	return C.uint32_t(n), nil
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

// Version returns the stegano library version string (e.g. "0.5.1").
func Version() string {
	return C.GoString(C.stegano_version())
}

// DetectFormat auto-detects the format of carrier bytes.
// Returns FormatBinary if the format cannot be determined.
func DetectFormat(carrier []byte) (Format, error) {
	if len(carrier) == 0 {
		return FormatBinary, &Error{Code: ErrInvalidParameter, Message: "DetectFormat: carrier is empty"}
	}
	n, err := toUint32(len(carrier), "carrier length")
	if err != nil {
		return FormatBinary, err
	}
	var fmtOut C.CDataFormat
	code := C.stegano_detect_format(
		(*C.uchar)(unsafe.Pointer(&carrier[0])),
		n,
		&fmtOut,
	)
	if code != C.SteganoSuccess {
		return FormatBinary, codeToError(code, "DetectFormat")
	}
	return Format(fmtOut), nil
}

// EstimateCapacity returns the embedding capacity of carrier.
// Format is auto-detected unless WithFormat is specified.
func EstimateCapacity(carrier []byte, opts ...Option) (CapacityInfo, error) {
	if len(carrier) == 0 {
		return CapacityInfo{}, &Error{Code: ErrInvalidParameter, Message: "EstimateCapacity: carrier is empty"}
	}
	o := applyOptions(opts)

	format, err := resolveFormat(carrier, o)
	if err != nil {
		return CapacityInfo{}, err
	}
	n, err := toUint32(len(carrier), "carrier length")
	if err != nil {
		return CapacityInfo{}, err
	}

	var capOut C.CCapacityInfo
	code := C.stegano_estimate_capacity(
		(*C.uchar)(unsafe.Pointer(&carrier[0])),
		n,
		C.CDataFormat(format),
		&capOut,
	)
	if code != C.SteganoSuccess {
		return CapacityInfo{}, codeToError(code, "EstimateCapacity")
	}
	return CapacityInfo{
		TotalBits:           int(capOut.total_bits),
		RecommendedBits:     int(capOut.recommended_bits),
		SteganographicRatio: float64(capOut.steganographic_ratio),
		Format:              format,
	}, nil
}

// Embed embeds payload into carrier and returns the modified carrier bytes.
// Format is auto-detected unless WithFormat is specified.
// Password-based AES-256-GCM encryption is applied when WithPassword is used.
func Embed(carrier, payload []byte, opts ...Option) ([]byte, error) {
	if len(carrier) == 0 {
		return nil, &Error{Code: ErrInvalidParameter, Message: "Embed: carrier is empty"}
	}
	if len(payload) == 0 {
		return nil, &Error{Code: ErrInvalidParameter, Message: "Embed: payload is empty"}
	}
	o := applyOptions(opts)

	format, err := resolveFormat(carrier, o)
	if err != nil {
		return nil, err
	}
	carrierLen, err := toUint32(len(carrier), "carrier length")
	if err != nil {
		return nil, err
	}
	payloadLen, err := toUint32(len(payload), "payload length")
	if err != nil {
		return nil, err
	}

	// Work on a copy so the caller's slice is not modified on failure.
	buf := make([]byte, len(carrier))
	copy(buf, carrier)

	var cPassword *C.char
	if o.password != "" {
		cPassword = C.CString(o.password)
		defer C.free(unsafe.Pointer(cPassword))
	}

	var result C.CEmbedResult
	code := C.stegano_embed(
		(*C.uchar)(unsafe.Pointer(&buf[0])),
		carrierLen,
		(*C.uchar)(unsafe.Pointer(&payload[0])),
		payloadLen,
		C.CDataFormat(format),
		cPassword,
		&result,
	)
	if code != C.SteganoSuccess {
		return nil, codeToError(code, "Embed")
	}
	return buf, nil
}

// Extract extracts the embedded payload from carrier.
// Format is auto-detected unless WithFormat is specified.
// The same password used during Embed must be provided via WithPassword.
func Extract(carrier []byte, opts ...Option) ([]byte, error) {
	if len(carrier) == 0 {
		return nil, &Error{Code: ErrInvalidParameter, Message: "Extract: carrier is empty"}
	}
	o := applyOptions(opts)

	format, err := resolveFormat(carrier, o)
	if err != nil {
		return nil, err
	}
	carrierLen, err := toUint32(len(carrier), "carrier length")
	if err != nil {
		return nil, err
	}

	var cPassword *C.char
	if o.password != "" {
		cPassword = C.CString(o.password)
		defer C.free(unsafe.Pointer(cPassword))
	}

	var payloadPtr *C.uchar
	var payloadLen C.uint32_t
	code := C.stegano_extract(
		(*C.uchar)(unsafe.Pointer(&carrier[0])),
		carrierLen,
		C.CDataFormat(format),
		cPassword,
		&payloadPtr,
		&payloadLen,
	)
	if code != C.SteganoSuccess {
		return nil, codeToError(code, "Extract")
	}
	if payloadPtr == nil || payloadLen == 0 {
		return nil, &Error{Code: ErrDataCorrupted, Message: "Extract: no payload found"}
	}
	defer C.stegano_free(unsafe.Pointer(payloadPtr))

	result := C.GoBytes(unsafe.Pointer(payloadPtr), C.int(payloadLen))
	return result, nil
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

func resolveFormat(carrier []byte, o options) (Format, error) {
	if o.format != nil {
		return *o.format, nil
	}
	return DetectFormat(carrier)
}
