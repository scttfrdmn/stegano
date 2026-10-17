//! FITS (Flexible Image Transport System) format handler.
//!
//! FITS is the standard format for astronomical images and data from
//! telescopes, satellites, and observatories.
//!
//! # FITS structure
//! A FITS file is a sequence of Header-Data Units (HDUs). Each header is a
//! sequence of 80-byte ASCII "cards" grouped into 2880-byte blocks and
//! terminated by an `END` card. The data that follows is
//! `|BITPIX|/8 × GCOUNT × (PCOUNT + NAXIS1 × … × NAXISn)` bytes (zero if
//! `NAXIS = 0`), padded to a multiple of 2880 bytes. The first HDU starts
//! with `SIMPLE = T`; each extension starts with `XTENSION`. Samples are
//! big-endian.
//!
//! The handler parses every header (`BITPIX`, `NAXIS`, `NAXISn`, `PCOUNT`,
//! `GCOUNT`, `XTENSION`, `GROUPS`, `BLANK`) and walks all HDUs. A malformed
//! or truncated header or data unit is an error. Bytes after the last HDU
//! that do not start with `XTENSION` (FITS "special records") are left alone.
//!
//! # Where data is embedded
//! The payload goes into the first image HDU (the primary HDU or an
//! `XTENSION = 'IMAGE'` extension, with `PCOUNT = 0`, `GCOUNT = 1` and at
//! least one pixel) whose `BITPIX` suits the strategy:
//!
//! * `pixel_lsb` (default): integer `BITPIX` 8/16/32/64. Only the least
//!   significant bit of the least significant (last, big-endian) byte of
//!   each sample changes, so every stored value changes by at most ±1. With
//!   `BSCALE`/`BZERO` scaling, this is ±`BSCALE` in physical units. If the
//!   header has a `BLANK` keyword, samples whose value is `BLANK` or
//!   `BLANK ^ 1` are skipped. Null pixels stay null, and no valid pixel can
//!   become null.
//! * `float_mantissa` (opt-in, via `format_specific["fits_strategy"]`):
//!   IEEE-754 `BITPIX` -32/-64. Only the mantissa LSB (the LSB of the last
//!   byte) changes, so the sign and exponent never do. NaN and ±Inf samples
//!   are skipped. Embedding into float data without this opt-in is an error.
//!   An LSB change in a float is a relative change of about 2⁻²³ (or 2⁻⁵²).
//!   It is still a change to scientific data.
//!
//! Header blocks, the padding after the data, other HDUs, and any trailing
//! bytes are never modified. The file length does not change.
//!
//! Capacity is exact: 1 bit per usable sample in the target HDU.
//!
//! # Wire format
//! The bit stream (LSB first, one bit per usable sample) is a 10-byte
//! header, `b"FTSJ"`, followed by the payload length (u32 LE), the strategy
//! byte (0 = `pixel_lsb`, 1 = `float_mantissa`), and the format version (`2`).
//! The header is followed by the output of [`codec::encode_payload`]. There
//! is no legacy or raw mode: every payload goes through the codec. On
//! extract, the header magic is checked before any (expensive) decoding.

use super::DataHandler;
use crate::codec;
use crate::core::*;
use async_trait::async_trait;
use std::collections::HashMap;
use crate::time::Instant;

/// FITS block size in bytes.
const BLOCK: usize = 2880;
/// FITS header card size in bytes.
const CARD: usize = 80;
/// Steganographic header magic.
const MAGIC: &[u8; 4] = b"FTSJ";
/// Steganographic header version (matches codec wire format v2).
const FORMAT_VERSION: u8 = 2;
/// Steganographic header length in bytes.
const HEADER_LEN: usize = 10;
/// Steganographic header length in carrier bits.
const HEADER_BITS: usize = HEADER_LEN * 8;
/// Maximum `NAXIS` allowed by the FITS standard.
const MAX_NAXIS: i64 = 999;
/// `format_specific` key selecting the strategy.
const STRATEGY_KEY: &str = "fits_strategy";

/// FITS file handler. Embeds in sample LSBs of the first suitable image HDU.
pub struct FitsHandler;

/// Embedding strategy (stored as the strategy byte of the header).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FitsStrategy {
    /// LSB of integer samples (BITPIX 8/16/32/64).
    PixelLsb = 0,
    /// Mantissa LSB of finite float samples (BITPIX -32/-64).
    FloatMantissa = 1,
}

impl FitsStrategy {
    const ALL: [FitsStrategy; 2] = [FitsStrategy::PixelLsb, FitsStrategy::FloatMantissa];

    fn to_u8(self) -> u8 {
        self as u8
    }

    fn name(self) -> &'static str {
        match self {
            FitsStrategy::PixelLsb => "pixel_lsb",
            FitsStrategy::FloatMantissa => "float_mantissa",
        }
    }

    fn accepts(self, bitpix: i64) -> bool {
        match self {
            FitsStrategy::PixelLsb => bitpix > 0,
            FitsStrategy::FloatMantissa => bitpix < 0,
        }
    }
}

/// One parsed Header-Data Unit.
#[derive(Debug, Clone)]
struct Hdu {
    /// Offset of the first data byte (just past the header blocks).
    data_start: usize,
    /// End of the data unit including padding, clamped to the file length.
    next_start: usize,
    bitpix: i64,
    /// Number of samples in the pixel array (product of the NAXISn).
    pixels: usize,
    pcount: i64,
    gcount: i64,
    /// Primary HDU (not random groups) or `XTENSION = 'IMAGE'`.
    is_image: bool,
    /// `BLANK` value (integer images only).
    blank: Option<i64>,
}

