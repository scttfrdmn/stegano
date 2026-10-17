use super::*;
use crate::codec::{decode_payload, encode_payload};
use crate::core::{CarrierData, ECLevel};
use crate::handlers::sam_bam::tests::{
    bam_fixture, embed_codec, noodles_bam, noodles_raw_bam, noodles_sam, params, sam_fixture,
    sam_to_raw_bam, with_compression,
};
use crate::handlers::sam_bam::{bgzf_write_block, SamBamHandler, BGZF_EOF};
use crate::handlers::DataHandler;
use crate::stream::{
    embed_encoded_stream, embed_file, embed_stream, extract_file, extract_stream, is_streamable,
};
use std::io::{BufReader, Cursor, Read};

const STRATEGIES: [&str; 2] = ["auxiliary", "quality"];

fn p(strategy: Option<&str>, pw: &[u8]) -> EmbedParams {
    params(strategy, pw, ECLevel::None)
}

/// BGZF with small (1000-byte) blocks, so input block boundaries differ
/// from the 0xff00-byte blocks both writers produce.
fn small_block_bgzf(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for c in raw.chunks(1000) {
        bgzf_write_block(c, &mut out).unwrap();
    }
    out.extend_from_slice(&BGZF_EOF);
    out
}

fn crlf_sam() -> Vec<u8> {
    let mut s = String::from_utf8(sam_fixture(200))
        .unwrap()
        .replace('\n', "\r\n");
    s.pop(); // no final newline
    s.into_bytes()
}

fn blank_lines_sam() -> Vec<u8> {
    let mut s = String::from_utf8(sam_fixture(200)).unwrap();
    s = s.replacen("\nr003", "\n\nr003", 3);
    s.push_str("\n\r\n");
    s.into_bytes()
}

/// (name, format, carrier).
fn carriers() -> Vec<(&'static str, DataFormat, Vec<u8>)> {
    vec![
        ("sam", DataFormat::SAM, sam_fixture(200)),
        ("sam-crlf", DataFormat::SAM, crlf_sam()),
        ("sam-blank", DataFormat::SAM, blank_lines_sam()),
        ("sam-tiny", DataFormat::SAM, sam_fixture(1)),
        ("bgzf", DataFormat::BAM, bam_fixture(200)),
        ("bgzf-multiblock", DataFormat::BAM, bam_fixture(1500)),
        (
            "bgzf-smallblocks",
            DataFormat::BAM,
            small_block_bgzf(&sam_to_raw_bam(&sam_fixture(300))),
        ),
        ("bgzf-tiny", DataFormat::BAM, bam_fixture(1)),
        ("raw", DataFormat::BAM, sam_to_raw_bam(&sam_fixture(200))),
        ("raw-tiny", DataFormat::BAM, sam_to_raw_bam(&sam_fixture(1))),
    ]
}

/// Parameter variants: strategy x password x (BAM) bam_compression.
fn variants(format: &DataFormat) -> Vec<EmbedParams> {
    let comps: &[Option<&str>] = if *format == DataFormat::BAM {
        &[None, Some("bgzf"), Some("none")]
    } else {
        &[None]
    };
    let mut out = Vec::new();
    for s in STRATEGIES {
        for pw in [&b""[..], b"pw"] {
            for c in comps {
                let base = p(Some(s), pw);
                out.push(match c {
                    Some(c) => with_compression(base, c),
                    None => base,
                });
            }
        }
    }
    out
}

fn stream_embed_encoded(
    format: &DataFormat,
    data: &[u8],
    encoded: &[u8],
    params: &EmbedParams,
) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    embed_encoded_stream(format, &mut Cursor::new(data), &mut out, encoded, params)?;
    Ok(out)
}

fn label(name: &str, params: &EmbedParams, n: usize) -> String {
    format!(
        "{name}/{:?}/pw={}/n={n}",
        params.format_specific,
        !params.encryption_key.is_empty()
    )
}

