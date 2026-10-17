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
//! last printed mantissa place (`12.34` → `12.35`, `-17` → `-16`).
//! **Numeric values do change**: do not use this on data where a last-place
//! change matters (identifiers, counts that must sum, checksums).
//!
//! # Significant-digit threshold
//!
//! `format_specific["csv_min_significant_digits"]` (an integer from 1 to 17,
//! default **2**) restricts embedding to cells whose mantissa has at least
//! that many significant digits.  Significant digits are counted from the
//! first non-zero digit to the end of the mantissa: leading zeros do not
//! count, trailing zeros do, including after the decimal point and in
//! integers (`0.70` → 2, `0.007` → 1, `100` → 3, `6.02E+23` → 3).  A
//! mantissa of only zeros (`0`, `0.00`) counts as 1.  With the default, a
//! single-digit cell such as `1e-5`, `3`, `0.7` or `0` is never modified, so
//! no value changes by more than 10% of itself (a 100% change such as
//! `1e-5` → `0e-5` is possible only with threshold 1, which is how the
//! handler behaved before the threshold existed).  Embedding never changes a cell's significant-digit count.
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
//! The bits are the envelope stream (`"CSVJ"`, strategy byte, length, then
//! the [`crate::codec::encode_payload`] output), written LSB first into
//! eligible cells in file order.  The strategy byte is the significant-digit
//! threshold `t` (`1..=17`).  Files written before the threshold existed
//! always have `1`, which is exactly threshold 1, so they still extract.  The extractor
//! does not need the threshold: it runs one envelope reader per threshold
//! over the same cells, each fed the cells its threshold admits and
//! expecting its own strategy byte, and keeps the one whose 40-bit
//! magic-and-strategy prefix matches.
//!
//! Capacity is exactly one bit per eligible cell; `recommended_bits` is that
//! minus the 72-bit envelope header, i.e. the largest codec frame that fits.
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
/// `format_specific` key: minimum significant digits of an eligible cell.
pub(crate) const MIN_DIGITS_KEY: &str = "csv_min_significant_digits";
/// Default `csv_min_significant_digits`.
pub(crate) const DEFAULT_MIN_DIGITS: u8 = 2;
/// Largest accepted `csv_min_significant_digits` (an `f64` needs at most 17
/// significant digits to round-trip).
pub(crate) const MAX_MIN_DIGITS: u8 = 17;
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

/// Number of significant digits in `mantissa` (an eligible field up to and
/// including its carrier digit): every digit from the first non-zero digit
/// to the end, so leading zeros do not count and trailing zeros do
/// (`0.70` → 2, `0.007` → 1, `100` → 3, `-3.140` → 4).  A mantissa that is
/// all zeros (`0`, `0.00`) counts as 1.  Saturates at 255.
///
/// Embedding never changes this count: with two or more significant digits
/// the first non-zero digit is not the carrier digit, and a count of 1 stays
/// 1 whichever parity the carrier digit takes.
pub(crate) fn significant_digits(mantissa: &[u8]) -> u8 {
    let n = mantissa
        .iter()
        .filter(|c| c.is_ascii_digit())
        .skip_while(|&&c| c == b'0')
        .count();
    n.clamp(1, 255) as u8
}

/// An eligible cell found by [`CsvScanner::scan_line`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Cell {
    /// Offset of the carrier digit within the line.
    pub(crate) offset: usize,
    /// [`significant_digits`] of the cell's mantissa.
    pub(crate) sig: u8,
}

/// The `csv_min_significant_digits` threshold (default
/// [`DEFAULT_MIN_DIGITS`]): an integer in `1..=MAX_MIN_DIGITS`, given as a
/// JSON number or a string of digits.
pub(crate) fn min_digits(params: &EmbedParams) -> Result<u8> {
    let Some(v) = params.format_specific.get(MIN_DIGITS_KEY) else {
        return Ok(DEFAULT_MIN_DIGITS);
    };
    let n = v
        .as_u64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()));
    match n {
        Some(n) if (1..=u64::from(MAX_MIN_DIGITS)).contains(&n) => Ok(n as u8),
        _ => Err(SteganoError::InvalidParameter(format!(
            "{MIN_DIGITS_KEY} must be an integer from 1 to {MAX_MIN_DIGITS}"
        ))),
    }
}