impl Hdu {
    fn width(&self) -> usize {
        // BITPIX is validated to be one of ±8, 16, 32, 64.
        (self.bitpix.unsigned_abs() / 8) as usize
    }

    fn is_target(&self, strategy: FitsStrategy) -> bool {
        self.is_image
            && self.pixels > 0
            && self.pcount == 0
            && self.gcount == 1
            && strategy.accepts(self.bitpix)
    }
}

fn malformed(msg: impl std::fmt::Display) -> SteganoError {
    SteganoError::InvalidParameter(format!("Malformed FITS file: {msg}"))
}

/// Parse the value field (columns 11-80) of a card: a quoted string is
/// unquoted (with `''` → `'`), otherwise the text before any `/` comment.
fn card_value(field: &str) -> String {
    let s = field.trim_start();
    if let Some(rest) = s.strip_prefix('\'') {
        let mut out = String::new();
        let mut chars = rest.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\'' {
                if chars.peek() == Some(&'\'') {
                    chars.next();
                    out.push('\'');
                } else {
                    break;
                }
            } else {
                out.push(c);
            }
        }
        out.trim_end().to_string()
    } else {
        s.split('/').next().unwrap_or("").trim().to_string()
    }
}

/// Parsed keyword/value pairs of one header (first occurrence wins).
struct Cards {
    first_keyword: String,
    values: HashMap<String, String>,
}

impl Cards {
    fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    fn int(&self, key: &str) -> Result<Option<i64>> {
        match self.get(key) {
            None => Ok(None),
            Some(v) => v
                .parse::<i64>()
                .map(Some)
                .map_err(|_| malformed(format!("{key} value {v:?} is not an integer"))),
        }
    }

    fn required_int(&self, key: &str) -> Result<i64> {
        self.int(key)?
            .ok_or_else(|| malformed(format!("missing required keyword {key}")))
    }
}

impl FitsHandler {
    pub fn new() -> Self {
        Self
    }

    /// Check FITS magic: first 8 bytes must be `SIMPLE  `.
    fn is_fits(data: &[u8]) -> bool {
        data.len() >= 8 && &data[..8] == b"SIMPLE  "
    }

    /// Read the header blocks starting at `start`. Returns the cards and the
    /// offset just past the last header block.
    fn read_cards(data: &[u8], start: usize) -> Result<(Cards, usize)> {
        let mut values = HashMap::new();
        let mut first_keyword = None;
        let mut pos = start;
        loop {
            let block_end = pos
                .checked_add(BLOCK)
                .filter(|&e| e <= data.len())
                .ok_or_else(|| malformed(format!("truncated header at offset {pos}")))?;
            let mut found_end = false;
            for card in data[pos..block_end].as_chunks::<CARD>().0 {
                if !card.iter().all(|&b| (0x20..=0x7E).contains(&b)) {
                    return Err(malformed(format!(
                        "non-ASCII header card in header at offset {start}"
                    )));
                }
                // Checked ASCII above, so this cannot fail.
                let text = std::str::from_utf8(card).map_err(malformed)?;
                let keyword = text[..8].trim_end();
                if first_keyword.is_none() {
                    first_keyword = Some(keyword.to_string());
                }
                if keyword == "END" {
                    if !text[8..].bytes().all(|b| b == b' ') {
                        return Err(malformed("END card is not blank-padded"));
                    }
                    found_end = true;
                    break;
                }
                if &text[8..10] == "= " && !keyword.is_empty() {
                    values
                        .entry(keyword.to_string())
                        .or_insert_with(|| card_value(&text[10..]));
                }
            }
            pos = block_end;
            if found_end {
                let first_keyword = first_keyword.unwrap_or_default();
                return Ok((
                    Cards {
                        first_keyword,
                        values,
                    },
                    pos,
                ));
            }
        }
    }

    /// Parse one HDU whose header starts at `start`.
    fn parse_hdu(data: &[u8], start: usize, primary: bool) -> Result<Hdu> {
        let (cards, data_start) = Self::read_cards(data, start)?;

        let mut random_groups = false;
        let is_image = if primary {
            if cards.first_keyword != "SIMPLE" || cards.get("SIMPLE") != Some("T") {
                return Err(malformed("primary header does not start with SIMPLE = T"));
            }
            random_groups = cards.get("GROUPS") == Some("T");
            !random_groups
        } else {
            if cards.first_keyword != "XTENSION" {
                return Err(malformed(format!(
                    "extension at offset {start} does not start with XTENSION"
                )));
            }
            cards.get("XTENSION") == Some("IMAGE")
        };

        let bitpix = cards.required_int("BITPIX")?;
        if ![8, 16, 32, 64, -32, -64].contains(&bitpix) {
            return Err(malformed(format!("invalid BITPIX {bitpix}")));
        }
        let naxis = cards.required_int("NAXIS")?;
        if !(0..=MAX_NAXIS).contains(&naxis) {
            return Err(malformed(format!("invalid NAXIS {naxis}")));
        }
        let mut axes = Vec::new();
        for n in 1..=naxis {
            let key = format!("NAXIS{n}");
            let len = cards.required_int(&key)?;
            let len = u64::try_from(len).map_err(|_| malformed(format!("negative {key}")))?;
            axes.push(len);
        }
        let pcount = cards.int("PCOUNT")?.unwrap_or(0);
        let gcount = cards.int("GCOUNT")?.unwrap_or(1);
        if pcount < 0 || gcount < 0 {
            return Err(malformed("negative PCOUNT or GCOUNT"));
        }
        let blank = if bitpix > 0 {
            cards.int("BLANK")?
        } else {
            None
        };

        // Random groups: NAXIS1 = 0 is a placeholder and excluded from the product.
        let counted = if random_groups && axes.first() == Some(&0) {
            &axes[1..]
        } else {
            &axes[..]
        };
        let overflow = || malformed("data size overflows");
        let pixels: u64 = if naxis == 0 {
            0
        } else {
            counted
                .iter()
                .try_fold(1u64, |acc, &a| acc.checked_mul(a))
                .ok_or_else(overflow)?
        };
        let width = bitpix.unsigned_abs() / 8;
        let data_len = (pcount as u64)
            .checked_add(pixels)
            .and_then(|v| v.checked_mul(gcount as u64))
            .and_then(|v| v.checked_mul(width))
            .ok_or_else(overflow)?;
        let data_len = usize::try_from(data_len).map_err(|_| overflow())?;
        let pixels = usize::try_from(pixels).map_err(|_| overflow())?;

        let data_end = data_start.checked_add(data_len).ok_or_else(overflow)?;
        if data_end > data.len() {
            return Err(malformed(format!(
                "data unit at offset {data_start} declares {data_len} bytes but only {} remain",
                data.len() - data_start
            )));
        }
        let padded = data_len
            .checked_next_multiple_of(BLOCK)
            .and_then(|p| data_start.checked_add(p))
            .ok_or_else(overflow)?;

        Ok(Hdu {
            data_start,
            next_start: padded.min(data.len()),
            bitpix,
            pixels,
            pcount,
            gcount,
            is_image,
            blank,
        })
    }

