//! Plain-text handler ([`DataFormat::TEXT`]): trailing-whitespace encoding.
//!
//! # Scheme
//!
//! Each **eligible** line carries `k` bits as `k` trailing whitespace
//! characters: space = `0`, tab = `1`.  A line is eligible unless it contains
//! a bare carriage return (one not part of a `\r\n` terminator), which some
//! tools treat as a line break.  Lines are split on `\n`; each line's
//! terminator (`\n`, `\r\n` or none for a final unterminated line) is kept
//! exactly, and the whitespace goes before it.
//!
//! On every line it writes to, the embedder first **removes any existing
//! trailing spaces and tabs**, then appends exactly `k` characters.  The
//! extractor therefore reads the whole trailing space/tab run of each
//! eligible line: on the first one its length gives `k`, and every later
//! line used must have a run of exactly `k`.  Existing trailing whitespace in
//! the cover can never desynchronise the extractor.  Lines after the last one
//! used are left byte-identical, including any trailing whitespace they had.
//!
//! The visible text is unchanged: every line with trailing whitespace
//! stripped equals the cover's line with trailing whitespace stripped.  The
//! output is still plain ASCII if the input was ASCII.  **The embedding is
//! visible** in any editor that shows whitespace, and it is destroyed by
//! tools that strip trailing whitespace (many editors, `git` hooks,
//! formatters).
//!
//! `k` is the smallest value in `1..=64` that fits the payload (so the fewest
//! characters are added per line), unless
//! `format_specific["text_bits_per_line"]` fixes it.  The envelope's strategy
//! byte records `k`.
//!
//! # Envelope and capacity
//!
//! The bits are the envelope stream (`"TXTJ"`, strategy
//! byte `k`, length, then the [`crate::codec::encode_payload`] output),
//! written LSB first, `k` bits per line, in file order; the last line used is
//! padded with spaces.  Capacity is exactly `64 x eligible lines` bits
//! (`k x eligible lines` with a fixed `k`); `recommended_bits` is that minus
//! the 72-bit envelope header.
//!
//! The carrier must be UTF-8 text without NUL bytes; anything else is
//! rejected with [`SteganoError::IntegrityError`].
//!
//! Streaming ([`crate::stream`]) needs a fixed `text_bits_per_line`, because
//! choosing `k` needs the line count up front; with the same `k` both paths
//! produce the same bytes.  Stream extraction works for any `k`.

use super::envelope::{self, EnvelopeReader, ENVELOPE_BITS, ENVELOPE_LEN};
use super::vcf::split_eol;
use super::DataHandler;
use crate::core::*;
use crate::time::Instant;
use async_trait::async_trait;

/// Envelope magic.
pub(crate) const MAGIC: &[u8; 4] = b"TXTJ";
/// Largest number of bits (whitespace characters) per line.
pub const MAX_BITS_PER_LINE: usize = 64;
/// `format_specific` key selecting the strategy (only `whitespace`).
pub(crate) const STRATEGY_KEY: &str = "text_strategy";
/// `format_specific` key fixing the bits per line (`1..=64`).
pub(crate) const BITS_KEY: &str = "text_bits_per_line";
/// Name used in messages.
pub(crate) const WHAT: &str = "text";
/// Codec header bytes (unencrypted, no EC).
const CODEC_HEADER: usize = crate::codec::HEADER_LEN;

/// Handler for plain-text carriers.  See the module documentation.
#[derive(Debug, Default, Clone, Copy)]
pub struct TextHandler;

impl TextHandler {
    /// Create a new handler.
    pub fn new() -> Self {
        Self
    }
}

/// Validate `text_strategy` (absent, `whitespace` or `trailing_whitespace`).
pub(crate) fn check_strategy(params: &EmbedParams) -> Result<()> {
    match params.format_specific.get(STRATEGY_KEY) {
        None => Ok(()),
        Some(v) => match v.as_str().map(str::to_ascii_lowercase).as_deref() {
            Some("whitespace" | "trailing_whitespace") => Ok(()),
            Some(other) => Err(SteganoError::InvalidParameter(format!(
                "unknown {STRATEGY_KEY} '{other}'; expected: whitespace"
            ))),
            None => Err(SteganoError::InvalidParameter(format!(
                "{STRATEGY_KEY} must be a string"
            ))),
        },
    }
}

