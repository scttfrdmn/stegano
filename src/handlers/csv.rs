//! CSV handler ([`DataFormat::CsvTimeseries`]): last-digit parity of
//! numeric cells.
//!
//! # Scheme
//!
//! The carrier is parsed as RFC 4180 CSV: fields are separated by a
//! delimiter (`,` by default, `format_specific["csv_delimiter"]` to
//! override), records by `\n` or `\r\n`, and a field that starts with `"` is
//! quoted (`""` escapes a quote; delimiters and line breaks inside quotes are
//! data).
//!
//! A cell is **eligible** iff it is unquoted and is a plain decimal number
//! whose mantissa ends in a digit:
//!
//! ```text
//! [+-]? ( digits [ "." digits* ] | "." digits ) [ (e|E) [+-]? digits ]
//! ```
//!
//! e.g. `12`, `-3.140`, `.5`, `1e-5`, `6.02E+23` (but not `12.`, ` 12`,
//! `"12"`, `nan`, `0x1F`).  Each eligible cell carries one bit in the parity
//! of the **last digit of its mantissa** (the digit before any exponent);
//! embedding flips that digit 0↔1, 2↔3, 4↔5, 6↔7 or 8↔9 when needed.  The
//! field width never changes, so a value changes by at most one unit in its
//! last printed mantissa place (`12.34` → `12.35`, `-7` → `-6`,
//! `1e-5` → `0e-5`).  **Numeric values do change**: do not use this on data
//! where a last-place change matters (identifiers, counts that must sum,
//! checksums).
//!
//! The first record is a **header** if it contains a non-empty cell that is
//! not eligible (a quoted cell, a name, ...); a header is never modified.
//! Leading blank lines are skipped for this decision.  Because eligibility
//! depends only on bytes that embedding never changes, the extractor sees
//! exactly the cells the embedder used.
//!
//! Everything else is byte-identical: delimiters, quotes, quoted cells,
//! non-numeric cells, line endings (`\n` / `\r\n`, per line) and the
//! presence or absence of a final newline.
//!
//! # Envelope and capacity
//!
//! The bits are the envelope stream (`"CSVJ"`, strategy
//! byte `1`, length, then the [`crate::codec::encode_payload`] output),
//! written LSB first into eligible cells in file order.  Capacity is exactly
//! one bit per eligible cell; `recommended_bits` is that minus the 72-bit
//! envelope header, i.e. the largest codec frame that fits.
//!
//! The carrier must be UTF-8 text without NUL bytes; anything else is
//! rejected with [`SteganoError::IntegrityError`].
//!
//! The format is streamable ([`crate::stream`]); both paths produce the same
//! bytes.

use super::envelope::{self, EnvelopeReader, ENVELOPE_BITS, ENVELOPE_LEN};
use super::vcf::split_eol;
use super::DataHandler;
use crate::core::*;
use crate::time::Instant;
use async_trait::async_trait;

/// Envelope magic.
pub(crate) const MAGIC: &[u8; 4] = b"CSVJ";
/// Envelope strategy byte for last-digit parity.
pub(crate) const STRATEGY_BYTE: u8 = 1;
/// `format_specific` key selecting the strategy (only `digit`).
pub(crate) const STRATEGY_KEY: &str = "csv_strategy";
/// `format_specific` key overriding the delimiter (one ASCII character).
pub(crate) const DELIMITER_KEY: &str = "csv_delimiter";
/// Name used in messages.
pub(crate) const WHAT: &str = "CSV";
/// Codec header bytes (unencrypted, no EC).
const CODEC_HEADER: usize = crate::codec::HEADER_LEN;

/// Handler for CSV carriers.  See the module documentation.
#[derive(Debug, Default, Clone, Copy)]
pub struct CsvHandler;

impl CsvHandler {
    /// Create a new handler.
    pub fn new() -> Self {
        Self
    }
}