#[test]
fn test_interop_byte_identical_with_in_memory() {
    let mut ok = 0;
    for (name, format, data) in carriers() {
        for params in variants(&format) {
            for n in [0usize, 1, 37, 600] {
                let payload: Vec<u8> = (0..n as u32).map(|i| (i * 31 % 251) as u8).collect();
                let encoded = encode_payload(&payload, &params).unwrap();
                let mem = embed_codec(&format, &data, &encoded, &params).map_err(|e| e.to_string());
                let st = stream_embed_encoded(&format, &data, &encoded, &params)
                    .map_err(|e| e.to_string());
                assert_eq!(st, mem, "{}", label(name, &params, n));
                ok += usize::from(st.is_ok());
            }
        }
    }
    assert!(ok > 200, "{ok}");
}

/// noodles rejects CRLF line endings and blank lines, both of which the
/// streaming reader tolerates (and preserves); normalise before validating.
fn lf_only(sam: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(sam.len());
    for line in sam.split(|&b| b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if !line.is_empty() {
            out.extend_from_slice(line);
            out.push(b'\n');
        }
    }
    out
}

/// Validate an output with noodles; returns the record count.
fn noodles_records(format: &DataFormat, out: &[u8]) -> usize {
    match format {
        DataFormat::SAM => noodles_sam(&lf_only(out)).1.len(),
        _ if out.starts_with(&BAM_MAGIC) => noodles_raw_bam(out).1.len(),
        _ => noodles_bam(out).1.len(),
    }
}

#[tokio::test]
async fn test_stream_embed_then_in_memory_extract_and_back() {
    let handler = SamBamHandler::new();
    for (name, format, data) in carriers() {
        if name.ends_with("tiny") {
            continue;
        }
        let in_records = noodles_records(&format, &data);
        for params in variants(&format) {
            let payload = format!("secret for {name}").into_bytes();
            let mut out = Vec::new();
            let res = embed_stream(
                format.clone(),
                Cursor::new(&data),
                &mut out,
                &payload,
                &params,
            )
            .unwrap();
            assert_eq!(res.bytes_embedded, payload.len());
            let what = label(name, &params, payload.len());
            assert_eq!(noodles_records(&format, &out), in_records, "{what}");
            let none = params.format_specific.get("bam_compression")
                == Some(&serde_json::Value::from("none"));
            if format == DataFormat::BAM {
                assert_eq!(out.starts_with(&BAM_MAGIC), none, "{what}");
            }
            // Extraction needs no strategy.
            let mut q = params.clone();
            q.format_specific.clear();
            let c = CarrierData::new(format.clone(), out.clone());
            assert_eq!(handler.extract(&c, &q).await.unwrap(), payload, "{what}");
            let got = extract_stream(format.clone(), Cursor::new(&out), &q).unwrap();
            assert_eq!(got, payload, "{what}");
        }
    }
}

#[tokio::test]
async fn test_in_memory_embed_then_stream_extract() {
    let handler = SamBamHandler::new();
    for (name, format, data) in carriers() {
        if name.ends_with("tiny") {
            continue;
        }
        // Every in-memory strategy is readable by the streaming extractor.
        for s in ["auxiliary", "quality", "optional", "comment", "multi"] {
            for pw in [&b""[..], b"hunter2"] {
                for c in ["bgzf", "none"] {
                    let params = with_compression(p(Some(s), pw), c);
                    let payload = format!("{s} in {name}").into_bytes();
                    let mut carrier = CarrierData::new(format.clone(), data.clone());
                    handler
                        .embed(&mut carrier, &payload, &params)
                        .await
                        .unwrap();
                    let got =
                        extract_stream(format.clone(), Cursor::new(&carrier.raw_data), &params)
                            .unwrap();
                    assert_eq!(got, payload, "{name}/{s}/{c}");
                }
            }
        }
    }
}

