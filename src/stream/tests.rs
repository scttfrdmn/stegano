use super::*;
use crate::codec::encode_payload;
use crate::core::{CarrierData, ECLevel, StealthLevel};
use crate::handlers::genomics::{GenomicsHandler, STRATEGY_KEY};
use crate::handlers::DataHandler;
use std::io::{Cursor, Read};

fn params(password: &[u8], strategy: Option<&str>) -> EmbedParams {
    let mut p = EmbedParams {
        encryption_key: password.to_vec(),
        error_correction_level: ECLevel::None,
        stealth_level: StealthLevel::Medium,
        ..Default::default()
    };
    if let Some(s) = strategy {
        p.format_specific
            .insert(STRATEGY_KEY.to_string(), serde_json::Value::from(s));
    }
    p
}

fn lcg(seed: &mut u64) -> u64 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *seed >> 33
}

/// One synthetic record; Illumina-style trailing '#' runs on every third read.
fn synth_record(seed: &mut u64, r: usize, len: usize, alphabet: &[u8], eol: &str) -> Vec<u8> {
    let seq: Vec<u8> = (0..len)
        .map(|_| b"ACGTN"[(lcg(seed) % 5) as usize])
        .collect();
    let mut qual: Vec<u8> = (0..len)
        .map(|_| alphabet[(lcg(seed) % alphabet.len() as u64) as usize])
        .collect();
    if r.is_multiple_of(3) {
        for q in qual.iter_mut().skip(len * 2 / 3) {
            *q = b'#';
        }
    }
    let mut out = format!("@read{r} 1:N:0:ACGT{eol}").into_bytes();
    out.extend_from_slice(&seq);
    out.extend_from_slice(eol.as_bytes());
    out.extend_from_slice(b"+");
    out.extend_from_slice(eol.as_bytes());
    out.extend_from_slice(&qual);
    out.extend_from_slice(eol.as_bytes());
    out
}

fn synth_fastq(records: usize, len: usize, alphabet: &[u8], eol: &str) -> Vec<u8> {
    let mut seed = 7u64;
    (0..records)
        .flat_map(|r| synth_record(&mut seed, r, len, alphabet, eol))
        .collect()
}

const ILLUMINA: &[u8] = b"!!!###\"#$%&'()*+5?IJ";
const STRATEGIES: [&str; 3] = ["conservative", "balanced", "aggressive"];

fn stream_embed_encoded(data: &[u8], encoded: &[u8], p: &EmbedParams) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    embed_encoded_stream(
        &DataFormat::FASTQ,
        &mut Cursor::new(data),
        &mut out,
        encoded,
        p,
    )?;
    Ok(out)
}

fn carriers() -> Vec<(&'static str, Vec<u8>)> {
    let mut partial = synth_fastq(40, 90, ILLUMINA, "\r\n");
    partial.extend_from_slice(b"@partial\r\nACGT");
    let mut no_final_nl = synth_fastq(40, 90, b"IJF:,#", "\n");
    no_final_nl.pop();
    let mut blank_tail = synth_fastq(40, 90, b"FFFF::,#", "\n");
    blank_tail.extend_from_slice(b"\n\r\n\n");
    vec![
        ("lf", synth_fastq(40, 90, b"#,:FFFFIJ<AE", "\n")),
        ("crlf", synth_fastq(40, 90, b"#,:FFFFIJ<AE", "\r\n")),
        ("illumina-lf", synth_fastq(60, 150, ILLUMINA, "\n")),
        ("illumina-crlf-partial", partial),
        ("no-final-newline", no_final_nl),
        ("blank-tail", blank_tail),
    ]
}

#[test]
fn test_interop_byte_identical_with_in_memory() {
    let handler = GenomicsHandler::new();
    for (name, orig) in carriers() {
        for s in STRATEGIES {
            for pw in [&b""[..], b"pw"] {
                let p = params(pw, Some(s));
                let strategy = GenomicsHandler::fastq_strategy(&p).unwrap();
                for n in [0usize, 1, 37, 120] {
                    let payload: Vec<u8> = (0..n as u32).map(|i| (i * 31 % 251) as u8).collect();
                    // Same encoded bytes for both paths (Argon2 once).
                    let encoded = encode_payload(&payload, &p).unwrap();
                    let mut mem = orig.clone();
                    handler
                        .embed_fastq_qualities(&mut mem, &encoded, strategy)
                        .unwrap();
                    let streamed = stream_embed_encoded(&orig, &encoded, &p).unwrap();
                    assert_eq!(streamed, mem, "{name}/{s}/pw={}/n={n}", !pw.is_empty());
                    assert_ne!(streamed, orig);
                }
            }
        }
    }
}