/// Envelope strategy byte for the digit strategy with threshold `t`.
pub(crate) fn strategy_byte(t: u8) -> u8 {
    debug_assert!((1..=MAX_MIN_DIGITS).contains(&t));
    t
}

/// Extractor that does not need to know the threshold: one
/// [`EnvelopeReader`] per possible threshold runs in parallel over the same
/// cells, each fed only the cells it would have used, and expecting its own
/// strategy byte.  All but the right one fail on the 40-bit magic and
/// strategy check.  Shared by the in-memory and streaming paths.
pub(crate) struct CsvExtractor {
    /// `(threshold, reader)`; `None` once the reader has failed.
    readers: Vec<(u8, Option<EnvelopeReader>)>,
    /// Error from a reader whose magic and strategy matched (more useful
    /// than "not found").
    matched_err: Option<SteganoError>,
}

impl CsvExtractor {
    /// `max_len(t)` is the largest declared body length accepted for
    /// threshold `t` (`usize::MAX` when unknown).
    pub(crate) fn new(max_len: impl Fn(u8) -> usize) -> Self {
        Self {
            readers: (1..=MAX_MIN_DIGITS)
                .map(|t| {
                    let r = EnvelopeReader::new(MAGIC, strategy_byte(t), max_len(t), WHAT);
                    (t, Some(r))
                })
                .collect(),
            matched_err: None,
        }
    }

    /// Feed the bit of one cell.  Returns `Some` once the outcome is known
    /// (an envelope was read completely, or every reader has failed).
    pub(crate) fn push(&mut self, cell: Cell, bit: u8) -> Option<Result<Vec<u8>>> {
        let mut alive = false;
        for (t, slot) in &mut self.readers {
            let Some(r) = slot.as_mut() else { continue };
            if *t <= cell.sig {
                match r.push(bit) {
                    Ok(false) => {}
                    Ok(true) => return slot.take().map(EnvelopeReader::finish),
                    Err(e) => {
                        if r.prefix_matches() {
                            self.matched_err.get_or_insert(e);
                        }
                        *slot = None;
                        continue;
                    }
                }
            }
            alive = true;
        }
        (!alive).then(|| Err(self.failure()))
    }

    fn failure(&mut self) -> SteganoError {
        self.matched_err.take().unwrap_or_else(|| envelope::not_found(WHAT))
    }

    /// The outcome at the end of the carrier.
    pub(crate) fn finish(mut self) -> Result<Vec<u8>> {
        if let Some(r) = self
            .readers
            .iter_mut()
            .find_map(|(_, r)| r.take_if(|r| r.header_read()))
        {
            return r.finish();
        }
        Err(self.failure())
    }
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
    cells: Vec<Cell>,
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

