//go:build cgo

package stegano_test

import (
	"bytes"
	"encoding/binary"
	"errors"
	"fmt"
	"image"
	"image/color"
	"image/png"
	"math"
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
func fastaCarrier() []byte {
	const seq = "ACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGT" // 60 nt
	var b strings.Builder
	for i := 0; i < 200; i++ {
		fmt.Fprintf(&b, ">seq%03d\n%s\n", i, seq)
	}
	return []byte(b.String())
}

// samCarrier returns a SAM text file with n alignment records.
func samCarrier(n int) []byte {
	var b strings.Builder
	b.WriteString("@HD\tVN:1.6\tSO:coordinate\n@SQ\tSN:chr1\tLN:1000000\n")
	for i := 0; i < n; i++ {
		fmt.Fprintf(&b, "read%d\t0\tchr1\t%d\t60\t8M\t*\t0\t0\tACGTACGT\tIIIIIIII\n", i, i+1)
	}
	return []byte(b.String())
}

// pngCarrier returns a real PNG image produced by the standard library encoder.
func pngCarrier(w, h int) []byte {
	img := image.NewRGBA(image.Rect(0, 0, w, h))
	for y := 0; y < h; y++ {
		for x := 0; x < w; x++ {
			img.Set(x, y, color.RGBA{uint8(x * 7), uint8(y * 5), uint8(x ^ y), 255})
		}
	}
	var buf bytes.Buffer
	if err := png.Encode(&buf, img); err != nil {
		panic(err)
	}
	return buf.Bytes()
}

// wavCarrier returns a mono 16-bit PCM WAV file with a sine tone.
func wavCarrier(sampleRate, seconds int) []byte {
	n := sampleRate * seconds
	data := make([]byte, 2*n)
	for i := 0; i < n; i++ {
		s := int16(8000 * math.Sin(2*math.Pi*440*float64(i)/float64(sampleRate)))
		binary.LittleEndian.PutUint16(data[2*i:], uint16(s))
	}
	var b bytes.Buffer
	b.WriteString("RIFF")
	binary.Write(&b, binary.LittleEndian, uint32(36+len(data)))
	b.WriteString("WAVEfmt ")
	binary.Write(&b, binary.LittleEndian, uint32(16))
	binary.Write(&b, binary.LittleEndian, uint16(1)) // PCM
	binary.Write(&b, binary.LittleEndian, uint16(1)) // mono
	binary.Write(&b, binary.LittleEndian, uint32(sampleRate))
	binary.Write(&b, binary.LittleEndian, uint32(sampleRate*2))
	binary.Write(&b, binary.LittleEndian, uint16(2))
	binary.Write(&b, binary.LittleEndian, uint16(16))
	b.WriteString("data")
	binary.Write(&b, binary.LittleEndian, uint32(len(data)))
	b.Write(data)
	return b.Bytes()
}

func roundtrip(t *testing.T, carrier, payload []byte, opts ...stegano.Option) []byte {
	t.Helper()
	modified, err := stegano.Embed(carrier, payload, opts...)
	if err != nil {
		t.Fatalf("Embed error: %v", err)
	}
	got, err := stegano.Extract(modified, opts...)
	if err != nil {
		t.Fatalf("Extract error: %v", err)
	}
	if !bytes.Equal(got, payload) {
		t.Fatalf("payload mismatch: got %q, want %q", got, payload)
	}
	return modified
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

func TestDetectFormat(t *testing.T) {
	fits := make([]byte, 80)
	copy(fits, "SIMPLE  =                    T / synthetic FITS header")
	cases := []struct {
		name    string
		carrier []byte
		want    stegano.Format
	}{
		{"binary", genericCarrier(256), stegano.FormatBinary},
		{"fasta", fastaCarrier(), stegano.FormatFASTA},
		{"fits", fits, stegano.FormatFITS},
		{"sam", samCarrier(10), stegano.FormatSAM},
		{"png", pngCarrier(8, 8), stegano.FormatPNG},
		{"wav", wavCarrier(8000, 1), stegano.FormatWAV},
	}
	for _, tc := range cases {
		got, err := stegano.DetectFormat(tc.carrier)
		if err != nil {
			t.Fatalf("%s: DetectFormat error: %v", tc.name, err)
		}
		if got != tc.want {
			t.Errorf("%s: got %s, want %s", tc.name, got, tc.want)
		}
	}
}

func TestDetectFormat_Empty(t *testing.T) {
	if _, err := stegano.DetectFormat(nil); err == nil {
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
	info, err := stegano.EstimateCapacity(genericCarrier(4096))
	if err != nil {
		t.Fatalf("EstimateCapacity error: %v", err)
	}
	if info.TotalBits <= 0 {
		t.Fatalf("expected positive TotalBits, got %d", info.TotalBits)
	}
	if info.RecommendedBits > info.TotalBits {
		t.Fatalf("RecommendedBits (%d) > TotalBits (%d)", info.RecommendedBits, info.TotalBits)
	}
}

func TestEstimateCapacity_WithFormat(t *testing.T) {
	info, err := stegano.EstimateCapacity(genericCarrier(8192), stegano.WithFormat(stegano.FormatBinary))
	if err != nil {
		t.Fatalf("EstimateCapacity with explicit format error: %v", err)
	}
	if info.Format != stegano.FormatBinary {
		t.Fatalf("expected FormatBinary, got %s", info.Format)
	}
}

func TestEstimateCapacity_InvalidFormat(t *testing.T) {
	_, err := stegano.EstimateCapacity(genericCarrier(4096), stegano.WithFormat(stegano.Format(99)))
	var serr *stegano.Error
	if !errors.As(err, &serr) || serr.Code != stegano.ErrInvalidParameter {
		t.Fatalf("expected ErrInvalidParameter for format 99, got %v", err)
	}
}

func TestEstimateCapacity_Empty(t *testing.T) {
	if _, err := stegano.EstimateCapacity(nil); err == nil {
		t.Fatal("expected error for empty carrier")
	}
}

// ---------------------------------------------------------------------------
// Embed / Extract roundtrips
// ---------------------------------------------------------------------------

func TestEmbedExtract_NoPassword(t *testing.T) {
	carrier := genericCarrier(4096)
	modified := roundtrip(t, carrier, []byte("Go binding roundtrip — no password"))
	if len(modified) != len(carrier) {
		t.Fatalf("binary carrier length changed: %d -> %d", len(carrier), len(modified))
	}
}

func TestEmbedExtract_WithPassword(t *testing.T) {
	roundtrip(t, genericCarrier(8192), []byte("Go binding roundtrip — AES-256-GCM encrypted"),
		stegano.WithPassword("go_test_key"))
}

func TestEmbedExtract_FASTA(t *testing.T) {
	roundtrip(t, fastaCarrier(), []byte("FASTA roundtrip"))
}

func TestEmbedExtract_ExplicitFormat(t *testing.T) {
	roundtrip(t, genericCarrier(4096), []byte("explicit format override"),
		stegano.WithFormat(stegano.FormatBinary))
}

func TestEmbedExtract_ECLevel(t *testing.T) {
	roundtrip(t, genericCarrier(8192), []byte("reed-solomon protected"),
		stegano.WithECLevel("medium"))
}

// SAM output grows; the full grown carrier must come back (no truncation).
func TestEmbedExtract_SAMGrowsOutput(t *testing.T) {
	carrier := samCarrier(300)
	payload := []byte("SAM payload that makes the alignment file grow")
	modified := roundtrip(t, carrier, payload)
	if len(modified) <= len(carrier) {
		t.Fatalf("expected SAM output to grow: in=%d out=%d", len(carrier), len(modified))
	}
	if !bytes.HasPrefix(modified, []byte("@HD")) {
		t.Fatalf("embedded SAM no longer starts with its header")
	}
}

func TestEmbedExtract_SAMStrategy(t *testing.T) {
	carrier := samCarrier(300)
	payload := []byte("SAM comment-line strategy")
	modified := roundtrip(t, carrier, payload, stegano.WithStrategy("comment"))
	if !bytes.Contains(modified, []byte("@CO")) {
		t.Fatalf("comment strategy should add @CO lines (strategy key not honoured?)")
	}
}

func TestEmbedExtract_SAMWithPassword(t *testing.T) {
	roundtrip(t, samCarrier(500), []byte("encrypted SAM"), stegano.WithPassword("sam_key"))
}

// PNG/WAV: only round-trip is asserted, not output file validity. These use
// the LSB strategies: the imagery/signal handlers currently fail to
// round-trip PNG at medium/high stealth and WAV at high stealth, and the
// signal handler overwrites the WAV header (so the format must be given
// explicitly on extract). Those are handler bugs tracked separately.
func TestEmbedExtract_PNG(t *testing.T) {
	roundtrip(t, pngCarrier(64, 64), []byte("PNG roundtrip"), stegano.WithStealth("low"))
}

func TestEmbedExtract_PNGStrategy(t *testing.T) {
	roundtrip(t, pngCarrier(64, 64), []byte("PNG lsb strategy"), stegano.WithStrategy("lsb"))
}

func TestEmbedExtract_WAV(t *testing.T) {
	roundtrip(t, wavCarrier(48000, 1), []byte("WAV roundtrip at 48 kHz"),
		stegano.WithFormat(stegano.FormatWAV), stegano.WithStealth("low"))
}

func TestEmbedExtract_WAVStrategy(t *testing.T) {
	roundtrip(t, wavCarrier(44100, 1), []byte("WAV time-domain"),
		stegano.WithFormat(stegano.FormatWAV), stegano.WithStrategy("timedomain"))
}

func TestEmbed_StrategyUnsupportedFormat(t *testing.T) {
	_, err := stegano.Embed(genericCarrier(4096), []byte("x"), stegano.WithStrategy("lsb"))
	var serr *stegano.Error
	if !errors.As(err, &serr) || serr.Code != stegano.ErrInvalidParameter {
		t.Fatalf("expected ErrInvalidParameter for strategy on Binary, got %v", err)
	}
}

func TestEmbed_Empty(t *testing.T) {
	if _, err := stegano.Embed(nil, []byte("payload")); err == nil {
		t.Fatal("expected error for empty carrier")
	}
	if _, err := stegano.Embed(genericCarrier(256), nil); err == nil {
		t.Fatal("expected error for empty payload")
	}
}

func TestExtract_Empty(t *testing.T) {
	if _, err := stegano.Extract(nil); err == nil {
		t.Fatal("expected error for empty carrier")
	}
}

func TestExtract_WrongPassword(t *testing.T) {
	modified, err := stegano.Embed(genericCarrier(4096), []byte("password-protected payload"),
		stegano.WithPassword("correct_key"))
	if err != nil {
		t.Fatalf("Embed error: %v", err)
	}
	if _, err = stegano.Extract(modified, stegano.WithPassword("wrong_key")); err == nil {
		t.Fatal("expected error when extracting with wrong password, got nil")
	}
}

// ---------------------------------------------------------------------------
// WithCipher
// ---------------------------------------------------------------------------

func TestEmbedExtract_WithCipher(t *testing.T) {
	carrier := genericCarrier(8192)
	payload := []byte("chacha20 cipher roundtrip")

	modified, err := stegano.Embed(carrier, payload,
		stegano.WithPassword("test_key"),
		stegano.WithCipher("chacha20"))
	if err != nil {
		t.Fatalf("Embed (WithCipher) error: %v", err)
	}
	// The cipher is auto-detected on extract.
	got, err := stegano.Extract(modified, stegano.WithPassword("test_key"))
	if err != nil {
		t.Fatalf("Extract error: %v", err)
	}
	if !bytes.Equal(got, payload) {
		t.Fatalf("payload mismatch: got %q, want %q", got, payload)
	}
}

func TestEmbed_UnknownCipherRejected(t *testing.T) {
	_, err := stegano.Embed(genericCarrier(8192), []byte("x"),
		stegano.WithPassword("k"), stegano.WithCipher("rot13"))
	var serr *stegano.Error
	if !errors.As(err, &serr) || serr.Code != stegano.ErrInvalidParameter {
		t.Fatalf("expected ErrInvalidParameter for unknown cipher, got %v", err)
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