#[test]
fn test_ec_and_wrong_password() {
    for (_, format, data) in carriers().into_iter().filter(|(n, ..)| *n == "bgzf") {
        for s in STRATEGIES {
            for level in [ECLevel::Low, ECLevel::High] {
                let params = params(Some(s), b"right", level);
                let mut out = Vec::new();
                embed_stream(format.clone(), Cursor::new(&data), &mut out, b"ec", &params).unwrap();
                assert_eq!(
                    extract_stream(format.clone(), Cursor::new(&out), &params).unwrap(),
                    b"ec"
                );
                let wrong = p(None, b"wrong");
                assert!(extract_stream(format.clone(), Cursor::new(&out), &wrong).is_err());
            }
        }
    }
}

#[test]
fn test_streamability() {
    for f in [DataFormat::SAM, DataFormat::BAM] {
        for s in STRATEGIES {
            assert!(is_streamable(&f, &p(Some(s), b"")), "{f:?}/{s}");
        }
        for s in [None, Some("optional"), Some("comment"), Some("multi")] {
            let params = p(s, b"");
            assert!(!is_streamable(&f, &params));
            let err =
                embed_stream(f.clone(), Cursor::new(b""), Vec::new(), b"x", &params).unwrap_err();
            match err {
                SteganoError::InvalidParameter(m) => assert_eq!(
                    m,
                    format!(
                        "{f:?}/{} is not streamable; use the in-memory API",
                        s.unwrap_or("default")
                    )
                ),
                other => panic!("{other:?}"),
            }
        }
        assert!(!is_streamable(&f, &p(Some("bogus"), b"")));
        let bad = with_compression(p(Some("quality"), b""), "zstd");
        assert!(!is_streamable(&f, &bad));
    }
}

#[test]
fn test_existing_payload_and_tags_match_in_memory() {
    for (name, format, data) in carriers() {
        if name.ends_with("tiny") {
            continue;
        }
        for first in ["auxiliary", "quality", "optional", "comment"] {
            let p1 = p(Some(first), b"");
            let enc = encode_payload(b"first", &p1).unwrap();
            let once = embed_codec(&format, &data, &enc, &p1).unwrap();
            for second in STRATEGIES {
                let p2 = p(Some(second), b"");
                let enc2 = encode_payload(b"second", &p2).unwrap();
                let mem = embed_codec(&format, &once, &enc2, &p2).map_err(|e| e.to_string());
                let st =
                    stream_embed_encoded(&format, &once, &enc2, &p2).map_err(|e| e.to_string());
                assert_eq!(st, mem, "{name}/{first}->{second}");
                assert!(st.is_err());
            }
        }
    }
    // A foreign zs tag: auxiliary refuses, quality embeds (both APIs).
    let sam = String::from_utf8(sam_fixture(50))
        .unwrap()
        .replacen("\tNM:i:1", "\tNM:i:1\tzs:Z:hello", 1)
        .into_bytes();
    let bam = bgzf_compress_raw(&sam_to_raw_bam(&sam));
    for (format, data) in [(DataFormat::SAM, sam), (DataFormat::BAM, bam)] {
        for s in STRATEGIES {
            let params = p(Some(s), b"");
            let enc = encode_payload(b"x", &params).unwrap();
            let mem = embed_codec(&format, &data, &enc, &params).map_err(|e| e.to_string());
            let st = stream_embed_encoded(&format, &data, &enc, &params).map_err(|e| e.to_string());
            assert_eq!(st, mem, "{format:?}/{s}");
            assert_eq!(st.is_ok(), s == "quality");
        }
    }
}

fn bgzf_compress_raw(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for c in raw.chunks(BGZF_BLOCK_DATA) {
        bgzf_write_block(c, &mut out).unwrap();
    }
    out.extend_from_slice(&BGZF_EOF);
    out
}

#[test]
fn test_unordered_tags_rejected() {
    let sam = String::from_utf8(sam_fixture(20))
        .unwrap()
        .replacen("\tNM:i:1", "\tNM:i:1\tzs:Z:1,00", 1)
        .replacen("\tRG:Z:rg1", "\tRG:Z:rg1\tzs:Z:0,00", 1)
        .into_bytes();
    let params = p(Some("quality"), b"");
    let enc = encode_payload(b"x", &params).unwrap();
    assert!(embed_codec(&DataFormat::SAM, &sam, &enc, &params).is_ok());
    let err = stream_embed_encoded(&DataFormat::SAM, &sam, &enc, &params).unwrap_err();
    assert!(matches!(err, SteganoError::InvalidParameter(_)), "{err}");
    let err = extract_stream(DataFormat::SAM, Cursor::new(&sam), &params).unwrap_err();
    assert!(matches!(err, SteganoError::InvalidParameter(_)), "{err}");
}