/// Validate `csv_strategy` (absent, `digit`, `last_digit` or `parity`).
pub(crate) fn check_strategy(params: &EmbedParams) -> Result<()> {
    match params.format_specific.get(STRATEGY_KEY) {
        None => Ok(()),
        Some(v) => match v.as_str().map(str::to_ascii_lowercase).as_deref() {
            Some("digit" | "last_digit" | "parity") => Ok(()),
            Some(other) => Err(SteganoError::InvalidParameter(format!(
                "unknown {STRATEGY_KEY} '{other}'; expected: digit"
            ))),
            None => Err(SteganoError::InvalidParameter(format!(
                "{STRATEGY_KEY} must be a string"
            ))),
        },
    }
}

/// The delimiter from `csv_delimiter` (default `,`).  `tab` and `\t` mean
/// a tab.
pub(crate) fn delimiter(params: &EmbedParams) -> Result<u8> {
    let Some(v) = params.format_specific.get(DELIMITER_KEY) else {
        return Ok(b',');
    };
    let bad = || {
        SteganoError::InvalidParameter(format!(
            "{DELIMITER_KEY} must be a single ASCII character other than '\"', CR or LF"
        ))
    };
    let s = v.as_str().ok_or_else(bad)?;
    let d = match s {
        "tab" | "\\t" => b'\t',
        _ if s.len() == 1 => s.as_bytes()[0],
        _ => return Err(bad()),
    };
    if !d.is_ascii() || matches!(d, b'"' | b'\r' | b'\n') {
        return Err(bad());
    }
    Ok(d)
}

/// Offset of the carrier digit (last mantissa digit) in an eligible field,
/// or `None` if the field is not eligible.
pub(crate) fn carrier_digit(f: &[u8]) -> Option<usize> {
    let digits = |mut i: usize| {
        while f.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        i
    };
    let mut i = usize::from(matches!(f.first(), Some(b'+' | b'-')));
    let int_end = digits(i);
    let mut n_digits = int_end - i;
    i = int_end;
    if f.get(i) == Some(&b'.') {
        let frac_end = digits(i + 1);
        n_digits += frac_end - (i + 1);
        i = frac_end;
    }
    if n_digits == 0 {
        return None;
    }
    let mant_end = i;
    if matches!(f.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(f.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        let exp_end = digits(i);
        if exp_end == i {
            return None;
        }
        i = exp_end;
    }
    (i == f.len() && f[mant_end - 1].is_ascii_digit()).then_some(mant_end - 1)
}

/// Header-detection state of a [`CsvScanner`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Header {
    /// No non-blank line seen yet.
    Pending,
    /// Inside a multi-line header record.
    Continuing,
    /// Past the header (or there is none).
    Data,
}

/// Per-field parser state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Start,
    Unquoted(usize),
    Quoted,
    /// A `"` inside a quoted field: an escape or the closing quote.
    QuoteSeen,
    /// Junk after a closing quote (lenient: still part of the quoted field).
    AfterQuote,
}

/// Incremental RFC 4180 scanner, fed one physical line at a time (so the
/// in-memory and streaming handlers share it and agree exactly).
pub(crate) struct CsvScanner {
    delim: u8,
    in_quotes: bool,
    header: Header,
    cells: Vec<usize>,
}

impl CsvScanner {
    pub(crate) fn new(delim: u8) -> Self {
        Self {
            delim,
            in_quotes: false,
            header: Header::Pending,
            cells: Vec::new(),
        }
    }