    /// Parse and walk every HDU in the file.
    fn parse_hdus(data: &[u8]) -> Result<Vec<Hdu>> {
        if !Self::is_fits(data) {
            return Err(malformed("missing SIMPLE keyword at start of file"));
        }
        let mut hdus = vec![Self::parse_hdu(data, 0, true)?];
        let mut pos = hdus[0].next_start;
        while data.len().saturating_sub(pos) >= 8 && &data[pos..pos + 8] == b"XTENSION" {
            let hdu = Self::parse_hdu(data, pos, false)?;
            pos = hdu.next_start;
            hdus.push(hdu);
        }
        Ok(hdus)
    }

    fn select_strategy(params: &EmbedParams) -> Result<FitsStrategy> {
        match params.format_specific.get(STRATEGY_KEY) {
            None => Ok(FitsStrategy::PixelLsb),
            Some(v) => match v.as_str() {
                Some("pixel_lsb") | Some("integer_lsb") | Some("lsb") => Ok(FitsStrategy::PixelLsb),
                Some("float_mantissa") => Ok(FitsStrategy::FloatMantissa),
                _ => Err(SteganoError::InvalidParameter(format!(
                    "Unknown {STRATEGY_KEY} {v}; expected \"pixel_lsb\" or \"float_mantissa\""
                ))),
            },
        }
    }

    fn target(hdus: &[Hdu], strategy: FitsStrategy) -> Option<&Hdu> {
        hdus.iter().find(|h| h.is_target(strategy))
    }

    /// The target HDU for `strategy`, or an error explaining why there is none.
    fn require_target(hdus: &[Hdu], strategy: FitsStrategy) -> Result<&Hdu> {
        if let Some(h) = Self::target(hdus, strategy) {
            return Ok(h);
        }
        Err(SteganoError::InvalidParameter(match strategy {
            FitsStrategy::PixelLsb => match Self::target(hdus, FitsStrategy::FloatMantissa) {
                Some(f) => format!(
                    "FITS image data is floating point (BITPIX = {}); flipping raw byte LSBs \
                     would corrupt it. Set format_specific[\"{STRATEGY_KEY}\"] = \
                     \"float_mantissa\" to embed in mantissa LSBs (this alters scientific data)",
                    f.bitpix
                ),
                None => "FITS file has no integer image HDU with pixel data".to_string(),
            },
            FitsStrategy::FloatMantissa => {
                "FITS file has no floating-point image HDU with pixel data \
                 (use the default pixel_lsb strategy for integer images)"
                    .to_string()
            }
        }))
    }

    /// Whether a sample (big-endian bytes) may carry a bit.
    fn usable(sample: &[u8], bitpix: i64, blank: Option<i64>) -> bool {
        match bitpix {
            // Exponent all ones => NaN or Inf.
            -32 => !(sample[0] & 0x7F == 0x7F && sample[1] & 0x80 == 0x80),
            -64 => !(sample[0] & 0x7F == 0x7F && sample[1] & 0xF0 == 0xF0),
            _ => match blank {
                None => true,
                Some(b) => {
                    let v = match bitpix {
                        8 => sample[0] as i64,
                        16 => i16::from_be_bytes([sample[0], sample[1]]) as i64,
                        32 => {
                            i32::from_be_bytes([sample[0], sample[1], sample[2], sample[3]]) as i64
                        }
                        _ => {
                            let mut b8 = [0u8; 8];
                            b8.copy_from_slice(&sample[..8]);
                            i64::from_be_bytes(b8)
                        }
                    };
                    // Skip both BLANK and its LSB twin, so the set of usable
                    // samples is unchanged by embedding.
                    (v | 1) != (b | 1)
                }
            },
        }
    }