#[tokio::test]
async fn test_stream_embed_then_in_memory_extract() {
    let handler = GenomicsHandler::new();
    for (name, orig) in carriers() {
        for pw in [&b""[..], b"hunter2"] {
            let p = params(pw, None);
            let payload = format!("secret for {name}").into_bytes();
            let mut out = Vec::new();
            let res = embed_stream(
                DataFormat::FASTQ,
                Cursor::new(&orig),
                &mut out,
                &payload,
                &p,
            )
            .unwrap();
            assert_eq!(res.bytes_embedded, payload.len());
            assert_eq!(out.len(), orig.len(), "{name}");
            let c = CarrierData::new(DataFormat::FASTQ, out.clone());
            assert_eq!(handler.extract(&c, &p).await.unwrap(), payload, "{name}");
            // And back through the stream path.
            let got = extract_stream(DataFormat::FASTQ, Cursor::new(&out), &p).unwrap();
            assert_eq!(got, payload);
        }
    }
}

#[tokio::test]
async fn test_in_memory_embed_then_stream_extract() {
    let handler = GenomicsHandler::new();
    for (name, orig) in carriers() {
        for s in STRATEGIES {
            for pw in [&b""[..], b"hunter2"] {
                let p = params(pw, Some(s));
                let payload = format!("{name}/{s}").repeat(3).into_bytes();
                let mut c = CarrierData::new(DataFormat::FASTQ, orig.clone());
                handler.embed(&mut c, &payload, &p).await.unwrap();
                // Extraction ignores the strategy key (auto-detected).
                let p2 = params(pw, None);
                let got = extract_stream(DataFormat::FASTQ, Cursor::new(&c.raw_data), &p2)
                    .unwrap_or_else(|e| panic!("{name}/{s}: {e}"));
                assert_eq!(got, payload, "{name}/{s}");
            }
        }
    }
}

#[test]
fn test_ec_levels_and_wrong_password() {
    let orig = synth_fastq(200, 100, b"#,:FFFFIJ<AE", "\n");
    for level in [ECLevel::Low, ECLevel::Maximum] {
        let mut p = params(b"pw", Some("aggressive"));
        p.error_correction_level = level;
        let mut out = Vec::new();
        embed_stream(
            DataFormat::FASTQ,
            Cursor::new(&orig),
            &mut out,
            b"ec payload",
            &p,
        )
        .unwrap();
        // EC level is auto-detected on extract.
        let got = extract_stream(DataFormat::FASTQ, Cursor::new(&out), &params(b"pw", None));
        assert_eq!(got.unwrap(), b"ec payload");
        let bad = extract_stream(DataFormat::FASTQ, Cursor::new(&out), &params(b"nope", None));
        assert!(bad.is_err());
    }
}

/// Reader adapter counting the bytes handed out.
struct Counting<R> {
    inner: R,
    count: u64,
}

impl<R: Read> Read for Counting<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.count += n as u64;
        Ok(n)
    }
}

#[test]
fn test_extract_stops_early() {
    let orig = synth_fastq(5000, 100, b"#,:FFFFIJ<AE", "\n");
    let p = params(b"", Some("balanced"));
    let mut out = Vec::new();
    embed_stream(
        DataFormat::FASTQ,
        Cursor::new(&orig),
        &mut out,
        b"early",
        &p,
    )
    .unwrap();
    let (raw, consumed) = fastq::extract(&mut Cursor::new(&out)).unwrap();
    assert_eq!(crate::codec::decode_payload(&raw, &p).unwrap(), b"early");
    assert!(consumed < 2000, "consumed {consumed} of {}", out.len());
    // Through the public API with a small BufReader over a counting reader.
    let mut counting = Counting {
        inner: Cursor::new(&out),
        count: 0,
    };
    let got = extract_stream(
        DataFormat::FASTQ,
        BufReader::with_capacity(512, &mut counting),
        &p,
    )
    .unwrap();
    assert_eq!(got, b"early");
    assert!(counting.count < 4096, "read {} bytes", counting.count);
}