    /// Offsets (into `body`, the line without its terminator) of the carrier
    /// digits on this line, in order.
    pub(crate) fn scan_line(&mut self, body: &[u8]) -> &[usize] {
        self.cells.clear();
        let mut non_numeric = false;
        let mut any = false;
        let mut finish = |cells: &mut Vec<usize>, s: usize, e: usize| {
            if e > s {
                any = true;
                match carrier_digit(&body[s..e]) {
                    Some(off) => cells.push(s + off),
                    None => non_numeric = true,
                }
            }
        };
        let mut state = if self.in_quotes {
            Field::Quoted
        } else {
            Field::Start
        };
        let mut quoted_seen = false;
        for (i, &c) in body.iter().enumerate() {
            state = match state {
                Field::Start if c == b'"' => {
                    quoted_seen = true;
                    Field::Quoted
                }
                Field::Start if c == self.delim => Field::Start,
                Field::Start => Field::Unquoted(i),
                Field::Unquoted(s) if c == self.delim => {
                    finish(&mut self.cells, s, i);
                    Field::Start
                }
                Field::Unquoted(s) => Field::Unquoted(s),
                Field::Quoted if c == b'"' => Field::QuoteSeen,
                Field::Quoted => Field::Quoted,
                Field::QuoteSeen if c == b'"' => Field::Quoted,
                Field::QuoteSeen | Field::AfterQuote if c == self.delim => Field::Start,
                Field::QuoteSeen | Field::AfterQuote => Field::AfterQuote,
            };
        }
        if let Field::Unquoted(s) = state {
            finish(&mut self.cells, s, body.len());
        }
        let started_in_quotes = self.in_quotes;
        self.in_quotes = state == Field::Quoted;

        match self.header {
            Header::Pending if !any && !quoted_seen && !started_in_quotes => {}
            Header::Pending => {
                if non_numeric || quoted_seen {
                    self.cells.clear();
                    self.header = if self.in_quotes {
                        Header::Continuing
                    } else {
                        Header::Data
                    };
                } else {
                    self.header = Header::Data;
                }
            }
            Header::Continuing => {
                self.cells.clear();
                if !self.in_quotes {
                    self.header = Header::Data;
                }
            }
            Header::Data => {}
        }
        &self.cells
    }
}

/// Set the parity of ASCII digit `d` to `bit` (flip 0↔1, 2↔3, ...).
pub(crate) fn set_parity(d: &mut u8, bit: u8) {
    if *d & 1 != bit {
        *d ^= 1;
    }
}

/// Absolute offsets of all carrier digits in `data`, after checking that
/// `data` is text.
fn carrier_positions(data: &[u8], delim: u8) -> Result<Vec<usize>> {
    let mut scanner = CsvScanner::new(delim);
    let mut out = Vec::new();
    let mut base = 0;
    for seg in data.split_inclusive(|&b| b == b'\n') {
        envelope::check_text(seg, base, WHAT)?;
        let (body, _) = split_eol(seg);
        out.extend(scanner.scan_line(body).iter().map(|&o| base + o));
        base += seg.len();
    }
    Ok(out)
}

/// Embed already codec-encoded bytes; returns the new carrier bytes.
pub(crate) fn embed_encoded(data: &[u8], encoded: &[u8], delim: u8) -> Result<Vec<u8>> {
    let positions = carrier_positions(data, delim)?;
    let stream = envelope::build(MAGIC, STRATEGY_BYTE, encoded)?;
    let needed = stream.len() * 8;
    if needed > positions.len() {
        return Err(SteganoError::InsufficientCapacity {
            needed,
            available: positions.len(),
        });
    }
    let mut out = data.to_vec();
    for (i, &p) in positions[..needed].iter().enumerate() {
        set_parity(&mut out[p], envelope::bit(&stream, i));
    }
    Ok(out)
}

/// Read the codec bytes back out of a carrier.
pub(crate) fn extract_encoded(data: &[u8], delim: u8) -> Result<Vec<u8>> {
    let positions = carrier_positions(data, delim)?;
    let max_len = positions.len().saturating_sub(ENVELOPE_BITS) / 8;
    let mut reader = EnvelopeReader::new(MAGIC, STRATEGY_BYTE, max_len, WHAT);
    for &p in &positions {
        if reader.push(data[p] & 1)? {
            break;
        }
    }
    reader.finish()
}

fn check_format(carrier: &CarrierData) -> Result<()> {
    if carrier.format == DataFormat::CsvTimeseries {
        Ok(())
    } else {
        Err(SteganoError::UnsupportedFormat(carrier.format.clone()))
    }
}