    /// Byte offsets (the last byte of each usable sample) that carry bits, in order.
    fn slots<'a>(data: &'a [u8], hdu: &Hdu) -> impl Iterator<Item = usize> + 'a {
        let (w, start, bitpix, blank) = (hdu.width(), hdu.data_start, hdu.bitpix, hdu.blank);
        // parse_hdu verified data_start + pixels * w <= data.len().
        (0..hdu.pixels).filter_map(move |i| {
            let s = start + i * w;
            Self::usable(&data[s..s + w], bitpix, blank).then_some(s + w - 1)
        })
    }

    fn capacity_bits(data: &[u8], hdu: &Hdu) -> usize {
        Self::slots(data, hdu).count()
    }

    fn read_bits(data: &[u8], hdu: &Hdu, n: usize) -> Vec<u8> {
        let bits: Vec<u8> = Self::slots(data, hdu)
            .take(n)
            .map(|i| data[i] & 1)
            .collect();
        bits.chunks(8)
            .map(|c| {
                c.iter()
                    .enumerate()
                    .fold(0u8, |acc, (i, &b)| acc | (b << i))
            })
            .collect()
    }

    /// Read the steganographic header for `strategy`, if present.
    /// Returns the declared payload length.
    fn find_header(data: &[u8], hdu: &Hdu, strategy: FitsStrategy) -> Option<Result<usize>> {
        let header = Self::read_bits(data, hdu, HEADER_BITS);
        if header.len() < HEADER_LEN || &header[..4] != MAGIC || header[8] != strategy.to_u8() {
            return None;
        }
        if header[9] != FORMAT_VERSION {
            return Some(Err(SteganoError::IntegrityError(format!(
                "unsupported FITS payload version {} (expected {FORMAT_VERSION})",
                header[9]
            ))));
        }
        Some(Ok(
            u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize,
        ))
    }

    fn extract_with_strategy(
        data: &[u8],
        hdus: &[Hdu],
        strategy: FitsStrategy,
        params: &EmbedParams,
    ) -> Option<Result<Vec<u8>>> {
        let hdu = Self::target(hdus, strategy)?;
        let len = match Self::find_header(data, hdu, strategy)? {
            Ok(len) => len,
            Err(e) => return Some(Err(e)),
        };
        let cap = Self::capacity_bits(data, hdu);
        let total_bits = match len
            .checked_add(HEADER_LEN)
            .and_then(|t| t.checked_mul(8))
            .filter(|&t| t <= cap)
        {
            Some(t) => t,
            None => {
                return Some(Err(SteganoError::IntegrityError(format!(
                    "declared FITS payload length {len} exceeds carrier capacity ({cap} bits)"
                ))))
            }
        };
        let all = Self::read_bits(data, hdu, total_bits);
        Some(codec::decode_payload(&all[HEADER_LEN..], params))
    }
}

impl Default for FitsHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DataHandler for FitsHandler {
    fn detect_format(&self, data: &[u8]) -> Option<DataFormat> {
        if Self::is_fits(data) {
            Some(DataFormat::FITS)
        } else {
            None
        }
    }

    /// Capacity of the default `pixel_lsb` strategy. If the file only has
    /// float image data, `total_bits` is 0 and
    /// `format_specific_info["float_mantissa_capacity_bits"]` gives the
    /// opt-in capacity.
    fn estimate_capacity(&self, carrier: &CarrierData) -> Result<CapacityInfo> {
        let data = &carrier.raw_data;
        let hdus = Self::parse_hdus(data)?;
        let int_target = Self::target(&hdus, FitsStrategy::PixelLsb);
        let total = int_target.map_or(0, |h| Self::capacity_bits(data, h));
        let mut info = CapacityInfo::new(total, total.saturating_sub(HEADER_BITS));
        let fsi = &mut info.format_specific_info;
        fsi.insert("hdu_count".to_string(), hdus.len().into());
        fsi.insert("overhead_bytes".to_string(), HEADER_LEN.into());
        fsi.insert("strategy".to_string(), FitsStrategy::PixelLsb.name().into());
        if let Some(h) = int_target {
            let index = hdus.iter().position(|x| std::ptr::eq(x, h)).unwrap_or(0);
            fsi.insert("hdu_index".to_string(), index.into());
            fsi.insert("bitpix".to_string(), h.bitpix.into());
            fsi.insert("data_offset".to_string(), h.data_start.into());
            fsi.insert("samples".to_string(), h.pixels.into());
        }
        if let Some(h) = Self::target(&hdus, FitsStrategy::FloatMantissa) {
            fsi.insert(
                "float_mantissa_capacity_bits".to_string(),
                Self::capacity_bits(data, h).into(),
            );
        }
        Ok(info)
    }