/// Truncations and byte flips: the streaming embed fails iff the in-memory
/// one does, with the same message (and identical output on success).
#[test]
fn test_corrupt_carriers_match_in_memory() {
    let mut seed = 0x5eed_u64;
    let mut rnd = |m: usize| {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) as usize % m
    };
    let cases = [
        (DataFormat::SAM, sam_fixture(10)),
        (DataFormat::BAM, bam_fixture(10)),
        (
            DataFormat::BAM,
            small_block_bgzf(&sam_to_raw_bam(&sam_fixture(10))),
        ),
        (DataFormat::BAM, sam_to_raw_bam(&sam_fixture(10))),
    ];
    for (format, data) in cases {
        let mut variants: Vec<Vec<u8>> = Vec::new();
        for _ in 0..60 {
            variants.push(data[..rnd(data.len())].to_vec());
            let mut flipped = data.clone();
            let i = rnd(data.len());
            flipped[i] ^= 1 << rnd(8);
            variants.push(flipped);
        }
        for s in STRATEGIES {
            let params = p(Some(s), b"");
            let enc = encode_payload(b"corrupt", &params).unwrap();
            for v in &variants {
                let mem = embed_codec(&format, v, &enc, &params).map_err(|e| e.to_string());
                let st = stream_embed_encoded(&format, v, &enc, &params).map_err(|e| e.to_string());
                assert_eq!(st, mem, "{format:?}/{s}/len={}", v.len());
            }
        }
    }
}

#[test]
fn test_no_payload() {
    for (name, format, data) in carriers() {
        let err = extract_stream(format, Cursor::new(&data), &p(None, b"")).unwrap_err();
        assert!(
            err.to_string().contains("No steganographic data found"),
            "{name}: {err}"
        );
    }
    let err = extract_stream(DataFormat::SAM, Cursor::new(b""), &p(None, b"")).unwrap_err();
    assert!(err.to_string().contains("Empty SAM file"), "{err}");
    let err = extract_stream(DataFormat::BAM, Cursor::new(b"nope"), &p(None, b"")).unwrap_err();
    assert!(err.to_string().contains("Not a BAM file"), "{err}");
}

#[test]
fn test_extract_reads_only_a_prefix() {
    for (format, data) in [
        (DataFormat::SAM, sam_fixture(3000)),
        (DataFormat::BAM, bam_fixture(3000)),
        (DataFormat::BAM, sam_to_raw_bam(&sam_fixture(3000))),
    ] {
        for s in STRATEGIES {
            let params = p(Some(s), b"pw");
            let mut out = Vec::new();
            embed_stream(
                format.clone(),
                Cursor::new(&data),
                &mut out,
                b"tiny",
                &params,
            )
            .unwrap();
            let h = if format == DataFormat::SAM {
                &SAM_STREAM
            } else {
                &BAM_STREAM
            };
            let (raw, consumed) = extract(h, &mut Cursor::new(&out)).unwrap();
            assert_eq!(decode_payload(&raw, &params).unwrap(), b"tiny");
            assert!(
                consumed < 140_000 && consumed * 4 < out.len() as u64,
                "{format:?}/{s}: read {consumed} of {}",
                out.len()
            );
        }
    }
}