#[async_trait]
impl DataHandler for CsvHandler {
    /// CSV cannot be told apart from other text reliably, so it is never
    /// detected from content; select it by extension (`.csv`) or explicitly.
    fn detect_format(&self, _data: &[u8]) -> Option<DataFormat> {
        None
    }

    /// Exact capacity: one bit per eligible cell.  `recommended_bits` is the
    /// largest codec frame (in bits) that fits after the envelope header.
    fn estimate_capacity(&self, carrier: &CarrierData) -> Result<CapacityInfo> {
        check_format(carrier)?;
        let data = &carrier.raw_data;
        let cells = carrier_positions(data, b',')?.len();
        let frame_bytes = cells.saturating_sub(ENVELOPE_BITS) / 8;
        let mut info = CapacityInfo::new(data.len() * 8, frame_bytes * 8);
        let fi = &mut info.format_specific_info;
        fi.insert("method".into(), "csv_last_digit_parity".into());
        fi.insert("eligible_cells".into(), cells.into());
        fi.insert("bits_per_cell".into(), 1.into());
        fi.insert("overhead_bytes".into(), ENVELOPE_LEN.into());
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
        fi.insert("delimiter".into(), ",".into());
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
        let delim = delimiter(params)?;
        // Structural check before the (expensive) codec.
        carrier_positions(&carrier.raw_data, delim)?;
        let enc_start = Instant::now();
        let encoded = crate::codec::encode_payload(payload, params)?;
        let encryption_time_ms = if params.encryption_key.is_empty() {
            0
        } else {
            (enc_start.elapsed().as_millis() as u64).max(1)
        };
        carrier.raw_data = embed_encoded(&carrier.raw_data, &encoded, delim)?;
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
        let delim = delimiter(params)?;
        let raw = extract_encoded(&carrier.raw_data, delim)?;
        crate::codec::decode_payload(&raw, params)
    }