    async fn embed(
        &self,
        carrier: &mut CarrierData,
        payload: &[u8],
        params: &EmbedParams,
    ) -> Result<EmbedResult> {
        let start = Instant::now();
        let strategy = Self::select_strategy(params)?;
        let hdus = Self::parse_hdus(&carrier.raw_data)?;
        let hdu = Self::require_target(&hdus, strategy)?;
        let available = Self::capacity_bits(&carrier.raw_data, hdu);

        let enc_start = Instant::now();
        let encoded = codec::encode_payload(payload, params)?;
        let encryption_time_ms = if params.encryption_key.is_empty() {
            0
        } else {
            (enc_start.elapsed().as_millis() as u64).max(1)
        };
        let len_u32 = u32::try_from(encoded.len()).map_err(|_| {
            SteganoError::InvalidParameter("payload too large for FITS header".to_string())
        })?;

        let mut stream = Vec::with_capacity(HEADER_LEN + encoded.len());
        stream.extend_from_slice(MAGIC);
        stream.extend_from_slice(&len_u32.to_le_bytes());
        stream.push(strategy.to_u8());
        stream.push(FORMAT_VERSION);
        stream.extend_from_slice(&encoded);

        let needed = stream
            .len()
            .checked_mul(8)
            .ok_or_else(|| SteganoError::InvalidParameter("payload too large".to_string()))?;
        if needed > available {
            return Err(SteganoError::InsufficientCapacity { needed, available });
        }

        let positions: Vec<usize> = Self::slots(&carrier.raw_data, hdu).take(needed).collect();
        let bits = stream
            .iter()
            .flat_map(|&byte| (0..8).map(move |i| (byte >> i) & 1));
        for (pos, bit) in positions.into_iter().zip(bits) {
            carrier.raw_data[pos] = (carrier.raw_data[pos] & 0xFE) | bit;
        }

        Ok(EmbedResult::success(
            payload.len(),
            PerformanceStats {
                embedding_time_ms: start.elapsed().as_millis() as u64,
                encryption_time_ms,
                gpu_acceleration_used: false,
                memory_usage_bytes: stream.len() + needed * std::mem::size_of::<usize>(),
            },
        ))
    }

    async fn extract(&self, carrier: &CarrierData, params: &EmbedParams) -> Result<Vec<u8>> {
        let hdus = Self::parse_hdus(&carrier.raw_data)?;
        for strategy in FitsStrategy::ALL {
            if let Some(r) = Self::extract_with_strategy(&carrier.raw_data, &hdus, strategy, params)
            {
                return r;
            }
        }
        Err(SteganoError::InvalidParameter(
            "No FITS steganographic payload found".to_string(),
        ))
    }