/// The fixed bits per line from `text_bits_per_line`, if given (a number
/// or a numeric string in `1..=64`).
pub(crate) fn bits_per_line(params: &EmbedParams) -> Result<Option<usize>> {
    let Some(v) = params.format_specific.get(BITS_KEY) else {
        return Ok(None);
    };
    let k = v
        .as_u64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
        .filter(|k| (1..=MAX_BITS_PER_LINE as u64).contains(k))
        .ok_or_else(|| {
            SteganoError::InvalidParameter(format!(
                "{BITS_KEY} must be an integer in 1..={MAX_BITS_PER_LINE}"
            ))
        })?;
    Ok(Some(k as usize))
}

/// Whether a line (without terminator) can carry bits.
pub(crate) fn eligible(body: &[u8]) -> bool {
    !body.contains(&b'\r')
}

/// Split a line body into its visible part and its trailing run of spaces
/// and tabs.
pub(crate) fn split_trailing(body: &[u8]) -> (&[u8], &[u8]) {
    let n = body
        .iter()
        .rev()
        .take_while(|&&b| b == b' ' || b == b'\t')
        .count();
    body.split_at(body.len() - n)
}

/// Whitespace character for a bit.
pub(crate) fn ws(bit: u8) -> u8 {
    if bit == 0 {
        b' '
    } else {
        b'\t'
    }
}

/// Lines needed for `bits` at `k` bits per line.
pub(crate) fn lines_needed(bits: usize, k: usize) -> usize {
    bits.div_ceil(k)
}

/// Line segments (terminator included) after checking the carrier is text.
fn lines(data: &[u8]) -> Result<Vec<&[u8]>> {
    let mut base = 0;
    data.split_inclusive(|&b| b == b'\n')
        .map(|seg| {
            envelope::check_text(seg, base, WHAT)?;
            base += seg.len();
            Ok(seg)
        })
        .collect()
}

/// Write `body`'s visible part, `k` whitespace bits starting at bit `start`
/// of `stream`, and `eol`.
pub(crate) fn write_line(
    out: &mut Vec<u8>,
    body: &[u8],
    eol: &[u8],
    stream: &[u8],
    start: usize,
    k: usize,
) {
    out.extend_from_slice(split_trailing(body).0);
    out.extend((start..start + k).map(|i| {
        ws(if i < stream.len() * 8 {
            envelope::bit(stream, i)
        } else {
            0
        })
    }));
    out.extend_from_slice(eol);
}

/// Embed already codec-encoded bytes with `k` bits per line (`None`: the
/// smallest `k` that fits).
pub(crate) fn embed_encoded(data: &[u8], encoded: &[u8], k: Option<usize>) -> Result<Vec<u8>> {
    let segs = lines(data)?;
    let n_eligible = segs.iter().filter(|s| eligible(split_eol(s).0)).count();
    let needed = (ENVELOPE_LEN + encoded.len()) * 8;
    let max_k = k.unwrap_or(MAX_BITS_PER_LINE);
    let k = k.unwrap_or_else(|| needed.div_ceil(n_eligible.max(1)).max(1));
    if k > max_k || lines_needed(needed, k) > n_eligible {
        return Err(SteganoError::InsufficientCapacity {
            needed,
            available: n_eligible * max_k,
        });
    }
    let stream = envelope::build(MAGIC, k as u8, encoded)?;
    let mut out = Vec::with_capacity(data.len() + needed.div_ceil(k) * k);
    let mut written = 0;
    for seg in segs {
        let (body, eol) = split_eol(seg);
        if written < needed && eligible(body) {
            write_line(&mut out, body, eol, &stream, written, k);
            written += k;
        } else {
            out.extend_from_slice(seg);
        }
    }
    Ok(out)
}

/// Incremental extractor shared with the stream handler.
pub(crate) struct TextExtractor {
    n_eligible: usize,
    state: Option<(usize, EnvelopeReader)>,
}

impl TextExtractor {
    /// `n_eligible`: the number of eligible lines when known (caps the
    /// declared length), else `None`.
    pub(crate) fn new(n_eligible: Option<usize>) -> Self {
        Self {
            n_eligible: n_eligible.unwrap_or(usize::MAX),
            state: None,
        }
    }