    /// `true` iff the carrier is CSV text containing a `CSVJ` envelope with
    /// the default delimiter (no decryption is attempted).
    fn verify_integrity(&self, carrier: &CarrierData) -> Result<bool> {
        check_format(carrier)?;
        Ok(extract_encoded(&carrier.raw_data, b',').is_ok())
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

    fn carrier(data: &[u8]) -> CarrierData {
        CarrierData::new(DataFormat::CsvTimeseries, data.to_vec())
    }

    /// A CSV with a header, quoted fields (commas, escaped quotes, a line
    /// break), CRLF endings, negative and exponent numbers and no final
    /// newline.
    fn sample_csv(rows: usize) -> Vec<u8> {
        let mut s = String::from("time,\"sensor, id\",value,note\r\n");
        for i in 0..rows {
            let note = match i % 4 {
                0 => "\"says \"\"hi\"\", ok\"".to_string(),
                1 => "\"multi\r\nline\"".to_string(),
                2 => "plain text".to_string(),
                _ => String::new(),
            };
            s.push_str(&format!(
                "{}.{:03},{},-{}.{}e-{},{}",
                i,
                (i * 37) % 1000,
                i % 7,
                i % 9,
                (i * 13) % 100,
                i % 5 + 1,
                note
            ));
            if i + 1 < rows {
                s.push_str("\r\n");
            }
        }
        s.into_bytes()
    }

    /// Unit in the last mantissa place of a numeric cell.
    fn ulp(cell: &str) -> f64 {
        let (mant, exp) = match cell.find(['e', 'E']) {
            Some(i) => (&cell[..i], cell[i + 1..].parse::<i32>().unwrap()),
            None => (cell, 0),
        };
        let frac = mant.split_once('.').map_or(0, |(_, f)| f.len() as i32);
        10f64.powi(exp - frac)
    }

    fn records(data: &[u8]) -> Vec<Vec<String>> {
        ::csv::ReaderBuilder::new()
            .has_headers(false)
            .flexible(true)
            .from_reader(data)
            .records()
            .map(|r| r.unwrap().iter().map(str::to_string).collect())
            .collect()
    }

    /// Non-numeric cells and the header identical; numeric cells within one
    /// last-place unit; same shape.
    fn assert_valid(orig: &[u8], stego: &[u8]) {
        let (a, b) = (records(orig), records(stego));
        assert_eq!(a.len(), b.len());
        assert_eq!(a[0], b[0], "header changed");
        for (ra, rb) in a.iter().zip(&b) {
            assert_eq!(ra.len(), rb.len());
            for (ca, cb) in ra.iter().zip(rb) {
                if carrier_digit(ca.as_bytes()).is_some() {
                    assert_eq!(ca.len(), cb.len());
                    let (x, y): (f64, f64) = (ca.parse().unwrap(), cb.parse().unwrap());
                    assert!((x - y).abs() <= ulp(ca) * 1.000001, "{ca} -> {cb}");
                } else {
                    assert_eq!(ca, cb);
                }
            }
        }
        // Structure bytes identical: only digits may differ, by parity.
        assert_eq!(orig.len(), stego.len());
        for (x, y) in orig.iter().zip(stego) {
            assert!(x == y || (x.is_ascii_digit() && x ^ 1 == *y));
        }
    }

    #[test]
    fn eligibility() {
        for (f, off) in [
            ("12", Some(1)),
            ("-3.140", Some(5)),
            ("1e-5", Some(0)),
            ("6.02E+23", Some(3)),
            (".5", Some(1)),
            ("+0", Some(1)),
        ] {
            assert_eq!(carrier_digit(f.as_bytes()), off, "{f}");
        }
        for f in [
            "", "12.", " 12", "12 ", "nan", "1e", "1e+", "-", ".", "0x1F", "1.2.3", "1,5",
        ] {
            assert_eq!(carrier_digit(f.as_bytes()), None, "{f}");
        }
    }

    #[tokio::test]
    async fn roundtrip_with_and_without_password() {
        let data = sample_csv(400);
        for pw in ["", "secret"] {
            let h = CsvHandler::new();
            let mut c = carrier(&data);
            let p = params(pw);
            h.embed(&mut c, b"csv payload \x00\xff", &p).await.unwrap();
            assert_ne!(c.raw_data, data);
            assert_valid(&data, &c.raw_data);
            assert!(!c.raw_data.ends_with(b"\n"));
            assert_eq!(
                c.raw_data.windows(2).filter(|w| w == b"\r\n").count(),
                data.windows(2).filter(|w| w == b"\r\n").count()
            );
            assert_eq!(h.extract(&c, &p).await.unwrap(), b"csv payload \x00\xff");
            assert!(h.verify_integrity(&c).unwrap());
        }
    }

    #[tokio::test]
    async fn header_and_quoted_cells_untouched() {
        // A numeric-looking header with one text cell; a quoted number.
        let mut s = String::from("id,2020,2021\n");
        for i in 0..800 {
            s.push_str(&format!("\"{i}\",{},{}\n", i * 3, i * 7));
        }
        let data = s.into_bytes();
        let h = CsvHandler::new();
        let mut c = carrier(&data);
        h.embed(&mut c, b"x", &params("")).await.unwrap();
        assert!(c.raw_data.starts_with(b"id,2020,2021\n"));
        let (a, b) = (records(&data), records(&c.raw_data));
        for (ra, rb) in a.iter().zip(&b) {
            assert_eq!(ra[0], rb[0], "quoted cell changed");
        }
        assert_valid(&data, &c.raw_data);
        assert_eq!(h.extract(&c, &params("")).await.unwrap(), b"x");
    }

    #[tokio::test]
    async fn numeric_first_row_is_data_and_custom_delimiter() {
        let mut s = String::new();
        for i in 0..800 {
            s.push_str(&format!("{};{}\n", i, i * 2));
        }
        let data = s.into_bytes();
        let mut p = params("pw");
        p.format_specific
            .insert(DELIMITER_KEY.into(), serde_json::Value::from(";"));
        let h = CsvHandler::new();
        let mut c = carrier(&data);
        h.embed(&mut c, b"semi", &p).await.unwrap();
        assert_eq!(h.extract(&c, &p).await.unwrap(), b"semi");
        // Default delimiter sees no numeric cells, so nothing is found.
        assert!(h.extract(&c, &params("pw")).await.is_err());
        assert!(extract_encoded(&c.raw_data, b';').is_ok());
    }

    #[tokio::test]
    async fn capacity_is_exact() {
        let data = sample_csv(300);
        let h = CsvHandler::new();
        let info = h.estimate_capacity(&carrier(&data)).unwrap();
        let max = info.format_specific_info["max_payload_bytes_unencrypted"]
            .as_u64()
            .unwrap() as usize;
        assert!(max > 0);
        let mut c = carrier(&data);
        h.embed(&mut c, &vec![7u8; max], &params("")).await.unwrap();
        assert_eq!(h.extract(&c, &params("")).await.unwrap(), vec![7u8; max]);
        let mut c = carrier(&data);
        match h.embed(&mut c, &vec![7u8; max + 1], &params("")).await {
            Err(SteganoError::InsufficientCapacity { needed, available }) => {
                assert!(needed > available);
                assert_eq!(
                    available,
                    info.format_specific_info["eligible_cells"]
                        .as_u64()
                        .unwrap() as usize
                );
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(c.raw_data, data, "carrier modified on error");
    }

    #[tokio::test]
    async fn errors_not_panics() {
        let h = CsvHandler::new();
        let p = params("");
        // No numeric cells at all.
        let mut c = carrier(b"a,b\nc,d\n");
        assert!(matches!(
            h.embed(&mut c, b"x", &p).await,
            Err(SteganoError::InsufficientCapacity { .. })
        ));
        // Binary / non-UTF-8.
        for bad in [&b"1,2\n\x00\x01\x02"[..], b"1,2\n\xff\xfe,3\n"] {
            let mut c = carrier(bad);
            assert!(matches!(
                h.embed(&mut c, b"x", &p).await,
                Err(SteganoError::IntegrityError(_))
            ));
            assert!(h.extract(&carrier(bad), &p).await.is_err());
        }
        // Unembedded, garbage and truncated carriers.
        assert!(h.extract(&carrier(&sample_csv(300)), &p).await.is_err());
        assert!(!h.verify_integrity(&carrier(&sample_csv(300))).unwrap());
        for g in [&b""[..], b"\"unterminated,1,2\n3", b",,,\n\n\r\n", b"1"] {
            assert!(h.extract(&carrier(g), &p).await.is_err());
            let _ = h.estimate_capacity(&carrier(g));
        }
        let data = sample_csv(400);
        let mut c = carrier(&data);
        h.embed(&mut c, &[1u8; 20], &p).await.unwrap();
        for cut in [10, 200, c.raw_data.len() / 3] {
            assert!(h.extract(&carrier(&c.raw_data[..cut]), &p).await.is_err());
        }
        // Bad parameters and wrong format.
        let mut bad = params("");
        bad.format_specific
            .insert(STRATEGY_KEY.into(), serde_json::Value::from("bogus"));
        assert!(h.embed(&mut carrier(&data), b"x", &bad).await.is_err());
        let mut bad = params("");
        bad.format_specific
            .insert(DELIMITER_KEY.into(), serde_json::Value::from("\""));
        assert!(h.embed(&mut carrier(&data), b"x", &bad).await.is_err());
        let mut other = CarrierData::new(DataFormat::TEXT, data);
        assert!(h.embed(&mut other, b"x", &p).await.is_err());
    }

    #[test]
    fn scanner_multiline_header_and_blank_lines() {
        let mut s = CsvScanner::new(b',');
        assert!(s.scan_line(b"").is_empty());
        assert!(s.scan_line(b"\"a").is_empty()); // header record continues
        assert!(s.scan_line(b"b\",1").is_empty()); // still the header
        assert_eq!(s.scan_line(b"12,x,3.5"), &[1, 7]);
        assert_eq!(s.scan_line(b"\"q,\"\"1\"\"\",2"), &[10]);
    }
}