#[test]
fn test_embed_file_atomic() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.bam");
    let output = dir.path().join("out.bam");
    std::fs::write(&input, bam_fixture(100)).unwrap();
    let params = p(Some("auxiliary"), b"pw");
    embed_file(DataFormat::BAM, &input, &output, b"file payload", &params).unwrap();
    assert_eq!(
        extract_file(DataFormat::BAM, &output, &params).unwrap(),
        b"file payload"
    );
    let before = std::fs::read(&output).unwrap();
    let err = embed_file(DataFormat::BAM, &input, &output, &[0u8; 200_000], &params).unwrap_err();
    assert!(matches!(err, SteganoError::InsufficientCapacity { .. }));
    assert_eq!(std::fs::read(&output).unwrap(), before);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
}

/// Lazily generated raw BAM: the fixture header, then its records repeated.
struct GenBam {
    header: Vec<u8>,
    records: Vec<u8>,
    repeats: usize,
    done: usize,
    pos: usize,
    in_header: bool,
}

impl GenBam {
    fn new(repeats: usize) -> Self {
        let raw = sam_to_raw_bam(&sam_fixture(1));
        let (_, _, end) = crate::handlers::sam_bam::parse_bam_header(&raw).unwrap();
        Self {
            header: raw[..end].to_vec(),
            records: raw[end..].to_vec(),
            repeats,
            done: 0,
            pos: 0,
            in_header: true,
        }
    }
}

impl Read for GenBam {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let cur = if self.in_header {
            &self.header
        } else {
            &self.records
        };
        if self.pos == cur.len() {
            if self.in_header {
                self.in_header = false;
            } else {
                self.done += 1;
            }
            self.pos = 0;
            if self.done == self.repeats {
                return Ok(0);
            }
            return self.read(out);
        }
        let n = out.len().min(cur.len() - self.pos);
        out[..n].copy_from_slice(&cur[self.pos..self.pos + n]);
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
fn test_bounded_memory() {
    // ~400 bytes per repeat: raw -> raw ~24 MB, raw -> BGZF ~4 MB.
    for (repeats, comp) in [(60_000, "none"), (10_000, "bgzf")] {
        for s in STRATEGIES {
            let params = with_compression(p(Some(s), b""), comp);
            let payload = vec![0xC3u8; 2048];
            let encoded = encode_payload(&payload, &params).unwrap();
            let gen = GenBam::new(repeats);
            let input_len = (gen.header.len() + gen.records.len() * repeats) as u64;
            let mut reader = BufReader::with_capacity(64 * 1024, gen);
            let mut sink = PrefixSink {
                prefix: Vec::new(),
                keep: 1 << 20,
                total: 0,
            };
            let stats =
                embed_encoded_stream(&DataFormat::BAM, &mut reader, &mut sink, &encoded, &params)
                    .unwrap();
            assert_eq!(stats.bytes_read, input_len);
            assert_eq!(sink.total, stats.bytes_written);
            // One BGZF block each way + one record + header + envelope.
            assert!(
                stats.peak_buffer_bytes < 400 * 1024,
                "{s}/{comp}: peak {}",
                stats.peak_buffer_bytes
            );
            // The payload sits in the first records: the 1 MiB prefix suffices.
            let (raw, consumed) = extract(&BAM_STREAM, &mut Cursor::new(&sink.prefix)).unwrap();
            assert_eq!(decode_payload(&raw, &params).unwrap(), payload);
            assert!(consumed < 400_000, "{consumed}");
        }
    }
}

/// Writes streaming outputs to `$STEGANO_SAM_OUT` for manual validation
/// with `samtools quickcheck` / `samtools view -c`.
#[test]
#[ignore]
fn dump_stream_outputs_for_samtools() {
    let Ok(dir) = std::env::var("STEGANO_SAM_OUT") else {
        return;
    };
    for (name, format, data) in carriers() {
        if name.ends_with("tiny") {
            continue;
        }
        for (i, params) in variants(&format).into_iter().enumerate() {
            let mut out = Vec::new();
            embed_stream(
                format.clone(),
                Cursor::new(&data),
                &mut out,
                b"samtools",
                &params,
            )
            .unwrap();
            let ext = if format == DataFormat::SAM {
                "sam"
            } else {
                "bam"
            };
            std::fs::write(format!("{dir}/stream-{name}-{i}.{ext}"), out).unwrap();
        }
    }
}