#[tokio::test]
async fn test_insufficient_capacity_matches_in_memory() {
    let handler = GenomicsHandler::new();
    let orig = synth_fastq(10, 50, b"#!FIJ:,", "\n");
    let p = params(b"", Some("balanced"));
    let payload = vec![0u8; 400];
    let mut c = CarrierData::new(DataFormat::FASTQ, orig.clone());
    let mem_err = handler.embed(&mut c, &payload, &p).await.unwrap_err();
    let mut out = Vec::new();
    let err = embed_stream(
        DataFormat::FASTQ,
        Cursor::new(&orig),
        &mut out,
        &payload,
        &p,
    )
    .unwrap_err();
    match (&err, &mem_err) {
        (
            SteganoError::InsufficientCapacity { needed, available },
            SteganoError::InsufficientCapacity {
                needed: n2,
                available: a2,
            },
        ) => assert_eq!((needed, available), (n2, a2)),
        other => panic!("unexpected {other:?}"),
    }
    // Documented behaviour: the whole (incomplete) output was written.
    assert_eq!(out.len(), orig.len());
}

#[tokio::test]
async fn test_structural_errors_match_in_memory() {
    let handler = GenomicsHandler::new();
    let good = synth_fastq(3, 20, b"IJF", "\n");
    let cases: Vec<Vec<u8>> = vec![
        b"".to_vec(),
        b"\n\n".to_vec(),
        b"@only\nACGT\n".to_vec(),
        b"not fastq\n".to_vec(),
        [&good[..], b"\n@late\nAC\n+\nII\n"].concat(),
        [&good[..], b"@bad\nACGT\n-\nIIII\n"].concat(),
        [&good[..], b"@bad\nACGT\n+\nIII\n"].concat(),
        [&good[..], b"garbage\n"].concat(),
    ];
    let p = params(b"", None);
    for data in cases {
        let mut c = CarrierData::new(DataFormat::FASTQ, data.clone());
        let mem = handler.embed(&mut c, b"x", &p).await.unwrap_err();
        let st =
            embed_stream(DataFormat::FASTQ, Cursor::new(&data), Vec::new(), b"x", &p).unwrap_err();
        assert_eq!(
            st.to_string(),
            mem.to_string(),
            "{:?}",
            String::from_utf8_lossy(&data)
        );
    }
}

#[test]
fn test_no_payload_and_truncated() {
    let orig = synth_fastq(100, 100, b"IJF", "\n");
    let p = params(b"", None);
    assert!(matches!(
        extract_stream(DataFormat::FASTQ, Cursor::new(&orig), &p),
        Err(SteganoError::IntegrityError(_))
    ));
    let mut out = Vec::new();
    embed_stream(
        DataFormat::FASTQ,
        Cursor::new(&orig),
        &mut out,
        &[9u8; 500],
        &p,
    )
    .unwrap();
    // Cut the carrier after the first record: the frame cannot complete.
    let cut = out
        .iter()
        .enumerate()
        .filter(|(_, &b)| b == b'\n')
        .nth(3)
        .unwrap()
        .0
        + 1;
    let err = extract_stream(DataFormat::FASTQ, Cursor::new(&out[..cut]), &p).unwrap_err();
    assert!(err.to_string().contains("ended before"), "{err}");
}

#[test]
fn test_unsupported_format_and_strategy() {
    let p = params(b"", None);
    assert!(is_streamable(&DataFormat::FASTQ, &p));
    assert!(is_streamable(
        &DataFormat::FASTQ,
        &params(b"", Some("quality"))
    ));
    assert!(!is_streamable(
        &DataFormat::FASTQ,
        &params(b"", Some("bogus"))
    ));
    for f in [DataFormat::FASTA, DataFormat::PNG, DataFormat::DICOM] {
        assert!(!is_streamable(&f, &p));
        let err = embed_stream(f.clone(), Cursor::new(b""), Vec::new(), b"x", &p).unwrap_err();
        match err {
            SteganoError::InvalidParameter(m) => {
                assert_eq!(
                    m,
                    format!("{f:?}/default is not streamable; use the in-memory API")
                )
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            extract_stream(f, Cursor::new(b""), &p),
            Err(SteganoError::InvalidParameter(_))
        ));
    }
    let mut sp = p.clone();
    sp.format_specific
        .insert("vcf_strategy".into(), serde_json::Value::from("format"));
    let err = embed_stream(DataFormat::VCF, Cursor::new(b""), Vec::new(), b"x", &sp).unwrap_err();
    assert!(
        err.to_string().contains("VCF/format is not streamable"),
        "{err}"
    );
}

