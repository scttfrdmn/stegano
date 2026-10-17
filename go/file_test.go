//go:build cgo

package stegano_test

import (
	"bytes"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	stegano "github.com/scttfrdmn/stegano/go"
)

// ---------------------------------------------------------------------------
// File API (EmbedFile / ExtractFile) and WithFormatSpecific
// ---------------------------------------------------------------------------

// fastqCarrier returns a FASTQ file with n 4-line records.
func fastqCarrier(n int) []byte {
	var b strings.Builder
	for i := 0; i < n; i++ {
		fmt.Fprintf(&b, "@read%d 1:N:0:ACGT\nACGTACGTACGTACGTACGTACGTACGTAC\n+\nIIIIIFFFFF::::,,,,IIIIIFFFFF::\n", i)
	}
	return []byte(b.String())
}

// vcfCarrier returns a VCF file with n variant records.
func vcfCarrier(n int) []byte {
	var b strings.Builder
	b.WriteString("##fileformat=VCFv4.2\n")
	b.WriteString("##contig=<ID=chr1,length=1000000000>\n")
	b.WriteString("##INFO=<ID=DP,Number=1,Type=Integer,Description=\"Depth\">\n")
	b.WriteString("#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n")
	for i := 0; i < n; i++ {
		fmt.Fprintf(&b, "chr1\t%d\t.\tA\tG\t50\tPASS\tDP=%d\n", 100+i*10, 10+i%30)
	}
	return []byte(b.String())
}

func writeTemp(t *testing.T, name string, data []byte) string {
	t.Helper()
	p := filepath.Join(t.TempDir(), name)
	if err := os.WriteFile(p, data, 0o644); err != nil {
		t.Fatal(err)
	}
	return p
}

// fileRoundtrip embeds with EmbedFile, extracts with ExtractFile and returns
// the embedded file's bytes.
func fileRoundtrip(t *testing.T, name string, carrier, payload []byte, opts ...stegano.Option) []byte {
	t.Helper()
	in := writeTemp(t, name, carrier)
	out := filepath.Join(filepath.Dir(in), "stego-"+name)
	if err := stegano.EmbedFile(in, out, payload, opts...); err != nil {
		t.Fatalf("EmbedFile(%s): %v", name, err)
	}
	got, err := stegano.ExtractFile(out, opts...)
	if err != nil {
		t.Fatalf("ExtractFile(%s): %v", name, err)
	}
	if !bytes.Equal(got, payload) {
		t.Fatalf("ExtractFile(%s) = %q, want %q", name, got, payload)
	}
	data, err := os.ReadFile(out)
	if err != nil {
		t.Fatal(err)
	}
	return data
}

func TestEmbedExtractFile_FASTQ(t *testing.T) {
	carrier := fastqCarrier(500)
	payload := []byte("streamed FASTQ payload")
	fileRoundtrip(t, "reads.fastq", carrier, payload, stegano.WithPassword("pw"))

	// The streamed output interoperates with the in-memory API.
	out := fileRoundtrip(t, "reads.fastq", carrier, payload)
	got, err := stegano.Extract(out, stegano.WithFormat(stegano.FormatFASTQ))
	if err != nil || !bytes.Equal(got, payload) {
		t.Fatalf("in-memory Extract of streamed file: %q, %v", got, err)
	}
}

func TestEmbedExtractFile_VCF(t *testing.T) {
	fileRoundtrip(t, "calls.vcf", vcfCarrier(400), []byte("streamed VCF payload"),
		stegano.WithPassword("vcf"))
	fileRoundtrip(t, "calls-id.vcf", vcfCarrier(400), []byte("VCF id strategy"),
		stegano.WithStrategy("id"))
}

func TestEmbedExtractFile_DetectsFormatFromContent(t *testing.T) {
	// No extension: the format comes from the file's first bytes.
	fileRoundtrip(t, "reads.dat", fastqCarrier(300), []byte("detected FASTQ"))
}

func TestEmbedExtractFile_SAM(t *testing.T) {
	out := fileRoundtrip(t, "aln.sam", samCarrier(300), []byte("SAM via EmbedFile"))
	if !bytes.HasPrefix(out, []byte("@HD")) {
		t.Fatalf("embedded SAM no longer starts with its header")
	}
}

