//go:build cgo

package stegano_test

import (
	"fmt"
	"strings"
	"testing"

	stegano "github.com/scttfrdmn/stegano/go"
)

// genericCarrier returns a repeating binary carrier of the given size.
// The stegano library treats unrecognised bytes as FormatBinary.
func genericCarrier(size int) []byte {
	buf := make([]byte, size)
	for i := range buf {
		buf[i] = byte(i & 0xFF)
	}
	return buf
}

// fastaCarrier returns a synthetic FASTA file large enough for roundtrip tests.
// The FFI uses StealthLevel::Low, so capacity ≈ num_nucleotides / 8 bytes.
// 50 sequences × 60 nucleotides = 3000 nt → ~375 bytes capacity.
func fastaCarrier() []byte {
	const seq = "ACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGT" // 60 nt
	var b strings.Builder
	for i := 0; i < 50; i++ {
		b.WriteString(">seq")
		b.WriteString(strings.Repeat("0", 3-len(fmt.Sprintf("%d", i))))
		b.WriteString(fmt.Sprintf("%d\n%s\n", i, seq))
	}
	return []byte(b.String())
}

// ---------------------------------------------------------------------------
// Version
// ---------------------------------------------------------------------------

func TestVersion(t *testing.T) {
	v := stegano.Version()
	if v == "" {
		t.Fatal("Version returned empty string")
	}
	if !strings.HasPrefix(v, "0.") {
		t.Fatalf("unexpected version format: %q", v)
	}
	t.Logf("stegano version: %s", v)
}

// ---------------------------------------------------------------------------
// DetectFormat
// ---------------------------------------------------------------------------

func TestDetectFormat_Binary(t *testing.T) {
	carrier := genericCarrier(256)
	fmt, err := stegano.DetectFormat(carrier)
	if err != nil {
		t.Fatalf("DetectFormat error: %v", err)
	}
	if fmt != stegano.FormatBinary {
		t.Fatalf("expected FormatBinary, got %s", fmt)
	}
}

func TestDetectFormat_FASTA(t *testing.T) {
	fmt, err := stegano.DetectFormat(fastaCarrier())
	if err != nil {
		t.Fatalf("DetectFormat error: %v", err)
	}
	if fmt != stegano.FormatFASTA {
		t.Fatalf("expected FormatFASTA, got %s", fmt)
	}
}

func TestDetectFormat_FITS(t *testing.T) {
	fits := make([]byte, 80)
	copy(fits, []byte("SIMPLE  =                    T / synthetic FITS header"))
	for i := len("SIMPLE  =                    T / synthetic FITS header"); i < 80; i++ {
		fits[i] = ' '
	}
	fmt, err := stegano.DetectFormat(fits)
	if err != nil {
		t.Fatalf("DetectFormat error: %v", err)
	}
	if fmt != stegano.FormatFITS {
		t.Fatalf("expected FormatFITS, got %s", fmt)
	}
}

func TestDetectFormat_Empty(t *testing.T) {
	_, err := stegano.DetectFormat(nil)
	if err == nil {
		t.Fatal("expected error for empty carrier, got nil")
	}
}

// ---------------------------------------------------------------------------
// Format.String()
// ---------------------------------------------------------------------------

func TestFormatString(t *testing.T) {
	cases := []struct {
		format stegano.Format
		want   string
	}{
		{stegano.FormatFASTA, "FASTA"},
		{stegano.FormatFASTQ, "FASTQ"},
		{stegano.FormatSAM, "SAM"},
		{stegano.FormatBAM, "BAM"},
		{stegano.FormatPNG, "PNG"},
		{stegano.FormatDICOM, "DICOM"},
		{stegano.FormatWAV, "WAV"},
		{stegano.FormatFITS, "FITS"},
		{stegano.FormatBinary, "Binary"},
	}
	for _, tc := range cases {
		if got := tc.format.String(); got != tc.want {
			t.Errorf("Format(%d).String() = %q, want %q", int(tc.format), got, tc.want)
		}
	}
}

// ---------------------------------------------------------------------------
// EstimateCapacity
// ---------------------------------------------------------------------------

func TestEstimateCapacity(t *testing.T) {
	carrier := genericCarrier(4096)
	info, err := stegano.EstimateCapacity(carrier)
	if err != nil {
		t.Fatalf("EstimateCapacity error: %v", err)
	}
	if info.TotalBits <= 0 {
		t.Fatalf("expected positive TotalBits, got %d", info.TotalBits)
	}
	if info.RecommendedBits > info.TotalBits {
		t.Fatalf("RecommendedBits (%d) > TotalBits (%d)", info.RecommendedBits, info.TotalBits)
	}
	t.Logf("capacity: format=%s total=%d recommended=%d ratio=%.4f",
		info.Format, info.TotalBits, info.RecommendedBits, info.SteganographicRatio)
}