#[test]
fn test_embed_file_atomic() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.fastq");
    let output = dir.path().join("out.fastq");
    let orig = synth_fastq(50, 80, b"#,:FFFFIJ<AE", "\n");
    std::fs::write(&input, &orig).unwrap();
    let p = params(b"pw", None);

    embed_file(DataFormat::FASTQ, &input, &output, b"file payload", &p).unwrap();
    assert_eq!(
        extract_file(DataFormat::FASTQ, &output, &p).unwrap(),
        b"file payload"
    );

    // Failure (capacity) leaves the existing output untouched, no temp files.
    let before = std::fs::read(&output).unwrap();
    let err = embed_file(DataFormat::FASTQ, &input, &output, &[0u8; 100_000], &p).unwrap_err();
    assert!(matches!(err, SteganoError::InsufficientCapacity { .. }));
    assert_eq!(std::fs::read(&output).unwrap(), before);
    let names: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names.len(), 2, "{names:?}");

    // In place.
    embed_file(DataFormat::FASTQ, &input, &input, b"in place", &p).unwrap();
    assert_eq!(
        extract_file(DataFormat::FASTQ, &input, &p).unwrap(),
        b"in place"
    );
}

/// Lazily generated FASTQ (never held in memory as a whole).
struct GenReader {
    seed: u64,
    record: usize,
    records: usize,
    buf: Vec<u8>,
    pos: usize,
}

impl GenReader {
    fn new(records: usize) -> Self {
        Self {
            seed: 99,
            record: 0,
            records,
            buf: Vec::new(),
            pos: 0,
        }
    }
}

impl Read for GenReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.pos == self.buf.len() {
            if self.record == self.records {
                return Ok(0);
            }
            self.buf = synth_record(&mut self.seed, self.record, 150, ILLUMINA, "\n");
            self.record += 1;
            self.pos = 0;
        }
        let n = out.len().min(self.buf.len() - self.pos);
        out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

/// Writer that keeps only a prefix and counts the rest.
struct PrefixSink {
    prefix: Vec<u8>,
    keep: usize,
    total: u64,
}

impl Write for PrefixSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let room = self.keep.saturating_sub(self.prefix.len()).min(buf.len());
        self.prefix.extend_from_slice(&buf[..room]);
        self.total += buf.len() as u64;
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn test_bounded_memory_50mb() {
    // ~340 bytes per record -> ~50 MB.
    let records = 150_000;
    let p = params(b"", Some("conservative"));
    let payload = vec![0xC3u8; 4096];
    let encoded = encode_payload(&payload, &p).unwrap();
    let mut reader = BufReader::with_capacity(64 * 1024, GenReader::new(records));
    let mut sink = PrefixSink {
        prefix: Vec::new(),
        keep: 1 << 20,
        total: 0,
    };
    let stats =
        embed_encoded_stream(&DataFormat::FASTQ, &mut reader, &mut sink, &encoded, &p).unwrap();
    assert!(stats.bytes_read > 45_000_000, "{}", stats.bytes_read);
    assert_eq!(stats.bytes_read, stats.bytes_written);
    assert_eq!(sink.total, stats.bytes_written);
    // Line buffers (one record) + the 4 KiB envelope; independent of size.
    assert!(
        stats.peak_buffer_bytes < 16 * 1024,
        "peak {}",
        stats.peak_buffer_bytes
    );
    // The payload lives in the first records, so the 1 MiB prefix suffices.
    let (raw, consumed) = fastq::extract(&mut Cursor::new(&sink.prefix)).unwrap();
    assert_eq!(crate::codec::decode_payload(&raw, &p).unwrap(), payload);
    assert!(consumed < 200_000, "{consumed}");
}

/// Manual large-file check (not run in CI):
///
/// ```text
/// STEGANO_BIG_FASTQ=/tmp/big.fastq cargo test --release --lib \
///     stream::tests::big_fastq_manual -- --ignored --nocapture
/// ```
#[test]
#[ignore]
fn big_fastq_manual() {
    let Ok(path) = std::env::var("STEGANO_BIG_FASTQ") else {
        eprintln!("STEGANO_BIG_FASTQ not set; skipping");
        return;
    };
    let out = format!("{path}.stego");
    let p = params(b"big password", None);
    let payload = vec![0x42u8; 16 * 1024];
    let t = std::time::Instant::now();
    let res = embed_file(DataFormat::FASTQ, &path, &out, &payload, &p).unwrap();
    let in_len = std::fs::metadata(&path).unwrap().len();
    eprintln!(
        "embed: {in_len} bytes in {:?} (reported memory {} bytes)",
        t.elapsed(),
        res.performance_stats.memory_usage_bytes
    );
    let t = std::time::Instant::now();
    let mut counting = Counting {
        inner: File::open(&out).unwrap(),
        count: 0,
    };
    let got = extract_stream(
        DataFormat::FASTQ,
        BufReader::with_capacity(FILE_BUF, &mut counting),
        &p,
    )
    .unwrap();
    assert_eq!(got, payload);
    eprintln!(
        "extract: read {} of {in_len} bytes in {:?}",
        counting.count,
        t.elapsed()
    );
    std::fs::remove_file(&out).unwrap();
}

