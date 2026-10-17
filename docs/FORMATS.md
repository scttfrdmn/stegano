# Carrier formats

What each handler changes in a file, how much it can hold, how visible the
change is, and how the output has been checked. This document follows the
`//!` module documentation in `src/handlers/*.rs` and `src/stream/mod.rs`. If
the two disagree, the source is right.

## Read this first

- **Every format carries a fixed marker.** Each handler writes a constant
  header (`GNSJ`, `IMSJ`, `SGSJ`, `FTSJ`, `STGD`, `SBM\x01`, `CSVJ`, `TXTJ`,
  the VCF stream byte, the generic length header) in front of the payload. Anyone with this
  library can tell whether a file carries a payload, without the password.
  `stegano detect` does exactly that.
- **Several strategies are plaintext-visible.** FASTA headers, VCF
  INFO/FORMAT/ID/QUAL, SAM/BAM tags and comments, and DICOM private elements
  store the payload as hex text or tagged bytes that anyone opening the file
  can see. The hex is ciphertext when you use a password and plaintext when
  you do not.
- **The LSB strategies are detectable.** LSB replacement in pixels, samples or
  quality scores is a well-studied target. The detectors described in
  [VALIDATION_GUIDE.md](VALIDATION_GUIDE.md) find full-capacity embedding.
- **Nothing is robust to lossy processing.** Recompression, resampling,
  re-encoding, normalisation or re-sorting destroys the payload. Stegano is
  designed for bit-exact storage only.