    /// Feed one line body; `Ok(true)` once the envelope is complete.
    pub(crate) fn line(&mut self, body: &[u8]) -> Result<bool> {
        if !eligible(body) {
            return Ok(false);
        }
        let trail = split_trailing(body).1;
        if self.state.is_none() {
            let k = trail.len();
            if k == 0 || k > MAX_BITS_PER_LINE {
                return Err(envelope::not_found(WHAT));
            }
            let max_len = self
                .n_eligible
                .checked_mul(k)
                .map_or(usize::MAX, |bits| bits.saturating_sub(ENVELOPE_BITS) / 8);
            self.state = Some((k, EnvelopeReader::new(MAGIC, k as u8, max_len, WHAT)));
        }
        let Some((k, reader)) = self.state.as_mut() else {
            return Err(envelope::not_found(WHAT));
        };
        let k = *k;
        if trail.len() != k {
            return Err(if reader.header_read() {
                SteganoError::IntegrityError(format!(
                    "text carrier is corrupted: a line has {} trailing whitespace \
                     characters, expected {k}",
                    trail.len()
                ))
            } else {
                envelope::not_found(WHAT)
            });
        }
        for &c in trail {
            if reader.push(u8::from(c == b'\t'))? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// The codec bytes, or an error if the envelope is missing/incomplete.
    pub(crate) fn finish(self) -> Result<Vec<u8>> {
        match self.state {
            None => Err(envelope::not_found(WHAT)),
            Some((_, r)) => r.finish(),
        }
    }
}

/// Read the codec bytes back out of a carrier.
pub(crate) fn extract_encoded(data: &[u8]) -> Result<Vec<u8>> {
    let segs = lines(data)?;
    let n_eligible = segs.iter().filter(|s| eligible(split_eol(s).0)).count();
    let mut ex = TextExtractor::new(Some(n_eligible));
    for seg in segs {
        if ex.line(split_eol(seg).0)? {
            break;
        }
    }
    ex.finish()
}

fn check_format(carrier: &CarrierData) -> Result<()> {
    if carrier.format == DataFormat::TEXT {
        Ok(())
    } else {
        Err(SteganoError::UnsupportedFormat(carrier.format.clone()))
    }
}

#[async_trait]
impl DataHandler for TextHandler {
    /// Plain text is never detected from content (it would shadow binary
    /// detection); select it by extension (`.txt`) or explicitly.
    fn detect_format(&self, _data: &[u8]) -> Option<DataFormat> {
        None
    }

    /// Exact capacity: 64 bits per eligible line (the automatic `k` goes up
    /// to 64).  `recommended_bits` is the largest codec frame (in bits) that
    /// fits after the envelope header.
    fn estimate_capacity(&self, carrier: &CarrierData) -> Result<CapacityInfo> {
        check_format(carrier)?;
        let data = &carrier.raw_data;
        let segs = lines(data)?;
        let n_eligible = segs.iter().filter(|s| eligible(split_eol(s).0)).count();
        let frame_bytes = (n_eligible * MAX_BITS_PER_LINE).saturating_sub(ENVELOPE_BITS) / 8;
        let mut info = CapacityInfo::new(data.len() * 8, frame_bytes * 8);
        let fi = &mut info.format_specific_info;
        fi.insert("method".into(), "trailing_whitespace".into());
        fi.insert("lines".into(), segs.len().into());
        fi.insert("eligible_lines".into(), n_eligible.into());
        fi.insert("max_bits_per_line".into(), MAX_BITS_PER_LINE.into());
        fi.insert("overhead_bytes".into(), ENVELOPE_LEN.into());
        fi.insert("visible_in_whitespace_aware_editors".into(), true.into());
        fi.insert(
            "max_payload_bytes_unencrypted".into(),
            frame_bytes.saturating_sub(CODEC_HEADER).into(),
        );
        fi.insert(
            "max_payload_bytes_encrypted".into(),
            frame_bytes
                .saturating_sub(CODEC_HEADER + crate::crypto::TAG_LEN)
                .into(),
        );
        fi.insert("estimated_capacity_bytes".into(), frame_bytes.into());
        Ok(info)
    }

    async fn embed(
        &self,
        carrier: &mut CarrierData,
        payload: &[u8],
        params: &EmbedParams,
    ) -> Result<EmbedResult> {
        check_format(carrier)?;
        let start = Instant::now();
        check_strategy(params)?;
        let k = bits_per_line(params)?;
        // Structural check before the (expensive) codec.
        lines(&carrier.raw_data)?;
        let enc_start = Instant::now();
        let encoded = crate::codec::encode_payload(payload, params)?;
        let encryption_time_ms = if params.encryption_key.is_empty() {
            0
        } else {
            (enc_start.elapsed().as_millis() as u64).max(1)
        };
        carrier.raw_data = embed_encoded(&carrier.raw_data, &encoded, k)?;
        Ok(EmbedResult::success(
            payload.len(),
            PerformanceStats {
                embedding_time_ms: start.elapsed().as_millis() as u64,
                encryption_time_ms,
                gpu_acceleration_used: false,
                memory_usage_bytes: carrier.raw_data.len() + encoded.len(),
            },
        ))
    }

    async fn extract(&self, carrier: &CarrierData, params: &EmbedParams) -> Result<Vec<u8>> {
        check_format(carrier)?;
        let raw = extract_encoded(&carrier.raw_data)?;
        crate::codec::decode_payload(&raw, params)
    }

    /// `true` iff the carrier is text containing a `TXTJ` envelope (no
    /// decryption is attempted).
    fn verify_integrity(&self, carrier: &CarrierData) -> Result<bool> {
        check_format(carrier)?;
        Ok(extract_encoded(&carrier.raw_data).is_ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(password: &str) -> EmbedParams {
        EmbedParams {
            encryption_key: password.as_bytes().to_vec(),
            error_correction_level: ECLevel::None,
            ..Default::default()
        }
    }

    fn with_k(mut p: EmbedParams, k: usize) -> EmbedParams {
        p.format_specific
            .insert(BITS_KEY.into(), serde_json::Value::from(k));
        p
    }

    fn carrier(data: &[u8]) -> CarrierData {
        CarrierData::new(DataFormat::TEXT, data.to_vec())
    }

    /// LF/CRLF mix, existing trailing whitespace, a Unicode line, blank
    /// lines, no final newline.
    fn sample_text(n: usize) -> Vec<u8> {
        let mut s = String::new();
        for i in 0..n {
            let line = match i % 5 {
                0 => format!("Line {i}: the quick brown fox"),
                1 => format!("trailing spaces {i}   \t "),
                2 => "Ünïcödé — 日本語 ✓".to_string(),
                3 => String::new(),
                _ => "\t indented".to_string(),
            };
            s.push_str(&line);
            if i + 1 < n {
                s.push_str(if i % 2 == 0 { "\r\n" } else { "\n" });
            }
        }
        s.into_bytes()
    }

    fn visible(data: &[u8]) -> Vec<String> {
        std::str::from_utf8(data)
            .unwrap()
            .split('\n')
            .map(|l| l.trim_end().to_string())
            .collect()
    }

    fn assert_visible_same(a: &[u8], b: &[u8]) {
        assert_eq!(visible(a), visible(b));
        let crlf = |d: &[u8]| d.windows(2).filter(|w| w == b"\r\n").count();
        assert_eq!(crlf(a), crlf(b));
        assert_eq!(a.ends_with(b"\n"), b.ends_with(b"\n"));
        assert!(b.is_ascii() || !a.is_ascii());
    }

    #[tokio::test]
    async fn roundtrip_with_and_without_password() {
        let data = sample_text(1000);
        let h = TextHandler::new();
        for pw in ["", "secret"] {
            for k in [None, Some(1), Some(7), Some(64)] {
                let p = match k {
                    Some(k) => with_k(params(pw), k),
                    None => params(pw),
                };
                let mut c = carrier(&data);
                h.embed(&mut c, b"hidden text \xff", &p).await.unwrap();
                assert_visible_same(&data, &c.raw_data);
                // Extraction auto-detects k.
                assert_eq!(
                    h.extract(&c, &params(pw)).await.unwrap(),
                    b"hidden text \xff"
                );
                assert!(h.verify_integrity(&c).unwrap());
                // Re-embedding over a stego file works (normalisation).
                let mut c2 = c.clone();
                h.embed(&mut c2, b"second", &p).await.unwrap();
                assert_visible_same(&data, &c2.raw_data);
                assert_eq!(h.extract(&c2, &params(pw)).await.unwrap(), b"second");
            }
        }
    }

    #[tokio::test]
    async fn auto_k_is_minimal_and_lines_after_untouched() {
        let data = sample_text(1000);
        let h = TextHandler::new();
        let mut c = carrier(&data);
        h.embed(&mut c, b"x", &params("")).await.unwrap();
        // 46-byte frame = 368 bits over 1000 lines -> k = 1.
        let first = std::str::from_utf8(&c.raw_data)
            .unwrap()
            .lines()
            .next()
            .unwrap();
        assert_eq!(split_trailing(first.as_bytes()).1.len(), 1);
        // The tail of the file is byte-identical.
        let n = 200;
        assert_eq!(&c.raw_data[c.raw_data.len() - n..], &data[data.len() - n..]);
    }

    #[tokio::test]
    async fn ascii_stays_ascii_and_bare_cr_lines_skipped() {
        let mut s = String::new();
        for i in 0..400 {
            s.push_str(&format!("row {i}\n"));
        }
        s.push_str("old\rmac\n");
        let data = s.into_bytes();
        let h = TextHandler::new();
        let mut c = carrier(&data);
        let p = with_k(params(""), 1);
        h.embed(&mut c, b"", &p).await.unwrap();
        assert!(c.raw_data.is_ascii());
        assert!(c.raw_data.ends_with(b"old\rmac\n"));
        assert_eq!(h.extract(&c, &p).await.unwrap(), b"");
    }

    #[tokio::test]
    async fn capacity_is_exact() {
        let data = sample_text(50);
        let h = TextHandler::new();
        let info = h.estimate_capacity(&carrier(&data)).unwrap();
        assert_eq!(info.format_specific_info["eligible_lines"], 50);
        let max = info.format_specific_info["max_payload_bytes_unencrypted"]
            .as_u64()
            .unwrap() as usize;
        let mut c = carrier(&data);
        h.embed(&mut c, &vec![9u8; max], &params("")).await.unwrap();
        assert_eq!(h.extract(&c, &params("")).await.unwrap(), vec![9u8; max]);
        let mut c = carrier(&data);
        assert!(matches!(
            h.embed(&mut c, &vec![9u8; max + 1], &params("")).await,
            Err(SteganoError::InsufficientCapacity {
                available: 3200,
                ..
            })
        ));
        assert_eq!(c.raw_data, data);
        // Fixed k: capacity is k x lines.
        let mut c = carrier(&data);
        assert!(matches!(
            h.embed(&mut c, b"x", &with_k(params(""), 1)).await,
            Err(SteganoError::InsufficientCapacity { available: 50, .. })
        ));
    }

    #[tokio::test]
    async fn errors_not_panics() {
        let h = TextHandler::new();
        let p = params("");
        for bad in [
            &b"hello\n\x00\x01"[..],
            b"caf\xe9\n",
            b"\x89PNG\r\n\x1a\n\x00\x00",
        ] {
            let mut c = carrier(bad);
            match h.embed(&mut c, b"x", &p).await {
                Err(SteganoError::IntegrityError(m)) => assert!(m.contains("UTF-8"), "{m}"),
                other => panic!("{other:?}"),
            }
            assert!(h.extract(&carrier(bad), &p).await.is_err());
            assert!(h.estimate_capacity(&carrier(bad)).is_err());
        }
        let mut c = carrier(b"");
        assert!(matches!(
            h.embed(&mut c, b"x", &p).await,
            Err(SteganoError::InsufficientCapacity { .. })
        ));
        // Cover text (with existing trailing whitespace) is not a payload.
        assert!(h.extract(&carrier(&sample_text(100)), &p).await.is_err());
        assert!(!h.verify_integrity(&carrier(&sample_text(100))).unwrap());
        for g in [&b""[..], b"a \n", b"\t\t\t\n \n", b"x\ty \t\n"] {
            assert!(h.extract(&carrier(g), &p).await.is_err());
        }
        // Truncated / edited stego.
        let data = sample_text(300);
        let mut c = carrier(&data);
        h.embed(&mut c, &[5u8; 30], &with_k(params(""), 4))
            .await
            .unwrap();
        for cut in [5, 100, c.raw_data.len() / 4] {
            assert!(h.extract(&carrier(&c.raw_data[..cut]), &p).await.is_err());
        }
        let stripped: Vec<u8> = visible(&c.raw_data).join("\n").into_bytes();
        assert!(h.extract(&carrier(&stripped), &p).await.is_err());
        // Bad parameters and wrong format.
        for (key, v) in [
            (STRATEGY_KEY, serde_json::Value::from("bogus")),
            (BITS_KEY, serde_json::Value::from(0)),
            (BITS_KEY, serde_json::Value::from(65)),
            (BITS_KEY, serde_json::Value::from("x")),
        ] {
            let mut bad = params("");
            bad.format_specific.insert(key.into(), v);
            assert!(h.embed(&mut carrier(&data), b"x", &bad).await.is_err());
        }
        let mut other = CarrierData::new(DataFormat::BINARY, data);
        assert!(h.embed(&mut other, b"x", &p).await.is_err());
    }
}