func TestEmbedExtractFile_SAMAuxiliaryFormatSpecific(t *testing.T) {
	// sam_strategy=auxiliary is streamable; passing it as a raw key proves
	// WithFormatSpecific reaches the file API.
	payload := []byte("SAM auxiliary via WithFormatSpecific")
	out := fileRoundtrip(t, "aux.sam", samCarrier(300), payload,
		stegano.WithFormatSpecific("sam_strategy", "auxiliary"), stegano.WithPassword("aux"))
	if !bytes.Contains(out, []byte("\tzs:Z:")) {
		t.Fatalf("auxiliary strategy should add zs:Z: tags")
	}
	if bytes.Contains(out, []byte("@CO")) {
		t.Fatalf("unexpected @CO lines: strategy key not honoured")
	}
	// Readable through the in-memory API as well.
	got, err := stegano.Extract(out, stegano.WithPassword("aux"))
	if err != nil || !bytes.Equal(got, payload) {
		t.Fatalf("in-memory Extract of streamed SAM: %q, %v", got, err)
	}
}

func TestEmbedFile_Errors(t *testing.T) {
	dir := t.TempDir()
	var serr *stegano.Error

	err := stegano.EmbedFile(filepath.Join(dir, "missing.fastq"), filepath.Join(dir, "o.fastq"), []byte("x"))
	if !errors.As(err, &serr) || serr.Code != stegano.ErrIO {
		t.Fatalf("expected ErrIO for a missing input, got %v", err)
	}

	in := writeTemp(t, "small.fastq", fastqCarrier(2))
	out := filepath.Join(filepath.Dir(in), "out.fastq")
	if err := os.WriteFile(out, []byte("keep"), 0o644); err != nil {
		t.Fatal(err)
	}
	err = stegano.EmbedFile(in, out, bytes.Repeat([]byte("p"), 4000))
	if !errors.As(err, &serr) || serr.Code != stegano.ErrInsufficientCapacity {
		t.Fatalf("expected ErrInsufficientCapacity, got %v", err)
	}
	if data, _ := os.ReadFile(out); string(data) != "keep" {
		t.Fatalf("failed EmbedFile modified the existing output: %q", data)
	}

	if _, err := stegano.ExtractFile(filepath.Join(dir, "missing.vcf")); !errors.As(err, &serr) || serr.Code != stegano.ErrIO {
		t.Fatalf("expected ErrIO for a missing input, got %v", err)
	}
	if err := stegano.EmbedFile("", out, []byte("x")); !errors.As(err, &serr) || serr.Code != stegano.ErrInvalidParameter {
		t.Fatalf("expected ErrInvalidParameter for an empty path, got %v", err)
	}
	if err := stegano.EmbedFile(in, out, nil); !errors.As(err, &serr) || serr.Code != stegano.ErrInvalidParameter {
		t.Fatalf("expected ErrInvalidParameter for an empty payload, got %v", err)
	}
}

func TestWithFormatSpecific_ReachesHandler(t *testing.T) {
	carrier := samCarrier(300)
	payload := []byte("format_specific passthrough")

	// sam_strategy given as a raw key selects the comment channel.
	modified := roundtrip(t, carrier, payload, stegano.WithFormatSpecific("sam_strategy", "comment"))
	if !bytes.Contains(modified, []byte("@CO")) {
		t.Fatalf("sam_strategy via WithFormatSpecific was not honoured")
	}

	// An invalid value is rejected by the handler.
	var serr *stegano.Error
	_, err := stegano.Embed(carrier, payload, stegano.WithFormatSpecific("sam_strategy", "bogus"))
	if !errors.As(err, &serr) || serr.Code != stegano.ErrInvalidParameter {
		t.Fatalf("expected ErrInvalidParameter for an unknown sam_strategy, got %v", err)
	}

	// The typed option wins over a raw key with the same name.
	modified = roundtrip(t, carrier, payload,
		stegano.WithFormatSpecific("sam_strategy", "bogus"), stegano.WithStrategy("comment"))
	if !bytes.Contains(modified, []byte("@CO")) {
		t.Fatalf("WithStrategy should override WithFormatSpecific")
	}

	// File API too.
	in := writeTemp(t, "aln.sam", carrier)
	err = stegano.EmbedFile(in, in+".out", payload, stegano.WithFormatSpecific("sam_strategy", "bogus"))
	if !errors.As(err, &serr) || serr.Code != stegano.ErrInvalidParameter {
		t.Fatalf("EmbedFile: expected ErrInvalidParameter, got %v", err)
	}

	// Values must be JSON-encodable.
	_, err = stegano.Embed(carrier, payload, stegano.WithFormatSpecific("x", func() {}))
	if !errors.As(err, &serr) || serr.Code != stegano.ErrInvalidParameter {
		t.Fatalf("expected ErrInvalidParameter for a non-JSON value, got %v", err)
	}
}