// ---------------------------------------------------------------------------
// CSV and plain text
// ---------------------------------------------------------------------------

/// CSV with a quoted header, quoted cells (commas, escaped quotes, an
/// embedded line break), negative/exponent numbers; `eol` endings, optional
/// final newline.
fn synth_csv(rows: usize, eol: &str, final_eol: bool) -> Vec<u8> {
    let mut s = format!("t,\"a, b\",v,note{eol}");
    for i in 0..rows {
        let note = match i % 3 {
            0 => "\"x \"\"y\"\", z\"".to_string(),
            1 => format!("\"two{eol}lines\""),
            _ => "text".to_string(),
        };
        s.push_str(&format!(
            "{i},-{}.{:02},{}e-{},{note}",
            i % 9,
            i % 100,
            i % 7 + 1,
            i % 4
        ));
        if i + 1 < rows || final_eol {
            s.push_str(eol);
        }
    }
    s.into_bytes()
}

/// Text with existing trailing whitespace, a Unicode line and blank lines.
fn synth_text(lines: usize, eol: &str, final_eol: bool) -> Vec<u8> {
    let mut s = String::new();
    for i in 0..lines {
        s.push_str(match i % 4 {
            0 => "plain line",
            1 => "trailing  \t",
            2 => "ünïcode ✓",
            _ => "",
        });
        if i + 1 < lines || final_eol {
            s.push_str(eol);
        }
    }
    s.into_bytes()
}

fn with_key(mut p: EmbedParams, key: &str, v: serde_json::Value) -> EmbedParams {
    p.format_specific.insert(key.to_string(), v);
    p
}

fn stream_embed(
    format: DataFormat,
    data: &[u8],
    encoded: &[u8],
    p: &EmbedParams,
) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    embed_encoded_stream(&format, &mut Cursor::new(data), &mut out, encoded, p)?;
    Ok(out)
}

#[tokio::test]
async fn test_csv_stream_interop() {
    use crate::handlers::csv::{embed_encoded, CsvHandler};
    let carriers = [
        synth_csv(300, "\r\n", false),
        synth_csv(300, "\n", true),
        synth_csv(300, "\r\n", true),
    ];
    for orig in &carriers {
        for pw in [&b""[..], b"pw"] {
            let p = params(pw, None);
            for n in [0usize, 1, 40] {
                let payload: Vec<u8> = (0..n as u8).collect();
                let encoded = encode_payload(&payload, &p).unwrap();
                let mem = embed_encoded(orig, &encoded, b',').unwrap();
                let streamed =
                    stream_embed(DataFormat::CsvTimeseries, orig, &encoded, &p).unwrap();
                assert_eq!(streamed, mem);
                assert_eq!(
                    extract_stream(DataFormat::CsvTimeseries, Cursor::new(&mem), &p).unwrap(),
                    payload
                );
                let c = CarrierData::new(DataFormat::CsvTimeseries, streamed);
                assert_eq!(CsvHandler.extract(&c, &p).await.unwrap(), payload);
            }
        }
    }
    // A custom delimiter is honoured on both paths.
    let semi: Vec<u8> = String::from_utf8(synth_csv(300, "\n", true))
        .unwrap()
        .replace(',', ";")
        .into_bytes();
    let p = with_key(params(b"", None), "csv_delimiter", ";".into());
    let encoded = encode_payload(b"semi", &p).unwrap();
    let streamed = stream_embed(DataFormat::CsvTimeseries, &semi, &encoded, &p).unwrap();
    assert_eq!(streamed, embed_encoded(&semi, &encoded, b';').unwrap());
    assert_eq!(
        extract_stream(DataFormat::CsvTimeseries, Cursor::new(&streamed), &p).unwrap(),
        b"semi"
    );
}