- **Encryption protects the content, not the fact of embedding.** See
  [the threat model and limitations](../SECURITY.md#threat-model-and-limitations).

Every handler stores the output of `codec::encode_payload` (see
[WIRE_FORMAT.md](WIRE_FORMAT.md)): a 36-byte header plus the body, plus a
16-byte tag with a password, plus Reed-Solomon parity with an EC level. The
capacity figures below are the raw space in the carrier. Subtract the handler's
own header and the codec overhead to get the payload size.

Strategy names are given as `format_specific` values. The CLI's `--strategy`
writes the same value under the key shown. Extraction reads the strategy from
the carrier for every format, so you never need to pass it to `extract`.

**External validators.** `tests/format_validity/run.py` (run in CI as the
"format validity" job) embeds a random payload with each strategy through the
CLI, extracts it, and checks the output with an independent tool. The tool is
named per format below. Formats without a validator entry are not covered by
that suite.

## Summary

| Format | `DataFormat` | Key | Strategies (default first) | Streamable |
|--------|--------------|-----|---------------------------|------------|
| [FASTQ](#fastq) | `FASTQ` | `genomics_strategy` | from stealth; `conservative`, `balanced`, `aggressive`, `quality` | yes, all |
| [FASTA](#fasta) | `FASTA` | `genomics_strategy` | `header` | no |
| [VCF](#vcf) | `VCF` | `vcf_strategy` | `info`, `format`, `id`, `quality`, `multi` | `info`, `id`, `quality` |
| [SAM / BAM](#sam-and-bam) | `SAM`, `BAM` | `sam_strategy` | `optional`, `auxiliary`, `quality`, `comment`, `multi` | `auxiliary`, `quality` |
| [PNG / TIFF](#png-and-tiff) | `PNG`, `TIFF`, `MICROSCOPY` | `imagery_strategy` | from stealth; `lsb`, `adaptive`, `edge`, `dct` | no |
| [DICOM](#dicom) | `DICOM` | `dicom_strategy` | `private`, `metadata`, `sequence`, `pixel_lsb`, `multi` | no |
| [FITS](#fits) | `FITS` | `fits_strategy` | `pixel_lsb`, `float_mantissa` | yes, all |
| [WAV](#wav) | `WAV` | `signal_strategy` | from stealth; `timedomain`, `frequency`, `phase`, `spread`, `echo` | `timedomain` |
| [Raw signals](#raw-signals) | `BinarySignal` | `signal_strategy` | as WAV | no |
| [CSV](#csv) | `CsvTimeseries` | `csv_strategy` | `digit` (last-digit parity of numeric cells) | yes |
| [Plain text](#plain-text) | `TEXT` | `text_strategy` | `whitespace` (trailing spaces/tabs) | with a fixed `text_bits_per_line` |
| [Binary](#binary-generic) | `BINARY` | none | 1-bit LSB | no |

"From stealth" means the strategy follows `EmbedParams::stealth_level` when
the key is absent. The other handlers ignore the stealth level.

---

## FASTQ

Handler: `handlers::genomics::GenomicsHandler`. Only 4-line records are
supported.

**Strategies** (`genomics_strategy`):

| Value | Bits per eligible quality character (`k`) |
|-------|-------------------------------------------|
| `conservative` | 1 |
| `balanced` | 2 |
| `aggressive` | 3 |
| `quality` or absent | from stealth: Low → aggressive, Medium → balanced, High/Maximum → conservative |

**What changes:** only the low `k` bits of quality characters. Headers,
sequences and `+` lines are never modified. A character `q` is eligible when,
with `b = q & !(2^k - 1)`, `b >= 36` (`$`) and `b + 2^k - 1 <= 126` (`~`).
Written characters stay inside `$`..`~`, so the output is valid Phred+33.
Q0 to Q2 (`!`, `"`, `#`; Illumina uses `#` as a quality-control flag) are
never touched or produced. Line endings, a missing final newline and a
trailing incomplete record are preserved byte for byte.

**Capacity:** `k × eligible characters` bits, minus the 72-bit `GNSJ` header.

**Visibility:** Each quality value changes by at most `2^k - 1`. That changes
downstream statistics and can matter to quality-aware tools. The `GNSJ` header
sits in the quality LSBs, where a reader that knows the scheme finds it
directly.

**Validated with:** Biopython `SeqIO.parse("fastq")` (LF and CRLF files).

**Streaming:** yes, all strategies.

## FASTA

Handler: `handlers::genomics::GenomicsHandler`.

**Strategy:** `header` (or absent).

**What changes:** FASTA has no redundant bits. The stream is hex-encoded and
split into chunks of at most 128 bytes (`FASTA_CHUNK_BYTES`). Chunk *i* is
appended to the *i*-th header line as ` stg=<hex>`. Sequence lines and their
wrapping are never touched.

**Capacity:** `header lines × 128` bytes, minus the 9-byte stream header.

**Visibility:** not covert. Anyone reading the headers sees the `stg=` suffix,
and tools that use the full header line as an identifier see changed IDs.

**Validated with:** Biopython `SeqIO.parse("fasta")`.

**Streaming:** no.

## VCF

Handler: `handlers::vcf::VcfHandler`. The file is treated as text. Only the
records and the one column a strategy writes to are rewritten. Every other
line, including meta lines and records past the end of the payload, is copied
byte for byte with its line ending.

**Strategies** (`vcf_strategy`, case-insensitive):

| Value (aliases) | Channel | Capacity |
|-----------------|---------|----------|
| `info` (`info_fields`), default | INFO key `STEG=<index>_<hex>` | 32 bytes per record |
| `format` (`sample`, `format_fields`) | FORMAT key `STEG`, value `<index>_<hex>` | 16 bytes per sample cell |
| `id` | extra ID item `STEG<index>_<hex>`, `;`-separated; the original ID is kept | 16 bytes per record |
| `quality` (`quality_precision`) | extra digits appended to QUAL | 8 bytes per record with a plain decimal QUAL |
| `multi` | the same stream in each of `info`, `format`, `id` that has room | at least one channel must hold it |

**What changes and visibility:**
- `info`/`format` add a `STEG` key and insert a matching
  `##INFO=<ID=STEG,...>` / `##FORMAT=<ID=STEG,...>` line before `#CHROM`, so
  `bcftools` accepts the file. The key, the header line and the hex value are
  all visible.
- `id` adds `STEG...` items to the ID column. These are visible and change
  variant IDs as seen by tools.
- `quality` keeps the original QUAL text and appends a guard `0` plus 24
  digits (`29.47` → `29.470…`). The numeric change is less than 0.1 of the last
  printed digit and rounds back to the original. The unusually long QUAL is
  visible. `.` and non-decimal QUAL values are left alone.
- `format` needs sample columns; it fails on a sites-only VCF.

**Validated with:** `bcftools view` (no new warnings) and `bcftools query`
(CHROM/POS/REF/ALT/GT and the sample list unchanged), or pysam when
`bcftools` is absent.

**Streaming:** embed with `info`, `id`, `quality`. Streaming extraction also
reads files written with `format` or `multi`.

## SAM and BAM

Handler: `handlers::sam_bam::SamBamHandler`. Parsing is strict: a malformed
line, record or BGZF block is an error, and records are never dropped. Lines
and records that carry no payload are written back unchanged.

**Strategies** (`sam_strategy`):

| Value (alias) | Where | Capacity |
|---------------|-------|----------|
| `optional` (`optional_fields`), default | one `zs:Z:<idx>,<hex>` tag per record, payload spread evenly | records × 255 bytes |
| `auxiliary` (`auxiliary_data`) | `zs:Z:<idx>,<hex>` tags in 255-byte chunks on as few records as possible | records × 255 bytes |
| `quality` (`quality_scores`) | LSB of QUAL values; records with QUAL `*` are skipped | quality values / 8 bytes |
| `comment` (`comment_lines`) | one `@CO\tzs:<hex>` header line | not bounded by the format |
| `multi` (`multi_strategy`) | the first of `optional`, `quality`, `comment` with enough room | |

`zs` is a lowercase tag, which the SAM specification reserves for end users,
so it does not collide with tags defined by the spec or by aligners.

**`bam_compression`** (BAM output only): `"bgzf"` (default) writes standard
BGZF blocks of at most 64 KiB followed by the 28-byte EOF block, readable by
`samtools` and htslib. `"none"` writes an uncompressed `BAM\1` stream. Input
may be either form, and extraction detects either. Other values are rejected.
The CLI has no flag for this; set it through `format_specific` in the library,
Go (`WithFormatSpecific`), Python or the C `format_specific_json`.

**Visibility:** `optional`, `auxiliary` and `comment` are plaintext-visible
tags or header lines (`samtools view` prints them). `quality` changes each
used quality value by at most 1. BAM output is recompressed, so the file
bytes differ even where records do not.

**Validated with:** `samtools quickcheck`, `samtools view -c` (record count
unchanged) and `samtools view` (no new stderr), for SAM and BAM, all
strategies.

**Streaming:** embed with `auxiliary` or `quality`. The default `optional`,
`comment` and `multi` need the whole file. Streaming extraction reads all
five.

## PNG and TIFF

Handler: `handlers::imagery::ImageryHandler`. The file is decoded to pixel
samples, payload bits go into the colour channels (never alpha), and the image
is re-encoded with the same dimensions, colour type and bit depth.

**Supported:** PNG Gray, GrayAlpha, RGB, RGBA at 8 or 16 bits. TIFF Gray
(BlackIsZero), RGB, RGBA at 8 or 16 bits. Rejected: palette images, sub-byte
depths, signed or float samples, planar or multi-page TIFF, CMYK/YCbCr/Lab,
APNG. Interlaced PNGs are accepted and written non-interlaced.

**Strategies** (`imagery_strategy`). `S` = width × height × colour channels.

| Value (aliases) | Stealth default | Capacity (bits) |
|-----------------|-----------------|-----------------|
| `lsb` (`simple`, `simplelsb`) | Low | `S`: 1 bit in the LSB of every colour sample |
| `adaptive` (`adaptivelsb`) | Medium | 1 bit per sample whose local variation (sum of absolute differences to 4 neighbours) exceeds a threshold |
| `edge` (`edgebased`) | High | 1 bit per colour sample of each pixel whose luma gradient exceeds a threshold |
| `dct` (`dctfrequency`) | Maximum | `height × colour channels × ⌊width / 8⌋ × 4`: QIM on DCT coefficients 2–5 of 8-sample horizontal blocks |

80 bits go to the `IMSJ` header. Eligibility is computed on LSB-masked
values, so the extractor sees the same positions and the capacity is exact.

**What changes:** sample values (±1 for the LSB strategies; small changes in
the DCT blocks for `dct`). PNG ancillary chunks (text, gamma, ICC, pHYs, ...)
are copied verbatim and only `IHDR`/`IDAT` are regenerated, so the compressed
stream differs from the original. TIFF keeps its compression scheme
(none/LZW/Deflate/PackBits) and predictor, but **other TIFF tags are not
preserved**.

**Visibility:** not visible to the eye. `lsb` at full capacity is reliably
found by the pairs-of-values and sample-pairs detectors. `adaptive` and
`edge` restrict the positions but are still LSB replacement.

**Validated with:** Pillow (the output loads with the same format, size, mode,
array shape and dtype) for PNG RGB8, PNG Gray16 and TIFF RGB8, all
strategies.

**Streaming:** no.

## DICOM

Handler: `handlers::dicom::DicomHandler`, with a lossless structural parser
for PS3.10 files (with or without the 128-byte preamble; implicit VR LE,
explicit VR LE and BE; encapsulated pixel data kept as is). Deflated transfer
syntaxes and malformed input are rejected. An unmodified parse and serialise
is byte-identical.

**Strategies** (`dicom_strategy`):

| Value (aliases) | Location | Capacity (frame bytes) |
|-----------------|----------|------------------------|
| `private` (`private_elements`), default | private block: creator `(gggg,00xx)` LO `STEGANO` + data `(gggg,xx10)` OB | 16 MiB (policy limit) |
| `metadata` (`unused_metadata`, `preamble`) | the 128-byte preamble | 128 (at most 119 codec bytes) |
| `sequence` (`padding`, `sequence_padding`) | `(FFFC,FFFC)` Data Set Trailing Padding, OB | 16 MiB (policy limit) |
| `pixel_lsb` (`pixel`) | LSB of each native pixel sample | samples / 8 |
| `multi` | `private` + `pixel_lsb` copies | the smaller of the two |

`dicom_private_group` (odd number in `0x0009..=0xFFFD`, default `0x0009`)
selects the private group. Every location holds the frame
`"STGD" | strategy | len u32 LE | codec bytes`.

**Visibility and constraints:**
- `private` adds an element whose creator is literally `STEGANO`. Any DICOM
  viewer that lists private tags shows it.
- `metadata` requires a Part 10 file with an all-zero preamble and fails if
  another application already uses it. The `STGD` frame is plain in the
  preamble.
- `sequence` adds a trailing padding element that tools usually ignore but can
  display.
- `pixel_lsb` changes pixel values by at most 1. It cannot write into
  encapsulated (compressed) pixel data. This changes clinical image data.

**Validated with:** pydicom `dcmread` without `force=True` (a conformant Part 10
file) and `pixel_array`, on a 16-bit monochrome CT, all strategies.

**Streaming:** no.

## FITS

Handler: `handlers::fits::FitsHandler`. Every HDU header is parsed and all
HDUs are walked. The payload goes into the first image HDU (primary or
`XTENSION = 'IMAGE'`, `PCOUNT = 0`, `GCOUNT = 1`, at least one pixel) whose
`BITPIX` suits the strategy.

**Strategies** (`fits_strategy`; CLI `--strategy`):

| Value (aliases) | Data | What changes |
|-----------------|------|--------------|
| `pixel_lsb` (`integer_lsb`, `lsb`), default | integer `BITPIX` 8/16/32/64 | LSB of each sample's last (least significant) byte: ±1 stored value, ±`BSCALE` physical. Samples equal to `BLANK` or `BLANK ^ 1` are skipped, so null pixels stay null. |
| `float_mantissa` | IEEE `BITPIX` -32/-64 | mantissa LSB only; sign and exponent never change; NaN and ±Inf skipped. A relative change of about 2⁻²³ (or 2⁻⁵²). Opt-in: embedding into float data without it is an error. |

Header blocks, data padding, other HDUs and trailing bytes are never modified.
The file length does not change.

**Capacity:** exactly 1 bit per usable sample in the target HDU, minus the
80-bit `FTSJ` header.

**Visibility:** not visible in headers. It is still a change to scientific
data, and pixel-LSB statistics apply.

**Validated with:** astropy `verify('exception')`, plus identical headers,
untouched other HDUs and unchanged file size, on a multi-HDU int16 file
(default strategy).

**Streaming:** yes, both strategies (`stream::fits`). Header blocks are read
one 2880-byte block at a time and copied through; other HDUs, padding and
trailing bytes are copied through; the target HDU's data is processed in
180 KiB chunks. Output is byte-identical to the in-memory handler's.
Extraction stops at the end of the payload. It takes the first integer or
float image HDU (in file order) with a matching header, where the in-memory
extractor prefers the integer HDU; the two differ only for a file with a
payload in both.

## WAV

Handler: `handlers::signals::SignalHandler`. RIFF/WAVE is parsed by walking
the chunk list. Supported: integer PCM at 8 (unsigned), 16, 24 and 32 bits, as
`WAVE_FORMAT_PCM` or `WAVE_FORMAT_EXTENSIBLE` (honouring
`wValidBitsPerSample`), and IEEE float at 32 or 64 bits with finite samples
only. Any channel count.

**What changes:** only bytes inside the `data` chunk payload. The RIFF
header, `fmt `, every other chunk, chunk order and trailing bytes are kept byte
for byte. The sample count never changes.

**Strategies** (`signal_strategy`), with SNR measured on a 16-bit multi-tone
plus noise test signal at 44.1 kHz, about 100 bytes of encrypted payload:

| Value (aliases) | Stealth default | Capacity (bits) | Measured SNR |
|-----------------|-----------------|-----------------|--------------|
| `timedomain` (`timedomainlsb`, `lsb`) | Low | 1 per sample, all channels | about 88 dB (≤ 1 LSB per sample) |
| `frequency` (`frequencydomain`) | Medium | 256 per 1024-frame chunk per channel | about 65 dB, upper quarter band only |
| `phase` (`phasemodulation`) | High | 128 per 1024-frame chunk per channel | about 50 dB, band fs/8 to fs/4 |
| `spread` (`spreadspectrum`) | Maximum | 1 per 512-frame chip per channel | about 29 dB (white noise) |
| `echo` (`echohiding`, `echo_hiding`) | not reachable from stealth | 1 per 1024-frame segment per channel | about 15 dB (audible echo) |

The 80-bit `SGSJ` header comes out of that capacity.

**Visibility:** `spread` adds audible noise and `echo` an audible echo at these
payload sizes. `timedomain` is LSB replacement. `spread` and `echo` derive
their pseudo-noise seed from the password with BLAKE3. Without a password that
seed is fixed and public.

**Robustness:** none of the strategies is claimed to survive lossy
compression (MP3, AAC, Opus), resampling, gain changes, filtering or
analogue conversion. Each embed re-extracts from the quantised output and
fails instead of writing a file it cannot read back. `echo` usually fails on
pure tones and on silence.

**Validated with:** Python's `wave` module and soundfile (frame count,
channels, sample width, rate and subtype unchanged) on 16-bit mono, 16-bit
stereo and 24-bit stereo files, all strategies.

**Streaming:** `timedomain` only (`stream::wav`). Chunks other than `data`
are copied through, and the RIFF and chunk size fields stay valid because the
file length does not change. `data` is processed in 256 KiB frame-aligned
buffers, byte-identical to the in-memory handler. `frequency` and `phase`
work per 1024-frame chunk, but their bit order is channel-major (all of
channel 0, then channel 1), so a single pass cannot place the bits for
multichannel audio. `spread` and `echo` have the same order and also
self-verify against the whole quantised output before writing anything.
These four are in-memory only. Stream extraction of a non-`timedomain`
payload returns the "not streamable" error and the CLI falls back to the
in-memory path. A `data` chunk before `fmt ` is in-memory only too.

## Raw signals

`DataFormat::BinarySignal` goes to the signal handler in raw mode: the bytes
are read as mono 16-bit little-endian samples with no header. The strategies
and capacities are as for [WAV](#wav), with the sample rate taken from
`carrier.sampling_rate` (default 44 100 Hz). Use it only for genuine 16-bit
sample streams. `CsvTimeseries` used to be routed here too, which corrupted
CSV files ([#97](https://github.com/scttfrdmn/stegano/issues/97)). It now has
its own handler.

**Validated with:** not covered by the format-validity suite.

**Streaming:** no.

## CSV

Handler: `handlers::csv::CsvHandler`, used for `DataFormat::CsvTimeseries`
(`.csv`).

**Parsing:** RFC 4180. Fields are separated by `,`, or by the single ASCII
character in `format_specific["csv_delimiter"]` (`tab` or `\t` for a tab).
Pass the same value to extract. Records end at `\n` or `\r\n`. A field that
starts with `"` is quoted, `""` is an escaped quote, and delimiters and line
breaks inside quotes are data. The carrier must be UTF-8 text without NUL
bytes. Anything else is an error.

**Strategy** (`csv_strategy`): `digit` is the only one (aliases `last_digit`,
`parity`). A cell is eligible if it is **unquoted** and a plain decimal number
whose mantissa ends in a digit: `12`, `-3.140`, `.5`, `1e-5`, `6.02E+23`, but
not `12.`, ` 12`, `"12"`, `nan` or `0x1F`. Each eligible cell carries one bit
in the parity of the last digit of its mantissa (the digit before any
exponent). Embedding flips that digit 0↔1, 2↔3, 4↔5, 6↔7 or 8↔9 as needed.

**Significant-digit threshold** (`csv_min_significant_digits`, an integer
from 1 to 17, default **2**): only cells whose mantissa has at least that many
significant digits are used. Significant digits run from the first non-zero
digit to the end of the mantissa: leading zeros do not count, trailing zeros
do, both after the decimal point and in integers (`0.70` → 2, `0.007` → 1,
`100` → 3, `6.02E+23` → 3). A mantissa of only zeros (`0`, `0.00`) counts as
1. With the default, single-digit cells such as `1e-5`, `3`, `0.7` and `0` are
never modified, so no value moves by more than 10% of itself. Threshold 1 uses
every numeric cell, which was the only behaviour before the option existed.
Embedding never changes a cell's count, and the threshold is stored in the
envelope, so extraction needs no parameter and still reads files written
before the option existed.

**Header:** the first non-blank record is a header if it has a non-empty cell
that is not eligible (a name, a quoted cell). A header is never modified, even
if some of its cells look numeric. An all-numeric first row is data.

**What changes:** only the last mantissa digit of eligible cells, so **numeric
values do change**, by at most one unit in their last printed place
(`12.34` → `12.35`, `-17` → `-16`; with threshold 1 also `-7` → `-6` and
`1e-5` → `0e-5`). Field widths never change.
Delimiters, quotes, quoted cells, non-numeric cells, the header, line endings
(per line) and the final-newline state stay byte-identical. Do not use this on
columns where a last-place change matters (identifiers, counts, checksums).

**Envelope:** `CSVJ`, a strategy byte, a 4-byte length and the codec frame.
The strategy byte is the significant-digit threshold (1–17). Files written
before the option existed always have 1, which is exactly threshold 1. The extractor runs one reader per
threshold over the same cells and keeps the one whose magic and strategy
byte match.

**Capacity:** exactly one bit per eligible cell, minus the 9-byte envelope
and the codec overhead. `estimate_capacity` reports the eligible cell count
and the largest unencrypted and encrypted payload, assuming the `,` delimiter
and the default threshold, plus
`eligible_cells_by_min_significant_digits` (element `t - 1` is the cell count
for threshold `t`; the largest codec frame is `(count - 72) / 8` bytes).
`InsufficientCapacity` is reported in bits.

**Visibility:** the file stays valid CSV with the same shape and text, but the
low digits are the payload, and about half of the cells used differ from the
cover. Anyone comparing against the original sees the changes, and the
last-digit distribution of a column may no longer look natural.

**Validated with:** Python's `csv` module (`strict=True`). The suite checks
that row and column counts are the same, the header and every non-numeric
cell are equal, cells with fewer than 2 significant digits are unchanged,
every numeric cell is within one last-place unit, and CRLF and
final-newline state are preserved.

**Streaming:** yes, byte-identical to the in-memory handler.

## Plain text

Handler: `handlers::text::TextHandler`, used for `DataFormat::TEXT` (`.txt`).

**Strategy** (`text_strategy`): `whitespace` is the only one (alias
`trailing_whitespace`). Each eligible line carries `k` bits as `k` trailing
characters, space = 0 and tab = 1, written before the line terminator. A line
is eligible unless it contains a bare carriage return (one that is not part of
`\r\n`). On every line it uses, the embedder first **removes existing
trailing spaces and tabs** and then writes exactly `k` characters. The
extractor reads the whole trailing run of each line, so whitespace that was
already in the cover cannot throw it off. Lines after the last one used are
left byte-identical, trailing whitespace included.

`k` is the smallest value in 1..=64 that fits the payload, unless
`format_specific["text_bits_per_line"]` fixes it. The envelope's strategy byte
records `k`, and extraction reads it from the file.

**What changes:** only trailing whitespace. Every line with trailing
whitespace stripped equals the cover's line stripped the same way. Line
endings (`\n`/`\r\n`, per line) and the final-newline state are kept, and an
ASCII file stays ASCII. The carrier must be UTF-8 text without NUL bytes.
Binary or non-UTF-8 input is an error.

**Capacity:** `64 × eligible lines` bits (`k × eligible lines` with a fixed
`k`), minus the 9-byte envelope (`TXTJ`, `k`, length) and the codec overhead.
`estimate_capacity` reports the eligible line count and the largest payloads.

**Visibility:** invisible in normal rendering, but plainly **visible in any
editor or diff that shows whitespace**. Anything that strips trailing
whitespace (editors on save, commit hooks, formatters) destroys the payload.

**Validated with:** UTF-8 decoding and line comparison. The suite checks that
the line count is the same, every line is equal after `rstrip()`, CRLF and
final-newline state are preserved, and no byte values other than space and
tab are introduced. It runs on an LF carrier and a CRLF carrier without a
final newline, both with existing trailing whitespace and Unicode lines.

**Streaming:** embedding streams only with a fixed `text_bits_per_line`,
because the automatic `k` needs the line count up front. The output is then
byte-identical to the in-memory handler with the same `k`. Extraction streams
for any `k`.

## Binary (generic)

Handler: `handlers::generic::GenericHandler`, used for `DataFormat::BINARY`
and any file whose format is not recognised.

**Strategy:** one only (no key): LSB replacement, 1 bit per carrier byte,
starting at the first byte, with a 32-bit length header before the codec
bytes.

**Capacity:** `file bytes − 32` bits.

**Visibility:** any byte may change, so text becomes garbled (`a` ↔ `` ` ``,
`,` ↔ `-`), and structured binary formats get corrupted headers because
embedding starts at byte 0. Use the format-specific handlers whenever one
exists. Name text files `.txt` or `.csv` so they get the text-aware handlers.

**Validated with:** not covered by the format-validity suite.

**Streaming:** no.

---

## Streaming

`stegano::stream` (and the CLI, C `stegano_embed_file`/`stegano_extract_file`,
Go `EmbedFile`/`ExtractFile` and Python `embed_file`/`extract_file`) streams
when `stream::is_streamable(format, params)` is true. Otherwise the CLI and
the bindings read the file into memory.

| Format | Streaming embed | Streaming extract |
|--------|-----------------|-------------------|
| FASTQ | all strategies | yes |
| VCF | `info` (default), `id`, `quality` | yes, any strategy |
| SAM | `auxiliary`, `quality` | yes, any strategy |
| BAM (BGZF or raw `BAM\1` input; `bam_compression` honoured) | `auxiliary`, `quality` | yes, any strategy |
| CSV (`csv_delimiter` honoured) | yes | yes |
| Text | only with a fixed `text_bits_per_line` | yes, any `k` |
| FITS | `pixel_lsb`, `float_mantissa` | yes, either strategy |
| WAV | `timedomain` only | `timedomain` payloads; others return the "not streamable" error (the CLI falls back to in-memory) |
| FASTA, PNG, TIFF, DICOM, raw signals, binary | no | no |

Memory use is bounded by one record (FASTQ, VCF, SAM), one line (CSV, text),
one BGZF block plus the largest record (BAM), or one header block or
180 KiB data chunk (FITS) or 256 KiB of frames (WAV), plus the encoded payload and Argon2id's 64 MiB.
CHANGELOG 0.7.0 reports about 70 MB peak RSS when embedding into or
extracting from 1–2 GB FASTQ, VCF and BAM files. Extraction stops reading once
the payload frame is complete. Streaming output is byte-identical to the
in-memory handlers' for the same encoded payload, and each API reads the
other's files.

Capacity is only known at end of file. `embed_stream` writes the whole carrier
and then returns `InsufficientCapacity`. Discard the output on any error.
`embed_file` and the CLI write to a temporary file and rename it only on
success.

## Selecting a strategy in code

```rust
use stegano::serde_json::json;
use stegano::EmbedParams;

let mut params = EmbedParams::new(b"correct horse battery staple".to_vec());
params.format_specific.insert("sam_strategy".into(), json!("auxiliary"));
params.format_specific.insert("bam_compression".into(), json!("none"));
params.format_specific.insert("fits_strategy".into(), json!("float_mantissa"));
params.format_specific.insert("dicom_private_group".into(), json!(0x0011));
assert_eq!(params.format_specific.len(), 4);
```

Each handler reads only its own keys, so one `EmbedParams` can hold keys for
several formats.