    /// The numeric cells on this line (offsets into `body`, the line without
    /// its terminator), in order, regardless of the significant-digit
    /// threshold; callers filter with [`Cell::sig`].
    pub(crate) fn scan_line(&mut self, body: &[u8]) -> &[Cell] {
        self.cells.clear();
        let mut non_numeric = false;
        let mut any = false;
        let mut finish = |cells: &mut Vec<Cell>, s: usize, e: usize| {
            if e > s {
                any = true;
                match carrier_digit(&body[s..e]) {
                    Some(off) => cells.push(Cell {
                        offset: s + off,
                        sig: significant_digits(&body[s..=s + off]),
                    }),
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

/// All numeric cells of `data` (with absolute carrier-digit offsets),
/// whatever their significant digits, after checking that `data` is text.
fn carrier_cells(data: &[u8], delim: u8) -> Result<Vec<Cell>> {
    let mut scanner = CsvScanner::new(delim);
    let mut out = Vec::new();
    let mut base = 0;
    for seg in data.split_inclusive(|&b| b == b'\n') {
        envelope::check_text(seg, base, WHAT)?;
        let (body, _) = split_eol(seg);
        out.extend(scanner.scan_line(body).iter().map(|c| Cell {
            offset: base + c.offset,
            sig: c.sig,
        }));
        base += seg.len();
    }
    Ok(out)
}

/// Number of eligible cells under each threshold: element `t - 1` is the
/// count for `csv_min_significant_digits = t`.
fn eligible_counts(cells: &[Cell]) -> Vec<usize> {
    (1..=MAX_MIN_DIGITS)
        .map(|t| cells.iter().filter(|c| c.sig >= t).count())
        .collect()
}

/// Embed already codec-encoded bytes into the cells with at least
/// `min_digits` significant digits; returns the new carrier bytes.
pub(crate) fn embed_encoded(
    data: &[u8],
    encoded: &[u8],
    delim: u8,
    min_digits: u8,
) -> Result<Vec<u8>> {
    let positions: Vec<usize> = carrier_cells(data, delim)?
        .into_iter()
        .filter(|c| c.sig >= min_digits)
        .map(|c| c.offset)
        .collect();
    let stream = envelope::build(MAGIC, strategy_byte(min_digits), encoded)?;
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

/// Read the codec bytes back out of a carrier, whatever threshold it was
/// embedded with (the threshold is the envelope's strategy byte).
pub(crate) fn extract_encoded(data: &[u8], delim: u8) -> Result<Vec<u8>> {
    let cells = carrier_cells(data, delim)?;
    let counts = eligible_counts(&cells);
    let mut ex =
        CsvExtractor::new(|t| counts[usize::from(t) - 1].saturating_sub(ENVELOPE_BITS) / 8);
    for &c in &cells {
        if let Some(r) = ex.push(c, data[c.offset] & 1) {
            return r;
        }
    }
    ex.finish()
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

    /// Exact capacity for the default delimiter and the default
    /// `csv_min_significant_digits`: one bit per eligible cell.
    /// `recommended_bits` is the largest codec frame (in bits) that fits
    /// after the envelope header.  `eligible_cells_by_min_significant_digits`
    /// lists the cell count for every threshold `1..=17` (element `t - 1`).
    fn estimate_capacity(&self, carrier: &CarrierData) -> Result<CapacityInfo> {
        check_format(carrier)?;
        let data = &carrier.raw_data;
        let counts = eligible_counts(&carrier_cells(data, b',')?);
        let cells = counts[usize::from(DEFAULT_MIN_DIGITS) - 1];
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
        fi.insert("min_significant_digits".into(), DEFAULT_MIN_DIGITS.into());
        fi.insert(
            "eligible_cells_by_min_significant_digits".into(),
            counts.into(),
        );
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
        let min = min_digits(params)?;
        // Structural check before the (expensive) codec.
        carrier_cells(&carrier.raw_data, delim)?;
        let enc_start = Instant::now();
        let encoded = crate::codec::encode_payload(payload, params)?;
        let encryption_time_ms = if params.encryption_key.is_empty() {
            0
        } else {
            (enc_start.elapsed().as_millis() as u64).max(1)
        };
        carrier.raw_data = embed_encoded(&carrier.raw_data, &encoded, delim, min)?;
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
        // Validated for consistency; the extractor reads the threshold from
        // the envelope.
        min_digits(params)?;
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
    /// last-place unit; same shape.  Uses the default threshold.
    fn assert_valid(orig: &[u8], stego: &[u8]) {
        assert_valid_min(orig, stego, DEFAULT_MIN_DIGITS);
    }

    /// [`assert_valid`], plus: cells with fewer than `min` significant
    /// digits are unchanged.
    fn assert_valid_min(orig: &[u8], stego: &[u8], min: u8) {
        let (a, b) = (records(orig), records(stego));
        assert_eq!(a.len(), b.len());
        assert_eq!(a[0], b[0], "header changed");
        for (ra, rb) in a.iter().zip(&b) {
            assert_eq!(ra.len(), rb.len());
            for (ca, cb) in ra.iter().zip(rb) {
                if let Some(off) = carrier_digit(ca.as_bytes()) {
                    if significant_digits(&ca.as_bytes()[..=off]) < min {
                        assert_eq!(ca, cb, "cell below the threshold changed");
                    }
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

    fn sig(f: &str) -> u8 {
        let off = carrier_digit(f.as_bytes()).unwrap();
        significant_digits(&f.as_bytes()[..=off])
    }

    #[test]
    fn significant_digit_count() {
        for (f, n) in [
            ("1e-5", 1),
            ("3", 1),
            ("0.7", 1),
            ("-0.007", 1),
            ("0", 1),
            ("0.00", 1),
            ("+0e3", 1),
            ("0.70", 2),
            ("12", 2),
            ("00012", 2),
            ("100", 3),
            ("-3.140", 4),
            (".05", 1),
            ("6.02E+23", 3),
            ("1.0e-5", 2),
            ("123456789.0123456789", 19),
        ] {
            assert_eq!(sig(f), n, "{f}");
        }
        // Flipping the carrier digit never changes the count.
        for f in ["0.01", "0.10", "1", "0", "10", "19", "0.000", "2.5e3"] {
            let mut b = f.as_bytes().to_vec();
            let off = carrier_digit(&b).unwrap();
            b[off] ^= 1;
            assert_eq!(significant_digits(&b[..=off]), sig(f), "{f}");
        }
    }

    #[test]
    fn min_digits_param() {
        assert_eq!(min_digits(&params("")).unwrap(), DEFAULT_MIN_DIGITS);
        for (v, want) in [
            (serde_json::Value::from(1), Some(1)),
            (serde_json::Value::from(3), Some(3)),
            (serde_json::Value::from("2"), Some(2)),
            (serde_json::Value::from(17), Some(17)),
            (serde_json::Value::from(0), None),
            (serde_json::Value::from(18), None),
            (serde_json::Value::from(-1), None),
            (serde_json::Value::from(1.5), None),
            (serde_json::Value::from("two"), None),
        ] {
            let mut p = params("");
            p.format_specific.insert(MIN_DIGITS_KEY.into(), v.clone());
            assert_eq!(min_digits(&p).ok(), want, "{v}");
        }
    }

    /// Rows mixing single-digit cells (`3`, `0.7`, `1e-5`, `0`) with
    /// multi-digit ones.
    fn mixed_csv(rows: usize) -> Vec<u8> {
        let mut s = String::from("a,b,c,d,e,f\n");
        for i in 0..rows {
            s.push_str(&format!(
                "{},0.{},{}e-5,0,{}.{:02},-0.{:03}\n",
                i % 10,
                i % 10,
                i % 9 + 1,
                i + 1,
                i % 100,
                (i * 7) % 1000 + 10
            ));
        }
        s.into_bytes()
    }

    #[tokio::test]
    async fn threshold_skips_single_digit_cells() {
        let data = mixed_csv(500);
        let h = CsvHandler::new();
        for min in [1u8, 2, 3] {
            for pw in ["", "pw"] {
                let mut p = params(pw);
                p.format_specific
                    .insert(MIN_DIGITS_KEY.into(), serde_json::Value::from(min));
                let mut c = carrier(&data);
                h.embed(&mut c, b"threshold", &p).await.unwrap();
                assert_valid_min(&data, &c.raw_data, min);
                // Extraction needs no threshold (it is in the envelope).
                assert_eq!(h.extract(&c, &params(pw)).await.unwrap(), b"threshold");
                assert_eq!(h.extract(&c, &p).await.unwrap(), b"threshold");
                assert!(h.verify_integrity(&c).unwrap());
            }
        }
        // Default threshold: columns a-d (one significant digit) untouched.
        let mut c = carrier(&data);
        h.embed(&mut c, &[0x5a; 40], &params("")).await.unwrap();
        let (a, b) = (records(&data), records(&c.raw_data));
        assert!(a.iter().zip(&b).all(|(x, y)| x[..4] == y[..4]));
        assert_ne!(a, b);
        // Threshold 1 does use them.
        let mut p = params("");
        p.format_specific
            .insert(MIN_DIGITS_KEY.into(), serde_json::Value::from(1));
        let mut c = carrier(&data);
        h.embed(&mut c, &[0x5a; 40], &p).await.unwrap();
        let b = records(&c.raw_data);
        assert!(a.iter().zip(&b).any(|(x, y)| x[..4] != y[..4]));
        // A bad threshold is rejected on embed and extract.
        let mut bad = params("");
        bad.format_specific
            .insert(MIN_DIGITS_KEY.into(), serde_json::Value::from(0));
        assert!(h.embed(&mut carrier(&data), b"x", &bad).await.is_err());
        assert!(h.extract(&c, &bad).await.is_err());
    }

    /// The layout from before the threshold existed (#97, also called the
    /// v0.7.0-style envelope), written independently of `embed_encoded`: every
    /// numeric cell, strategy byte 1.
    fn embed_v070(data: &[u8], encoded: &[u8]) -> Vec<u8> {
        let stream = envelope::build(b"CSVJ", 1, encoded).unwrap();
        let mut out = data.to_vec();
        let mut cells = CsvScanner::new(b',');
        let mut base = 0;
        let mut i = 0;
        for seg in data.split_inclusive(|&b| b == b'\n') {
            let (body, _) = split_eol(seg);
            for c in cells.scan_line(body) {
                if i < stream.len() * 8 {
                    set_parity(&mut out[base + c.offset], envelope::bit(&stream, i));
                    i += 1;
                }
            }
            base += seg.len();
        }
        assert_eq!(i, stream.len() * 8, "carrier too small for the test");
        out
    }

    #[tokio::test]
    async fn v070_files_still_extract() {
        let h = CsvHandler::new();
        for data in [mixed_csv(500), sample_csv(400)] {
            for pw in ["", "old secret"] {
                let p = params(pw);
                let encoded = crate::codec::encode_payload(b"made by v0.7.0", &p).unwrap();
                let old = embed_v070(&data, &encoded);
                assert_eq!(h.extract(&carrier(&old), &p).await.unwrap(), b"made by v0.7.0");
                assert!(h.verify_integrity(&carrier(&old)).unwrap());
                // Threshold 1 reproduces the v0.7.0 bytes exactly.
                assert_eq!(embed_encoded(&data, &encoded, b',', 1).unwrap(), old);
                // The default threshold does not.
                assert_ne!(embed_encoded(&data, &encoded, b',', 2).unwrap(), old);
            }
        }
    }

    /// The threshold scan works on raw envelope bits only: it recovers an
    /// arbitrary (non-codec) frame for every threshold, so `extract` runs the
    /// codec decode (and its key derivation) exactly once, on the result.
    #[test]
    fn threshold_scan_needs_no_decode() {
        let data = mixed_csv(300);
        let frame: Vec<u8> = (0u8..20).map(|i| i.wrapping_mul(37) ^ 0xA5).collect();
        for t in [1u8, 2, 3, 4] {
            let stego = embed_encoded(&data, &frame, b',', t).unwrap();
            assert_eq!(extract_encoded(&stego, b',').unwrap(), frame, "t={t}");
            assert!(crate::codec::decode_payload(&frame, &params("")).is_err());
        }
    }

    #[test]
    fn capacity_per_threshold() {
        let data = mixed_csv(300);
        let info = CsvHandler.estimate_capacity(&carrier(&data)).unwrap();
        let fi = &info.format_specific_info;
        let by: Vec<usize> = fi["eligible_cells_by_min_significant_digits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as usize)
            .collect();
        assert_eq!(by.len(), usize::from(MAX_MIN_DIGITS));
        assert_eq!(by[0], 300 * 6);
        // Columns e and f only (`{i}.{:02}` and `-0.{:03}` with >= 10).
        assert_eq!(by[1], 300 * 2);
        assert_eq!(fi["eligible_cells"].as_u64().unwrap() as usize, by[1]);
        assert_eq!(fi["min_significant_digits"], 2);
        assert_eq!(info.recommended_bits, (by[1] - ENVELOPE_BITS) / 8 * 8);
        // Each threshold's count is exactly its embed limit.
        for t in [1u8, 2, 3, 4] {
            let n = by[usize::from(t) - 1];
            let frame = (n - ENVELOPE_BITS) / 8;
            assert!(embed_encoded(&data, &vec![0; frame], b',', t).is_ok(), "t={t}");
            match embed_encoded(&data, &vec![0; frame + 1], b',', t) {
                Err(SteganoError::InsufficientCapacity { available, .. }) => {
                    assert_eq!(available, n, "t={t}")
                }
                other => panic!("t={t}: {other:?}"),
            }
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
        let cell = |offset, sig| Cell { offset, sig };
        assert_eq!(s.scan_line(b"12,x,3.5"), &[cell(1, 2), cell(7, 2)]);
        assert_eq!(s.scan_line(b"\"q,\"\"1\"\"\",2"), &[cell(10, 1)]);
    }
}