#[test]
fn test_csv_stream_errors_match_in_memory() {
    use crate::handlers::csv::embed_encoded;
    let p = params(b"", None);
    let small = synth_csv(20, "\n", true);
    let encoded = encode_payload(&[0u8; 64], &p).unwrap();
    let mem = embed_encoded(&small, &encoded, b',').unwrap_err();
    let st = stream_embed(DataFormat::CsvTimeseries, &small, &encoded, &p).unwrap_err();
    assert!(matches!(mem, SteganoError::InsufficientCapacity { .. }));
    assert_eq!(mem.to_string(), st.to_string());
    let mut bin = synth_csv(300, "\n", true);
    bin.extend_from_slice(b"1,2\0\n");
    let mem = embed_encoded(&bin, &encoded, b',').unwrap_err();
    let st = stream_embed(DataFormat::CsvTimeseries, &bin, &encoded, &p).unwrap_err();
    assert_eq!(mem.to_string(), st.to_string());
    assert!(extract_stream(DataFormat::CsvTimeseries, Cursor::new(&small), &p).is_err());
    assert!(extract_stream(DataFormat::CsvTimeseries, Cursor::new(b"\xff\x00"), &p).is_err());
    let bad = with_key(p, "csv_strategy", "bogus".into());
    assert!(!is_streamable(&DataFormat::CsvTimeseries, &bad));
}

#[tokio::test]
async fn test_text_stream_interop() {
    use crate::handlers::text::{embed_encoded, TextHandler};
    let p0 = params(b"", None);
    assert!(is_streamable(&DataFormat::CsvTimeseries, &p0));
    // The automatic bits-per-line choice is not streamable.
    assert!(!is_streamable(&DataFormat::TEXT, &p0));
    let err = embed_stream(DataFormat::TEXT, Cursor::new(b""), Vec::new(), b"x", &p0).unwrap_err();
    assert!(err.to_string().contains("is not streamable"), "{err}");

    let carriers = [synth_text(400, "\n", true), synth_text(400, "\r\n", false)];
    for orig in &carriers {
        for pw in [&b""[..], b"pw"] {
            for k in [2usize, 3, 64] {
                let p = with_key(params(pw, None), "text_bits_per_line", k.into());
                assert!(is_streamable(&DataFormat::TEXT, &p));
                let payload = b"text payload".to_vec();
                let encoded = encode_payload(&payload, &p).unwrap();
                let mem = embed_encoded(orig, &encoded, Some(k)).unwrap();
                let streamed = stream_embed(DataFormat::TEXT, orig, &encoded, &p).unwrap();
                assert_eq!(streamed, mem, "k={k}");
                let c = CarrierData::new(DataFormat::TEXT, streamed);
                assert_eq!(TextHandler.extract(&c, &p).await.unwrap(), payload);
            }
            // Stream extraction reads files embedded with the automatic k.
            let p = params(pw, None);
            let mut c = CarrierData::new(DataFormat::TEXT, orig.clone());
            TextHandler.embed(&mut c, b"auto", &p).await.unwrap();
            assert_eq!(
                extract_stream(DataFormat::TEXT, Cursor::new(&c.raw_data), &p).unwrap(),
                b"auto"
            );
        }
    }
}

#[test]
fn test_text_stream_errors_match_in_memory() {
    use crate::handlers::text::embed_encoded;
    let p = with_key(params(b"", None), "text_bits_per_line", 2.into());
    let small = synth_text(30, "\n", true);
    let encoded = encode_payload(b"too big for thirty lines", &p).unwrap();
    let mem = embed_encoded(&small, &encoded, Some(2)).unwrap_err();
    let st = stream_embed(DataFormat::TEXT, &small, &encoded, &p).unwrap_err();
    assert!(matches!(mem, SteganoError::InsufficientCapacity { .. }));
    assert_eq!(mem.to_string(), st.to_string());
    let mut bin = synth_text(400, "\n", true);
    bin.extend_from_slice(b"caf\xe9\n");
    let mem = embed_encoded(&bin, &encoded, Some(2)).unwrap_err();
    let st = stream_embed(DataFormat::TEXT, &bin, &encoded, &p).unwrap_err();
    assert_eq!(mem.to_string(), st.to_string());
    for g in [&b""[..], b"a \n", b"x\t\n\n", b"\x00\x01"] {
        assert!(extract_stream(DataFormat::TEXT, Cursor::new(g), &p).is_err());
    }
}