    fn verify_integrity(&self, carrier: &CarrierData) -> Result<bool> {
        let data = &carrier.raw_data;
        let Ok(hdus) = Self::parse_hdus(data) else {
            return Ok(false);
        };
        Ok(FitsStrategy::ALL.iter().any(|&s| {
            Self::target(&hdus, s).is_some_and(|h| Self::find_header(data, h, s).is_some())
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- FITS builders ----

    fn card(s: &str) -> String {
        assert!(s.len() <= CARD, "card too long: {s}");
        format!("{s:<80}")
    }

    fn int_card(key: &str, v: impl std::fmt::Display) -> String {
        card(&format!("{key:<8}= {v:>20}"))
    }

    fn header(cards: &[String]) -> Vec<u8> {
        let mut h: Vec<u8> = cards.iter().flat_map(|c| c.bytes()).collect();
        h.extend_from_slice(card("END").as_bytes());
        while !h.len().is_multiple_of(BLOCK) {
            h.push(b' ');
        }
        h
    }

    fn axis_cards(bitpix: i64, axes: &[usize]) -> Vec<String> {
        let mut c = vec![int_card("BITPIX", bitpix), int_card("NAXIS", axes.len())];
        for (i, a) in axes.iter().enumerate() {
            c.push(int_card(&format!("NAXIS{}", i + 1), a));
        }
        c
    }

    fn primary_header(bitpix: i64, axes: &[usize], extra: &[String]) -> Vec<u8> {
        let mut c = vec![card(
            "SIMPLE  =                    T / conforms to FITS standard",
        )];
        c.extend(axis_cards(bitpix, axes));
        c.push(card("EXTEND  =                    T"));
        c.extend_from_slice(extra);
        header(&c)
    }

    fn ext_header(xtension: &str, bitpix: i64, axes: &[usize], extra: &[String]) -> Vec<u8> {
        let mut c = vec![card(&format!(
            "XTENSION= '{xtension:<8}'           / extension"
        ))];
        c.extend(axis_cards(bitpix, axes));
        c.push(int_card("PCOUNT", 0));
        c.push(int_card("GCOUNT", 1));
        c.extend_from_slice(extra);
        header(&c)
    }

    fn padded(mut v: Vec<u8>) -> Vec<u8> {
        while !v.len().is_multiple_of(BLOCK) {
            v.push(0);
        }
        v
    }

    /// Deterministic pseudo-random big-endian integer samples.
    fn int_data(bitpix: i64, n: usize) -> Vec<u8> {
        let w = (bitpix / 8) as usize;
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut out = Vec::with_capacity(n * w);
        for _ in 0..n {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            out.extend_from_slice(&x.to_be_bytes()[8 - w..]);
        }
        out
    }

    /// Float samples, with NaN and ±Inf at every 7th position.
    fn float_data(bitpix: i64, n: usize) -> Vec<u8> {
        let mut out = Vec::new();
        for i in 0..n {
            let v = match i % 7 {
                0 => f64::NAN,
                3 if i % 2 == 0 => f64::INFINITY,
                3 => f64::NEG_INFINITY,
                _ => (i as f64 * 0.731).sin() * 1.0e3,
            };
            if bitpix == -32 {
                out.extend_from_slice(&(v as f32).to_be_bytes());
            } else {
                out.extend_from_slice(&v.to_be_bytes());
            }
        }
        out
    }

    fn carrier(raw: Vec<u8>) -> CarrierData {
        CarrierData {
            format: DataFormat::FITS,
            raw_data: raw,
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        }
    }

    fn plain_params() -> EmbedParams {
        EmbedParams {
            encryption_key: vec![],
            error_correction_level: ECLevel::None,
            stealth_level: StealthLevel::High,
            use_gpu: false,
            enable_validation: false,
            format_specific: HashMap::new(),
            cipher_kind: Default::default(),
        }
    }

    fn float_params() -> EmbedParams {
        let mut p = plain_params();
        p.format_specific
            .insert(STRATEGY_KEY.to_string(), "float_mantissa".into());
        p
    }

    /// Single primary image HDU.
    fn simple_image(bitpix: i64, axes: &[usize], data: Vec<u8>) -> (Vec<u8>, usize) {
        let mut f = primary_header(bitpix, axes, &[]);
        let start = f.len();
        f.extend(padded(data));
        (f, start)
    }

    /// Primary NAXIS=0, 16-bit IMAGE extension, BINTABLE extension.
    /// Returns (file, image data start, image data len).
    fn multi_hdu() -> (Vec<u8>, usize, usize) {
        let mut f = primary_header(16, &[], &[card("OBJECT  = 'M31     '")]);
        f.extend(ext_header(
            "IMAGE",
            16,
            &[40, 30],
            &[card("EXTNAME = 'SCI     '")],
        ));
        let img_start = f.len();
        let img = int_data(16, 1200);
        let img_len = img.len();
        f.extend(padded(img));
        f.extend(ext_header(
            "BINTABLE",
            8,
            &[12, 5],
            &[
                int_card("TFIELDS", 1),
                card("TFORM1  = '12A     '"),
                card("EXTNAME = 'TABLE   '"),
            ],
        ));
        f.extend(padded(vec![b'x'; 60]));
        (f, img_start, img_len)
    }

    fn assert_headers_end_aligned(raw: &[u8]) {
        let hdus = FitsHandler::parse_hdus(raw).expect("output must re-parse");
        for h in &hdus {
            assert_eq!(h.data_start % BLOCK, 0);
            // The header preceding the data must contain an END card.
            assert!(raw[..h.data_start]
                .chunks(CARD)
                .any(|c| c.starts_with(b"END     ")));
        }
    }

    // ---- parsing ----

    #[test]
    fn test_detect_format() {
        let handler = FitsHandler::new();
        let (f, _) = simple_image(16, &[10], int_data(16, 10));
        assert_eq!(handler.detect_format(&f), Some(DataFormat::FITS));
        assert_eq!(handler.detect_format(b"random"), None);
        assert!(!FitsHandler::is_fits(b"SIMPLE"));
    }

    #[test]
    fn test_parse_multi_hdu() {
        let (f, img_start, _) = multi_hdu();
        let hdus = FitsHandler::parse_hdus(&f).unwrap();
        assert_eq!(hdus.len(), 3);
        assert_eq!(hdus[0].pixels, 0);
        assert!(hdus[1].is_image);
        assert_eq!(hdus[1].data_start, img_start);
        assert_eq!(hdus[1].pixels, 1200);
        assert!(!hdus[2].is_image);
        assert_eq!(hdus[2].next_start, f.len());
    }

    #[test]
    fn test_card_value() {
        assert_eq!(card_value("                   16 / comment"), "16");
        assert_eq!(card_value(" 'IMAGE   '           / x"), "IMAGE");
        assert_eq!(card_value(" 'O''Neil / x'"), "O'Neil / x");
    }

    #[test]
    fn test_malformed_headers_are_errors() {
        let bad = |cards: Vec<String>, data: usize| {
            let mut f = header(&cards);
            f.extend(vec![0u8; data]);
            FitsHandler::parse_hdus(&f)
        };
        let simple = || card("SIMPLE  =                    T");
        // Invalid BITPIX
        assert!(bad(
            vec![simple(), int_card("BITPIX", 12), int_card("NAXIS", 0)],
            0
        )
        .is_err());
        // Missing NAXIS1
        assert!(bad(
            vec![simple(), int_card("BITPIX", 16), int_card("NAXIS", 1)],
            0
        )
        .is_err());
        // Negative axis
        assert!(bad(
            vec![
                simple(),
                int_card("BITPIX", 16),
                int_card("NAXIS", 1),
                int_card("NAXIS1", -4)
            ],
            0
        )
        .is_err());
        // Non-numeric NAXIS
        assert!(bad(
            vec![simple(), int_card("BITPIX", 16), card("NAXIS   = 'two'")],
            0
        )
        .is_err());
        // NAXIS > 999
        assert!(bad(
            vec![simple(), int_card("BITPIX", 16), int_card("NAXIS", 1000)],
            0
        )
        .is_err());
        // Size overflow
        let mut c = vec![simple(), int_card("BITPIX", 64), int_card("NAXIS", 3)];
        for n in 1..=3 {
            c.push(int_card(&format!("NAXIS{n}"), i64::MAX));
        }
        assert!(bad(c, 0).is_err());
        // Data declared longer than file
        assert!(bad(
            vec![
                simple(),
                int_card("BITPIX", 16),
                int_card("NAXIS", 1),
                int_card("NAXIS1", 5000)
            ],
            100
        )
        .is_err());
        // SIMPLE = F
        assert!(bad(
            vec![
                card("SIMPLE  =                    F"),
                int_card("BITPIX", 8),
                int_card("NAXIS", 0)
            ],
            0
        )
        .is_err());
        // Missing END card
        let mut no_end = card("SIMPLE  =                    T").into_bytes();
        no_end.resize(BLOCK * 2, b' ');
        assert!(FitsHandler::parse_hdus(&no_end).is_err());
        // Garbage
        assert!(FitsHandler::parse_hdus(&[0xFFu8; 6000]).is_err());
        let mut garbage = b"SIMPLE  ".to_vec();
        garbage.extend((0..6000u32).map(|i| (i * 31 % 251) as u8));
        assert!(FitsHandler::parse_hdus(&garbage).is_err());
    }

    #[tokio::test]
    async fn test_truncated_files_never_panic() {
        let handler = FitsHandler::new();
        let (mut f, img_start, img_len) = multi_hdu();
        let mut c = carrier(f.clone());
        handler
            .embed(&mut c, b"truncate me", &plain_params())
            .await
            .unwrap();
        f = c.raw_data;
        for cut in (0..f.len()).step_by(97) {
            let t = carrier(f[..cut].to_vec());
            let _ = handler.estimate_capacity(&t);
            let _ = handler.verify_integrity(&t);
            let r = handler.extract(&t, &plain_params()).await;
            if cut < img_start + img_len {
                assert!(r.is_err(), "cut at {cut} must not extract");
            }
        }
    }

    // ---- embedding ----

    #[tokio::test]
    async fn test_multi_hdu_only_image_lsbs_change() {
        let handler = FitsHandler::new();
        let (orig, img_start, img_len) = multi_hdu();
        let mut c = carrier(orig.clone());
        let payload = b"Andromeda hides a secret";
        handler
            .embed(&mut c, payload, &plain_params())
            .await
            .unwrap();
        let out = &c.raw_data;

        assert_eq!(out.len(), orig.len());
        // Every byte outside the image data is identical (headers, padding, other HDUs).
        assert_eq!(out[..img_start], orig[..img_start]);
        assert_eq!(out[img_start + img_len..], orig[img_start + img_len..]);
        // Each 16-bit sample differs by at most 1; the MSB never changes.
        let mut changed = 0;
        for (a, b) in orig[img_start..img_start + img_len]
            .chunks(2)
            .zip(out[img_start..img_start + img_len].chunks(2))
        {
            assert_eq!(a[0], b[0]);
            let d = (i16::from_be_bytes([a[0], a[1]]) as i32
                - i16::from_be_bytes([b[0], b[1]]) as i32)
                .abs();
            assert!(d <= 1);
            changed += d;
        }
        assert!(changed > 0);
        assert_headers_end_aligned(out);
        assert!(handler.verify_integrity(&c).unwrap());
        assert_eq!(handler.extract(&c, &plain_params()).await.unwrap(), payload);
    }

    #[tokio::test]
    async fn test_integer_bitpix_roundtrip() {
        let handler = FitsHandler::new();
        for bitpix in [8i64, 16, 32, 64] {
            let (orig, start) = simple_image(bitpix, &[50, 20], int_data(bitpix, 1000));
            let w = (bitpix / 8) as usize;
            let mut c = carrier(orig.clone());
            handler
                .embed(&mut c, b"pixels!", &plain_params())
                .await
                .unwrap();
            for (i, (a, b)) in orig.iter().zip(&c.raw_data).enumerate() {
                if a != b {
                    assert!(
                        i >= start && i < start + 1000 * w,
                        "BITPIX {bitpix}: byte {i}"
                    );
                    assert_eq!((i - start) % w, w - 1, "BITPIX {bitpix}: non-LSB byte {i}");
                    assert_eq!(a ^ b, 1);
                }
            }
            assert_headers_end_aligned(&c.raw_data);
            assert_eq!(
                handler.extract(&c, &plain_params()).await.unwrap(),
                b"pixels!"
            );
        }
    }

    #[tokio::test]
    async fn test_encrypted_roundtrip_and_wrong_password() {
        let handler = FitsHandler::new();
        let (f, _) = simple_image(16, &[64, 64], int_data(16, 4096));
        let mut c = carrier(f);
        let params = EmbedParams::new(b"astro_key".to_vec());
        handler
            .embed(&mut c, b"Encrypted FITS payload", &params)
            .await
            .unwrap();
        assert_eq!(
            handler.extract(&c, &params).await.unwrap(),
            b"Encrypted FITS payload"
        );
        let wrong = EmbedParams::new(b"wrong".to_vec());
        assert!(handler.extract(&c, &wrong).await.is_err());
    }

    #[tokio::test]
    async fn test_exact_capacity_in_bits() {
        let handler = FitsHandler::new();
        let payload = vec![0xA5u8; 20];
        let enc_len = codec::encode_payload(&payload, &plain_params())
            .unwrap()
            .len();
        let samples = (HEADER_LEN + enc_len) * 8;
        let (f, _) = simple_image(16, &[samples], int_data(16, samples));
        let cap = handler.estimate_capacity(&carrier(f.clone())).unwrap();
        assert_eq!(cap.total_bits, samples);
        assert_eq!(cap.recommended_bits, samples - HEADER_BITS);

        let mut c = carrier(f.clone());
        handler
            .embed(&mut c, &payload, &plain_params())
            .await
            .unwrap();
        assert_eq!(handler.extract(&c, &plain_params()).await.unwrap(), payload);

        let mut c = carrier(f);
        let r = handler.embed(&mut c, &[0u8; 21], &plain_params()).await;
        match r {
            Err(SteganoError::InsufficientCapacity { needed, available }) => {
                assert_eq!(available, samples);
                assert_eq!(needed, samples + 8);
            }
            other => panic!("expected InsufficientCapacity, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_blank_samples_skipped() {
        let handler = FitsHandler::new();
        let n = 2000;
        let mut data = int_data(16, n);
        for i in (0..n).step_by(5) {
            let v: i16 = if i % 2 == 0 { -32768 } else { -32767 };
            data[2 * i..2 * i + 2].copy_from_slice(&v.to_be_bytes());
        }
        let mut f = primary_header(16, &[n], &[int_card("BLANK", -32768)]);
        let start = f.len();
        f.extend(padded(data));
        let cap = handler.estimate_capacity(&carrier(f.clone())).unwrap();
        assert_eq!(cap.total_bits, n - n / 5);

        let mut c = carrier(f.clone());
        let payload = vec![0xFFu8; 150];
        handler
            .embed(&mut c, &payload, &plain_params())
            .await
            .unwrap();
        for i in (0..n).step_by(5) {
            assert_eq!(
                c.raw_data[start + 2 * i..start + 2 * i + 2],
                f[start + 2 * i..start + 2 * i + 2]
            );
        }
        assert_eq!(handler.extract(&c, &plain_params()).await.unwrap(), payload);
    }

    #[tokio::test]
    async fn test_float_rejected_by_default() {
        let handler = FitsHandler::new();
        for bitpix in [-32i64, -64] {
            let (f, _) = simple_image(bitpix, &[500], float_data(bitpix, 500));
            let mut c = carrier(f.clone());
            let err = handler
                .embed(&mut c, b"x", &plain_params())
                .await
                .unwrap_err();
            assert!(err.to_string().contains("float_mantissa"), "{err}");
            assert_eq!(c.raw_data, f);
            let cap = handler.estimate_capacity(&c).unwrap();
            assert_eq!(cap.total_bits, 0);
            assert!(
                cap.format_specific_info["float_mantissa_capacity_bits"]
                    .as_u64()
                    .unwrap()
                    > 0
            );
        }
    }

    #[tokio::test]
    async fn test_float_mantissa_roundtrip() {
        let handler = FitsHandler::new();
        for bitpix in [-32i64, -64] {
            let n = 2000;
            let w = (bitpix.abs() / 8) as usize;
            let (orig, start) = simple_image(bitpix, &[n], float_data(bitpix, n));
            let finite = (0..n).filter(|i| i % 7 != 0 && i % 7 != 3).count();
            let mut c = carrier(orig.clone());
            let payload = vec![0x5Au8; 100];
            handler
                .embed(&mut c, &payload, &float_params())
                .await
                .unwrap();
            assert_eq!(
                handler.estimate_capacity(&c).unwrap().format_specific_info
                    ["float_mantissa_capacity_bits"],
                finite
            );
            for i in 0..n {
                let (a, b) = (
                    &orig[start + i * w..start + (i + 1) * w],
                    &c.raw_data[start + i * w..start + (i + 1) * w],
                );
                assert_eq!(
                    a[..w - 1],
                    b[..w - 1],
                    "sign/exponent/high mantissa changed"
                );
                assert!(a[w - 1] ^ b[w - 1] <= 1);
                if i % 7 == 0 || i % 7 == 3 {
                    assert_eq!(a, b, "NaN/Inf modified");
                }
            }
            assert_eq!(orig[..start], c.raw_data[..start]);
            assert_eq!(handler.extract(&c, &plain_params()).await.unwrap(), payload);
        }
    }

    #[tokio::test]
    async fn test_float_mantissa_on_integer_file_errors() {
        let handler = FitsHandler::new();
        let (f, _) = simple_image(16, &[1000], int_data(16, 1000));
        let mut c = carrier(f);
        assert!(handler.embed(&mut c, b"x", &float_params()).await.is_err());
    }

    #[tokio::test]
    async fn test_unknown_strategy_errors() {
        let handler = FitsHandler::new();
        let (f, _) = simple_image(16, &[1000], int_data(16, 1000));
        let mut c = carrier(f);
        let mut p = plain_params();
        p.format_specific
            .insert(STRATEGY_KEY.to_string(), "bogus".into());
        assert!(handler.embed(&mut c, b"x", &p).await.is_err());
    }

    #[tokio::test]
    async fn test_no_image_data_errors() {
        let handler = FitsHandler::new();
        let f = primary_header(8, &[], &[]);
        let mut c = carrier(f);
        assert!(handler.embed(&mut c, b"x", &plain_params()).await.is_err());
        assert_eq!(handler.estimate_capacity(&c).unwrap().total_bits, 0);
        assert!(!handler.verify_integrity(&c).unwrap());
    }

    #[tokio::test]
    async fn test_forged_length_is_error() {
        let handler = FitsHandler::new();
        let (f, start) = simple_image(8, &[2000], int_data(8, 2000));
        let mut c = carrier(f);
        handler
            .embed(&mut c, b"hello", &plain_params())
            .await
            .unwrap();
        // Length field is header bytes 4..8 → samples 32..64 (BITPIX 8: 1 byte each).
        for i in 32..64 {
            c.raw_data[start + i] |= 1;
        }
        let err = handler.extract(&c, &plain_params()).await.unwrap_err();
        assert!(
            err.to_string().contains("exceeds carrier capacity"),
            "{err}"
        );
    }

    #[test]
    fn test_verify_integrity() {
        let handler = FitsHandler::new();
        let (f, _) = simple_image(16, &[1024], int_data(16, 1024));
        let mut c = carrier(f);
        assert!(!handler.verify_integrity(&c).unwrap());
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(handler.embed(&mut c, b"test", &plain_params()))
            .unwrap();
        assert!(handler.verify_integrity(&c).unwrap());
        assert!(!handler
            .verify_integrity(&carrier(b"garbage".to_vec()))
            .unwrap());
    }

    #[tokio::test]
    async fn test_extract_without_payload_errors() {
        let handler = FitsHandler::new();
        let (f, _) = simple_image(16, &[1024], vec![0u8; 2048]);
        assert!(handler.extract(&carrier(f), &plain_params()).await.is_err());
    }
}