func TestEstimateCapacity_WithFormat(t *testing.T) {
	carrier := genericCarrier(8192)
	info, err := stegano.EstimateCapacity(carrier, stegano.WithFormat(stegano.FormatBinary))
	if err != nil {
		t.Fatalf("EstimateCapacity with explicit format error: %v", err)
	}
	if info.Format != stegano.FormatBinary {
		t.Fatalf("expected FormatBinary, got %s", info.Format)
	}
}

func TestEstimateCapacity_Empty(t *testing.T) {
	_, err := stegano.EstimateCapacity(nil)
	if err == nil {
		t.Fatal("expected error for empty carrier")
	}
}

// ---------------------------------------------------------------------------
// Embed / Extract roundtrips
// ---------------------------------------------------------------------------

func TestEmbedExtract_NoPassword(t *testing.T) {
	carrier := genericCarrier(4096)
	payload := []byte("Go binding roundtrip — no password")

	modified, err := stegano.Embed(carrier, payload)
	if err != nil {
		t.Fatalf("Embed error: %v", err)
	}
	if len(modified) == 0 {
		t.Fatal("Embed returned empty result")
	}

	got, err := stegano.Extract(modified)
	if err != nil {
		t.Fatalf("Extract error: %v", err)
	}
	if string(got) != string(payload) {
		t.Fatalf("payload mismatch: got %q, want %q", got, payload)
	}
}

func TestEmbedExtract_WithPassword(t *testing.T) {
	carrier := genericCarrier(8192)
	payload := []byte("Go binding roundtrip — AES-256-GCM encrypted")

	modified, err := stegano.Embed(carrier, payload, stegano.WithPassword("go_test_key"))
	if err != nil {
		t.Fatalf("Embed error: %v", err)
	}

	got, err := stegano.Extract(modified, stegano.WithPassword("go_test_key"))
	if err != nil {
		t.Fatalf("Extract error: %v", err)
	}
	if string(got) != string(payload) {
		t.Fatalf("payload mismatch: got %q, want %q", got, payload)
	}
}

func TestEmbedExtract_FASTA(t *testing.T) {
	carrier := fastaCarrier()
	payload := []byte("FASTA roundtrip")

	modified, err := stegano.Embed(carrier, payload)
	if err != nil {
		t.Fatalf("Embed error: %v", err)
	}

	got, err := stegano.Extract(modified)
	if err != nil {
		t.Fatalf("Extract error: %v", err)
	}
	if string(got) != string(payload) {
		t.Fatalf("payload mismatch: got %q, want %q", got, payload)
	}
}

func TestEmbedExtract_ExplicitFormat(t *testing.T) {
	carrier := genericCarrier(4096)
	payload := []byte("explicit format override")

	modified, err := stegano.Embed(carrier, payload, stegano.WithFormat(stegano.FormatBinary))
	if err != nil {
		t.Fatalf("Embed error: %v", err)
	}

	got, err := stegano.Extract(modified, stegano.WithFormat(stegano.FormatBinary))
	if err != nil {
		t.Fatalf("Extract error: %v", err)
	}
	if string(got) != string(payload) {
		t.Fatalf("payload mismatch: got %q, want %q", got, payload)
	}
}

func TestEmbed_Empty(t *testing.T) {
	_, err := stegano.Embed(nil, []byte("payload"))
	if err == nil {
		t.Fatal("expected error for empty carrier")
	}
	_, err = stegano.Embed(genericCarrier(256), nil)
	if err == nil {
		t.Fatal("expected error for empty payload")
	}
}

func TestExtract_Empty(t *testing.T) {
	_, err := stegano.Extract(nil)
	if err == nil {
		t.Fatal("expected error for empty carrier")
	}
}

func TestExtract_WrongPassword(t *testing.T) {
	carrier := genericCarrier(4096)
	payload := []byte("password-protected payload")

	modified, err := stegano.Embed(carrier, payload, stegano.WithPassword("correct_key"))
	if err != nil {
		t.Fatalf("Embed error: %v", err)
	}

	// Extraction with wrong password should fail.
	_, err = stegano.Extract(modified, stegano.WithPassword("wrong_key"))
	if err == nil {
		t.Fatal("expected error when extracting with wrong password, got nil")
	}
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

func TestError_Message(t *testing.T) {
	e := &stegano.Error{Code: stegano.ErrInsufficientCapacity, Message: "Embed: insufficient carrier capacity for payload"}
	if !strings.Contains(e.Error(), "insufficient") {
		t.Fatalf("unexpected error string: %s", e.Error())
	}
}
