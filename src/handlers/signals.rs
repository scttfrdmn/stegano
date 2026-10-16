//! Signal carrier handler: RIFF/WAVE audio and raw 16-bit sample streams.
//!
//! # Carrier layouts
//!
//! * [`DataFormat::WAV`]: the bytes are parsed as a RIFF/WAVE file. The
//!   `fmt ` and `data` chunks are located by walking the chunk list (odd-sized
//!   chunks are padded as the RIFF spec requires). Supported encodings:
//!   * integer PCM at 8 (unsigned), 16, 24 and 32 bits per sample, either as
//!     `WAVE_FORMAT_PCM` or as `WAVE_FORMAT_EXTENSIBLE`. With the extensible
//!     format, `wValidBitsPerSample` is honoured, so a 24-in-32 container is
//!     modified at bit 8 and the padding bits are left alone;
//!   * IEEE float at 32 or 64 bits (`WAVE_FORMAT_IEEE_FLOAT` or extensible).
//!     Every float sample must be finite. A NaN or infinity is rejected
//!     rather than risk turning an infinity into a NaN;
//!   * any number of channels.
//!
//!   Only bytes inside the `data` chunk payload are ever modified. The RIFF
//!   header, the `fmt ` chunk, every other chunk (`LIST`, `fact`, `cue `, …),
//!   chunk order, and any trailing bytes are preserved byte for byte. The
//!   sample count never changes. A trailing partial frame in the `data`
//!   chunk is left untouched. A `data` chunk whose declared size runs past the
//!   end of the file (common for streamed recordings) is clamped to the bytes
//!   that are actually present.
//! * [`DataFormat::BinarySignal`] and [`DataFormat::CsvTimeseries`] keep the
//!   historical raw semantics: the bytes are treated as mono 16-bit
//!   little-endian samples with no header. If the length is odd, the final
//!   byte is not part of any sample and is preserved unchanged. The sample
//!   rate is `carrier.sampling_rate`, or 44 100 Hz if that is absent. The
//!   rate only affects the echo-hiding delays.
//!
//! # Wire format
//!
//! The embedded bit stream is a 10-byte header, `b"SGSJ"`, followed by the
//! payload length (u32 LE), the strategy tag, and the format version (`2`).
//! The header is followed by the output of
//! [`codec::encode_payload`]. Bits are written LSB first. There is no legacy
//! or raw mode: every payload goes through the codec.
//!
//! # Strategies and honest robustness
//!
//! None of the strategies is claimed to survive lossy compression (MP3, AAC,
//! Opus), resampling, gain changes, filtering or D/A–A/D conversion. All of
//! them are designed for, and tested against, bit-exact lossless storage
//! only. Embedding always ends by extracting from the *quantised* output and
//! comparing the result with what was meant to be written. If the carrier
//! cannot hold the bits reliably (it clips, is silent, or is too short), the
//! embed fails with an error instead of producing output that cannot be
//! extracted.
//!
//! | Strategy | Capacity (bits) | Measured SNR, 16-bit test signal |
//! |---|---|---|
//! | TimeDomainLSB | 1 per sample (all channels) | ≈ 88 dB (≤ 1 LSB per sample) |
//! | FrequencyDomain | 256 per 1024-frame chunk per channel | ≈ 65 dB, upper quarter band only |
//! | PhaseModulation | 128 per 1024-frame chunk per channel | ≈ 50 dB, band fs/8 to fs/4 |
//! | SpreadSpectrum | 1 per 512-frame chip per channel | ≈ 29 dB (white noise) |
//! | EchoHiding | 1 per 1024-frame segment per channel | ≈ 15 dB (audible echo, gain 0.1–0.8) |
//!
//! The SNR figures were measured with an encrypted payload of about 100 bytes
//! on a multi-tone plus noise signal at 44.1 kHz. Spread-spectrum and echo
//! hiding fill almost the whole carrier at that size. All strategies work on
//! noisy, broadband audio. EchoHiding usually *fails at embed time* on pure
//! tones (about 20 of 24 single-sine carriers in testing). It also fails on
//! silence.
//!
//! The SpreadSpectrum and EchoHiding strategies take their keyed
//! pseudo-noise (PN) sequence from a seed derived with BLAKE3 from
//! `params.encryption_key`. With an empty key, the seed is derived from the
//! empty string, which is fixed and public, so it provides no secrecy.
//! See [`SignalEmbedStrategy`] for details of each strategy.

use super::DataHandler;
use crate::codec;
use crate::core::*;
use async_trait::async_trait;
use rustfft::{num_complex::Complex, Fft, FftPlanner};
use std::sync::Arc;
use std::time::Instant;

/// Magic bytes that open every embedded header.
const MAGIC: &[u8; 4] = b"SGSJ";
/// Header length in bytes: magic(4) + length(4) + strategy(1) + version(1).
const HEADER_LEN: usize = 10;
const HEADER_BITS: usize = HEADER_LEN * 8;
/// Wire-format version stored in header byte 9 (2 = codec pipeline).
const FORMAT_VERSION: u8 = 2;

/// Default sample rate for raw (headerless) carriers without `sampling_rate`.
const DEFAULT_RAW_RATE: u32 = 44_100;

/// FFT size for the FrequencyDomain and PhaseModulation strategies.
const FFT_CHUNK: usize = 1024;
/// Positive-frequency bins used by FrequencyDomain (`[256, 512)`).
const MAG_BINS: (usize, usize) = (FFT_CHUNK / 4, FFT_CHUNK / 2);
/// Positive-frequency bins used by PhaseModulation (`[128, 256)`).
const PHASE_BINS: (usize, usize) = (FFT_CHUNK / 8, FFT_CHUNK / 4);
/// Magnitude quantisation step, in sample units. Re-quantising 1024 samples
/// to integers adds per-bin noise with an RMS of about 9 units, which a
/// half-step of 256 swamps. The step is also large enough that a single-bin
/// change is not simply rounded away. If it is, the embedder refines the
/// chunk and then self-verifies.
const MAG_STEP: f64 = 512.0;
/// Phase quantisation step, in radians.
const PHASE_STEP: f64 = 0.1;
/// Highest phase unit used, so that `PHASE_MAX_UNIT * PHASE_STEP` < 2π − margin.
const PHASE_MAX_UNIT: i64 = 61;
/// Minimum bin magnitude forced before phase coding. This keeps the phase
/// error caused by re-quantisation well below `PHASE_STEP / 2`.
const PHASE_NORM_MIN: f64 = 4096.0;

/// Spread-spectrum chip length, in frames per bit.
const SS_CHIP: usize = 512;
/// Spread-spectrum target correlation, as a fraction of the chip's RMS.
const SS_REL_AMPLITUDE: f64 = 0.01;
/// Lower bound on the target correlation, in sample units.
const SS_MIN_AMPLITUDE: f64 = 1.0;

/// Echo-hiding segment length, in frames per bit.
const ECHO_SEGMENT: usize = 1024;
/// Echo gains tried in order until the segment decodes with margin.
const ECHO_ALPHAS: [f64; 7] = [0.1, 0.2, 0.3, 0.4, 0.5, 0.65, 0.8];
/// Minimum |c[d1] − c[d0]| cepstral difference that is accepted at embed time.
const ECHO_MARGIN: f64 = 0.02;

/// One sample unit for float carriers: a float sample `v` is handled as
/// `v * FLOAT_UNIT_SCALE`, so the constants above behave as they do for 16-bit.
const FLOAT_UNIT_SCALE: f64 = 32768.0;

/// Handler for digital signal carriers (WAV audio and raw binary signals).
///
/// The strategy comes from [`EmbedParams::stealth_level`]. It can be
/// overridden with `format_specific["signal_strategy"]`, which accepts
/// `timedomain`, `frequency`, `phase`, `spread` or `echo`.
pub struct SignalHandler;

/// Signal embedding strategy.
///
/// # Robustness
///
/// The robustness claims are limited to bit-exact lossless storage. See the
/// [module documentation](self).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalEmbedStrategy {
    /// Replaces the least significant bit of each sample, interleaved
    /// across channels. For float samples, this is the mantissa LSB (1 ULP).
    /// It carries 1 bit per sample, changes each sample by at most 1 LSB,
    /// and does not survive any modification of the samples.
    TimeDomainLSB,
    /// Uses parity quantisation of the FFT magnitudes in bins
    /// `[256, 512)` of each 1024-frame chunk, per channel. The quantisation
    /// step is 128 sample units.
    FrequencyDomain,
    /// Uses parity quantisation of the FFT phases (0.1 rad steps) in bins
    /// `[128, 256)` of each 1024-frame chunk, per channel.
    PhaseModulation,
    /// Uses informed (host-interference-cancelling) spread spectrum. Each
    /// bit is spread over a 512-frame chip of keyed ±1 pseudo-noise `p`.
    /// The embedder measures the host correlation `ρ = <x,p>/L` and adds
    /// only enough `k·p` to make `b·<y,p>/L ≥ a`, where
    /// `a = max(1 % of the chip RMS, 1 unit)`.
    ///
    /// Chips whose host correlation already has the right sign and enough
    /// margin are not touched at all. Otherwise the added noise has an
    /// amplitude of roughly `|ρ|` (about RMS/√512, or −27 dB below the
    /// signal), plus `a`.
    ///
    /// Detection only needs the key. Margins are small, so expect bit errors
    /// after anything other than lossless storage.
    SpreadSpectrum,
    /// Uses keyed echo hiding. Each 1024-frame segment gets an echo of
    /// sign `s` (taken from the keyed PN) at delay `d0` (bit 0) or `d1` (bit 1).
    /// The delays are about 1 ms and 1.5 ms, derived from the real sample rate
    /// and clamped to 8–160 frames.
    ///
    /// Detection is blind: it compares `s·(c[d1] − c[d0])` over the
    /// Hann-windowed real cepstrum. The embedder tries echo gains from 0.1 to
    /// 0.8 and keeps the smallest one that decodes with margin after
    /// quantisation. If no gain works (for example on silent or clipped
    /// segments), embedding fails with an error. Echoes at these gains are
    /// audible as colouration.
    EchoHiding,
}

impl SignalEmbedStrategy {
    const ALL: [SignalEmbedStrategy; 5] = [
        SignalEmbedStrategy::TimeDomainLSB,
        SignalEmbedStrategy::FrequencyDomain,
        SignalEmbedStrategy::PhaseModulation,
        SignalEmbedStrategy::SpreadSpectrum,
        SignalEmbedStrategy::EchoHiding,
    ];

    fn to_u8(self) -> u8 {
        match self {
            SignalEmbedStrategy::TimeDomainLSB => 0,
            SignalEmbedStrategy::FrequencyDomain => 1,
            SignalEmbedStrategy::PhaseModulation => 2,
            SignalEmbedStrategy::SpreadSpectrum => 3,
            SignalEmbedStrategy::EchoHiding => 4,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn from_u8(v: u8) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.to_u8() == v)
    }
}

// ---------------------------------------------------------------------------
// Carrier layout
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SampleKind {
    /// 8-bit unsigned PCM (offset 128).
    U8,
    /// Signed little-endian PCM in a `bytes`-byte container. The value sits in
    /// the top `valid_bits`, and the low `shift` bits are padding.
    Int {
        bytes: usize,
        shift: u32,
        valid_bits: u32,
    },
    F32,
    F64,
}

impl SampleKind {
    fn bytes(self) -> usize {
        match self {
            SampleKind::U8 => 1,
            SampleKind::Int { bytes, .. } => bytes,
            SampleKind::F32 => 4,
            SampleKind::F64 => 8,
        }
    }
}

/// Where the samples live inside the carrier bytes and how to read them.
#[derive(Debug, Clone)]
struct SignalLayout {
    data_offset: usize,
    /// Length in bytes of the sample region. Always a multiple of the frame size.
    data_len: usize,
    channels: usize,
    sample_rate: u32,
    kind: SampleKind,
}

fn le_u16(d: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([d[at], d[at + 1]])
}

fn le_u32(d: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn invalid(msg: impl Into<String>) -> SteganoError {
    SteganoError::InvalidParameter(msg.into())
}

impl SignalLayout {
    /// Build the layout for a carrier according to its declared format.
    fn from_carrier(carrier: &CarrierData) -> Result<Self> {
        match carrier.format {
            DataFormat::WAV => Self::parse_wav(&carrier.raw_data),
            _ => {
                let rate = carrier
                    .sampling_rate
                    .filter(|r| r.is_finite() && *r >= 1.0 && *r <= u32::MAX as f64)
                    .map(|r| r as u32)
                    .unwrap_or(DEFAULT_RAW_RATE);
                Ok(Self {
                    data_offset: 0,
                    data_len: carrier.raw_data.len() & !1,
                    channels: 1,
                    sample_rate: rate,
                    kind: SampleKind::Int {
                        bytes: 2,
                        shift: 0,
                        valid_bits: 16,
                    },
                })
            }
        }
    }

    /// Parse a RIFF/WAVE file and locate the `fmt ` and `data` chunks.
    fn parse_wav(d: &[u8]) -> Result<Self> {
        if d.len() < 12 || &d[0..4] != b"RIFF" || &d[8..12] != b"WAVE" {
            return Err(invalid("WAV carrier is not a RIFF/WAVE file"));
        }

        let mut fmt: Option<(usize, usize)> = None;
        let mut data: Option<(usize, usize)> = None;
        let mut pos = 12usize;
        while pos.checked_add(8).is_some_and(|e| e <= d.len()) {
            let id = &d[pos..pos + 4];
            let size = le_u32(d, pos + 4) as usize;
            let body = pos + 8;
            let declared_end = body.checked_add(size);
            if id == b"data" {
                // Clamp the data chunk to what is actually present (streamed WAVs).
                let end = declared_end.map_or(d.len(), |e| e.min(d.len()));
                data = Some((body, end - body));
                if fmt.is_some() {
                    break;
                }
            } else {
                if declared_end.is_none_or(|e| e > d.len()) {
                    return Err(invalid("WAV chunk extends past end of file"));
                }
                if id == b"fmt " && fmt.is_none() {
                    fmt = Some((body, size));
                }
                if fmt.is_some() && data.is_some() {
                    break;
                }
            }
            // Advance past the body plus the RIFF pad byte for odd sizes.
            match declared_end.and_then(|e| e.checked_add(size & 1)) {
                Some(next) => pos = next,
                None => break,
            }
        }

        let (fmt_at, fmt_len) = fmt.ok_or_else(|| invalid("WAV file has no fmt chunk"))?;
        let (data_at, data_len) = data.ok_or_else(|| invalid("WAV file has no data chunk"))?;
        if fmt_len < 16 {
            return Err(invalid("WAV fmt chunk is too short"));
        }

        let mut format_tag = le_u16(d, fmt_at);
        let channels = le_u16(d, fmt_at + 2) as usize;
        let sample_rate = le_u32(d, fmt_at + 4);
        let block_align = le_u16(d, fmt_at + 12) as usize;
        let bits = le_u16(d, fmt_at + 14) as u32;
        let mut valid_bits = bits;

        if format_tag == 0xFFFE {
            if fmt_len < 40 {
                return Err(invalid("WAVE_FORMAT_EXTENSIBLE fmt chunk is too short"));
            }
            let vb = le_u16(d, fmt_at + 18) as u32;
            if vb != 0 {
                valid_bits = vb;
            }
            // The first two bytes of the SubFormat GUID hold the format code.
            format_tag = le_u16(d, fmt_at + 24);
        }

        if channels == 0 {
            return Err(invalid("WAV file declares zero channels"));
        }
        if sample_rate == 0 {
            return Err(invalid("WAV file declares a sample rate of 0 Hz"));
        }
        if !bits.is_multiple_of(8) || bits == 0 {
            return Err(invalid(format!("unsupported WAV bit depth {bits}")));
        }
        let bytes = (bits / 8) as usize;
        if channels.checked_mul(bytes) != Some(block_align) {
            return Err(invalid(format!(
                "WAV block_align {block_align} does not match {channels} channels × {bits} bits"
            )));
        }

        let kind = match (format_tag, bits) {
            (1, 8) => SampleKind::U8,
            (1, 16 | 24 | 32) => {
                if valid_bits < 2 || valid_bits > bits {
                    return Err(invalid(format!(
                        "invalid wValidBitsPerSample {valid_bits} for {bits}-bit container"
                    )));
                }
                SampleKind::Int {
                    bytes,
                    shift: bits - valid_bits,
                    valid_bits,
                }
            }
            (3, 32) => SampleKind::F32,
            (3, 64) => SampleKind::F64,
            (1, _) => return Err(invalid(format!("unsupported PCM bit depth {bits}"))),
            (3, _) => return Err(invalid(format!("unsupported float bit depth {bits}"))),
            (tag, _) => {
                return Err(invalid(format!(
                    "unsupported WAV format tag 0x{tag:04X} (only PCM and IEEE float)"
                )))
            }
        };

        let layout = Self {
            data_offset: data_at,
            data_len: data_len - data_len % block_align,
            channels,
            sample_rate,
            kind,
        };
        layout.check_finite(d)?;
        Ok(layout)
    }

    fn check_finite(&self, d: &[u8]) -> Result<()> {
        if matches!(self.kind, SampleKind::F32 | SampleKind::F64) {
            for i in 0..self.num_samples() {
                if !self.read_units(d, i).is_finite() {
                    return Err(invalid(
                        "float WAV contains NaN/Inf samples; refusing to embed/extract",
                    ));
                }
            }
        }
        Ok(())
    }

    fn num_samples(&self) -> usize {
        self.data_len / self.kind.bytes()
    }

    fn frames(&self) -> usize {
        self.num_samples() / self.channels
    }

    fn at(&self, i: usize) -> usize {
        self.data_offset + i * self.kind.bytes()
    }

    fn int_range(&self) -> (i64, i64) {
        match self.kind {
            SampleKind::U8 => (-128, 127),
            SampleKind::Int { valid_bits, .. } => {
                let half = 1i64 << (valid_bits - 1);
                (-half, half - 1)
            }
            _ => (i64::MIN, i64::MAX),
        }
    }

    /// Read the integer sample code (with the padding shift removed).
    fn read_code(&self, d: &[u8], i: usize) -> i64 {
        let a = self.at(i);
        match self.kind {
            SampleKind::U8 => d[a] as i64 - 128,
            SampleKind::Int { bytes, shift, .. } => {
                let mut raw: i64 = 0;
                for b in 0..bytes {
                    raw |= (d[a + b] as i64) << (8 * b);
                }
                let width = 8 * bytes as u32;
                // Sign-extend from the container width.
                raw = (raw << (64 - width)) >> (64 - width);
                raw >> shift
            }
            _ => unreachable!("read_code on float layout"),
        }
    }

    fn write_code(&self, d: &mut [u8], i: usize, code: i64) {
        let a = self.at(i);
        match self.kind {
            SampleKind::U8 => d[a] = (code + 128) as u8,
            SampleKind::Int { bytes, shift, .. } => {
                let mut old: i64 = 0;
                for b in 0..bytes {
                    old |= (d[a + b] as i64) << (8 * b);
                }
                let pad_mask = (1i64 << shift) - 1;
                let raw = (code << shift) | (old & pad_mask);
                for b in 0..bytes {
                    d[a + b] = (raw >> (8 * b)) as u8;
                }
            }
            _ => unreachable!("write_code on float layout"),
        }
    }

    /// Read a sample in "units": integer LSBs for PCM, `v * 32768` for float.
    fn read_units(&self, d: &[u8], i: usize) -> f64 {
        let a = self.at(i);
        match self.kind {
            SampleKind::F32 => {
                f32::from_le_bytes([d[a], d[a + 1], d[a + 2], d[a + 3]]) as f64 * FLOAT_UNIT_SCALE
            }
            SampleKind::F64 => {
                let mut b = [0u8; 8];
                b.copy_from_slice(&d[a..a + 8]);
                f64::from_le_bytes(b) * FLOAT_UNIT_SCALE
            }
            _ => self.read_code(d, i) as f64,
        }
    }

    /// The value `u` will have after it is written and read back.
    fn quantize(&self, u: f64) -> f64 {
        match self.kind {
            SampleKind::F32 => ((u / FLOAT_UNIT_SCALE) as f32) as f64 * FLOAT_UNIT_SCALE,
            SampleKind::F64 => u,
            _ => {
                let (lo, hi) = self.int_range();
                u.round().clamp(lo as f64, hi as f64)
            }
        }
    }

    fn write_units(&self, d: &mut [u8], i: usize, u: f64) {
        let a = self.at(i);
        match self.kind {
            SampleKind::F32 => {
                d[a..a + 4].copy_from_slice(&((u / FLOAT_UNIT_SCALE) as f32).to_le_bytes())
            }
            SampleKind::F64 => d[a..a + 8].copy_from_slice(&(u / FLOAT_UNIT_SCALE).to_le_bytes()),
            _ => {
                let q = self.quantize(u) as i64;
                self.write_code(d, i, q);
            }
        }
    }

    fn read_lsb(&self, d: &[u8], i: usize) -> u8 {
        let a = self.at(i);
        match self.kind {
            SampleKind::F32 | SampleKind::F64 => d[a] & 1,
            _ => (self.read_code(d, i) & 1) as u8,
        }
    }

    fn write_lsb(&self, d: &mut [u8], i: usize, bit: u8) {
        let a = self.at(i);
        match self.kind {
            // Little-endian: the mantissa LSB is bit 0 of the first byte.
            // The exponent is untouched, so a finite value stays finite.
            SampleKind::F32 | SampleKind::F64 => d[a] = (d[a] & !1) | (bit & 1),
            _ => {
                let code = (self.read_code(d, i) & !1) | (bit & 1) as i64;
                self.write_code(d, i, code);
            }
        }
    }

    /// Read `count` frames of channel `ch` starting at frame `start`.
    fn read_lane(&self, d: &[u8], ch: usize, start: usize, count: usize) -> Vec<f64> {
        (start..start + count)
            .map(|f| self.read_units(d, f * self.channels + ch))
            .collect()
    }

    /// Write frames back to channel `ch`, touching only samples whose value changed.
    fn write_lane(&self, d: &mut [u8], ch: usize, start: usize, old: &[f64], new: &[f64]) {
        for (j, (&o, &n)) in old.iter().zip(new).enumerate() {
            let q = self.quantize(n);
            if q != o {
                self.write_units(d, (start + j) * self.channels + ch, q);
            }
        }
    }

    /// Echo delays `(d0, d1)` in frames, derived from the real sample rate.
    fn echo_delays(&self) -> (usize, usize) {
        let d0 = (self.sample_rate as usize / 1000).clamp(8, 160);
        let d1 = (d0 + d0 / 2).max(d0 + 1);
        (d0, d1)
    }

    /// Capacity in bits for `strategy`, header included.
    fn capacity_bits(&self, strategy: SignalEmbedStrategy) -> usize {
        let per_lane = |unit: usize, bits: usize| (self.frames() / unit) * bits * self.channels;
        match strategy {
            SignalEmbedStrategy::TimeDomainLSB => self.num_samples(),
            SignalEmbedStrategy::FrequencyDomain => per_lane(FFT_CHUNK, MAG_BINS.1 - MAG_BINS.0),
            SignalEmbedStrategy::PhaseModulation => {
                per_lane(FFT_CHUNK, PHASE_BINS.1 - PHASE_BINS.0)
            }
            SignalEmbedStrategy::SpreadSpectrum => per_lane(SS_CHIP, 1),
            SignalEmbedStrategy::EchoHiding => per_lane(ECHO_SEGMENT, 1),
        }
    }
}

// ---------------------------------------------------------------------------
// Keyed PN
// ---------------------------------------------------------------------------

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Derive the 64-bit PN seed from the key. An empty key gives a fixed public seed.
fn pn_seed(key: &[u8]) -> u64 {
    let mut h = blake3::Hasher::new_derive_key("stegano signals PN seed v1");
    h.update(key);
    let out = h.finalize();
    let mut b = [0u8; 8];
    b.copy_from_slice(&out.as_bytes()[..8]);
    u64::from_le_bytes(b)
}

/// Keyed ±1 chips for spread-spectrum unit `unit`.
fn pn_chips(seed: u64, unit: usize, len: usize) -> Vec<f64> {
    let mut st = seed ^ (unit as u64).wrapping_mul(0xD6E8_FEB8_6659_FD93);
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        let r = splitmix64(&mut st);
        for b in 0..64 {
            if out.len() == len {
                break;
            }
            out.push(if (r >> b) & 1 == 1 { 1.0 } else { -1.0 });
        }
    }
    out
}

/// Keyed echo sign (±1) for echo segment `unit`.
fn echo_sign(seed: u64, unit: usize) -> f64 {
    let mut st = (seed ^ 0xA5A5_5A5A_C3C3_3C3C) ^ (unit as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    if splitmix64(&mut st) & 1 == 1 {
        1.0
    } else {
        -1.0
    }
}

// ---------------------------------------------------------------------------
// FFT helpers
// ---------------------------------------------------------------------------

struct FftPair {
    fwd: Arc<dyn Fft<f64>>,
    inv: Arc<dyn Fft<f64>>,
}

impl FftPair {
    fn new(n: usize) -> Self {
        let mut p = FftPlanner::new();
        Self {
            fwd: p.plan_fft_forward(n),
            inv: p.plan_fft_inverse(n),
        }
    }

    fn forward(&self, x: &[f64]) -> Vec<Complex<f64>> {
        let mut b: Vec<Complex<f64>> = x.iter().map(|&v| Complex::new(v, 0.0)).collect();
        self.fwd.process(&mut b);
        b
    }

    fn inverse_real(&self, mut b: Vec<Complex<f64>>) -> Vec<f64> {
        self.inv.process(&mut b);
        let n = b.len() as f64;
        b.iter().map(|c| c.re / n).collect()
    }
}

fn mag_bit(c: Complex<f64>) -> u8 {
    let units = (c.norm().max(MAG_STEP) / MAG_STEP).round() as u64;
    (units % 2) as u8
}

fn phase_offset(c: Complex<f64>) -> f64 {
    (c.arg() + std::f64::consts::PI).rem_euclid(2.0 * std::f64::consts::PI)
}

fn phase_bit(c: Complex<f64>) -> u8 {
    let units = (phase_offset(c) / PHASE_STEP).round() as i64;
    (units.rem_euclid(2)) as u8
}

/// Hann-windowed real cepstrum difference `c[d1] − c[d0]`.
fn cepstral_diff(fft: &FftPair, seg: &[f64], d0: usize, d1: usize) -> f64 {
    let n = seg.len();
    let windowed: Vec<f64> = seg
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            let w = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos();
            v * w
        })
        .collect();
    let spec = fft.forward(&windowed);
    let peak = spec.iter().map(|c| c.norm()).fold(0.0f64, f64::max);
    if peak <= 0.0 {
        return 0.0;
    }
    let eps = peak * 1e-6;
    let logmag: Vec<Complex<f64>> = spec
        .iter()
        .map(|c| Complex::new((c.norm() + eps).ln(), 0.0))
        .collect();
    let ceps = fft.inverse_real(logmag);
    ceps[d1] - ceps[d0]
}

// ---------------------------------------------------------------------------
// Bit extraction (shared by extract and embed self-verification)
// ---------------------------------------------------------------------------

/// Extract `nbits` bits with `strategy`. The caller guarantees that
/// `nbits <= layout.capacity_bits(strategy)`.
fn extract_bits(
    layout: &SignalLayout,
    d: &[u8],
    strategy: SignalEmbedStrategy,
    nbits: usize,
    seed: u64,
) -> Vec<u8> {
    let nbits = nbits.min(layout.capacity_bits(strategy));
    let mut bits = Vec::with_capacity(nbits);
    if strategy == SignalEmbedStrategy::TimeDomainLSB {
        bits.extend((0..nbits).map(|i| layout.read_lsb(d, i)));
        return bits;
    }

    let (unit_len, fft) = match strategy {
        SignalEmbedStrategy::SpreadSpectrum => (SS_CHIP, None),
        SignalEmbedStrategy::EchoHiding => (ECHO_SEGMENT, Some(FftPair::new(ECHO_SEGMENT))),
        _ => (FFT_CHUNK, Some(FftPair::new(FFT_CHUNK))),
    };
    let (d0, d1) = layout.echo_delays();
    let units_per_lane = layout.frames() / unit_len;
    let mut unit = 0usize;

    'outer: for ch in 0..layout.channels {
        for u in 0..units_per_lane {
            if bits.len() >= nbits {
                break 'outer;
            }
            let seg = layout.read_lane(d, ch, u * unit_len, unit_len);
            match strategy {
                SignalEmbedStrategy::FrequencyDomain | SignalEmbedStrategy::PhaseModulation => {
                    let spec = fft.as_ref().expect("fft").forward(&seg);
                    let (lo, hi) = if strategy == SignalEmbedStrategy::FrequencyDomain {
                        MAG_BINS
                    } else {
                        PHASE_BINS
                    };
                    for &c in &spec[lo..hi] {
                        if bits.len() >= nbits {
                            break;
                        }
                        bits.push(if strategy == SignalEmbedStrategy::FrequencyDomain {
                            mag_bit(c)
                        } else {
                            phase_bit(c)
                        });
                    }
                }
                SignalEmbedStrategy::SpreadSpectrum => {
                    let pn = pn_chips(seed, unit, unit_len);
                    let c: f64 = seg.iter().zip(&pn).map(|(x, p)| x * p).sum();
                    bits.push((c > 0.0) as u8);
                }
                SignalEmbedStrategy::EchoHiding => {
                    let diff = cepstral_diff(fft.as_ref().expect("fft"), &seg, d0, d1);
                    bits.push((echo_sign(seed, unit) * diff > 0.0) as u8);
                }
                SignalEmbedStrategy::TimeDomainLSB => unreachable!(),
            }
            unit += 1;
        }
    }
    bits
}

// ---------------------------------------------------------------------------
// Embedding
// ---------------------------------------------------------------------------

fn embed_lsb(layout: &SignalLayout, out: &mut [u8], bits: &[u8]) {
    for (i, &b) in bits.iter().enumerate() {
        layout.write_lsb(out, i, b);
    }
}

fn embed_fft(
    layout: &SignalLayout,
    out: &mut [u8],
    bits: &[u8],
    strategy: SignalEmbedStrategy,
) -> Result<()> {
    let fft = FftPair::new(FFT_CHUNK);
    let units_per_lane = layout.frames() / FFT_CHUNK;
    let mut idx = 0usize;
    let phase_mode = strategy == SignalEmbedStrategy::PhaseModulation;
    let (lo, hi) = if phase_mode { PHASE_BINS } else { MAG_BINS };

    'outer: for ch in 0..layout.channels {
        for u in 0..units_per_lane {
            if idx >= bits.len() {
                break 'outer;
            }
            let start = u * FFT_CHUNK;
            let seg = layout.read_lane(out, ch, start, FFT_CHUNK);
            let mut spec = fft.forward(&seg);
            let n_here = (hi - lo).min(bits.len() - idx);
            let chunk_bits = &bits[idx..idx + n_here];
            let mut targets = Vec::with_capacity(n_here);
            for (j, &bit) in chunk_bits.iter().enumerate() {
                let k = lo + j;
                let t = fft_target(spec[k], bit, phase_mode);
                spec[k] = t;
                spec[FFT_CHUNK - k] = t.conj();
                targets.push(t);
            }
            let quantized =
                |v: Vec<f64>| -> Vec<f64> { v.into_iter().map(|x| layout.quantize(x)).collect() };
            let mut q = quantized(fft.inverse_real(spec));
            // Re-quantising to integers can partly cancel small per-bin changes
            // (the rounding error is correlated with a sub-LSB modification).
            // Refine: push each still-failing bin past its target by the residual.
            for _ in 0..8 {
                let mut sq = fft.forward(&q);
                let mut wrong = false;
                for (j, &bit) in chunk_bits.iter().enumerate() {
                    let k = lo + j;
                    if fft_bit(sq[k], phase_mode) != bit {
                        wrong = true;
                        let t = targets[j] + (targets[j] - sq[k]);
                        sq[k] = t;
                        sq[FFT_CHUNK - k] = t.conj();
                    }
                }
                if !wrong {
                    break;
                }
                q = quantized(fft.inverse_real(sq));
            }
            layout.write_lane(out, ch, start, &seg, &q);
            idx += n_here;
        }
    }
    Ok(())
}

/// Target coefficient for FFT bin `c` so that it carries `bit`.
fn fft_target(c: Complex<f64>, bit: u8, phase_mode: bool) -> Complex<f64> {
    let bit = bit as i64;
    if phase_mode {
        // Choose the allowed unit (1..=61) with the right parity that is
        // circularly nearest to the current phase.
        let off = phase_offset(c);
        let two_pi = 2.0 * std::f64::consts::PI;
        let dist = |u: i64| {
            let d = (u as f64 * PHASE_STEP - off).abs();
            d.min(two_pi - d)
        };
        let best = (1..=PHASE_MAX_UNIT)
            .filter(|u| u % 2 == bit)
            .min_by(|&a, &b| dist(a).total_cmp(&dist(b)))
            .expect("non-empty unit set");
        let phase = best as f64 * PHASE_STEP - std::f64::consts::PI;
        Complex::from_polar(c.norm().max(PHASE_NORM_MIN), phase)
    } else {
        let units = (c.norm().max(MAG_STEP) / MAG_STEP).round() as i64;
        let new_units = if units % 2 == bit { units } else { units + 1 };
        Complex::from_polar(new_units as f64 * MAG_STEP, c.arg())
    }
}

fn fft_bit(c: Complex<f64>, phase_mode: bool) -> u8 {
    if phase_mode {
        phase_bit(c)
    } else {
        mag_bit(c)
    }
}

fn embed_spread(layout: &SignalLayout, out: &mut [u8], bits: &[u8], seed: u64) -> Result<()> {
    let units_per_lane = layout.frames() / SS_CHIP;
    let mut unit = 0usize;
    'outer: for ch in 0..layout.channels {
        for u in 0..units_per_lane {
            if unit >= bits.len() {
                break 'outer;
            }
            let start = u * SS_CHIP;
            let x = layout.read_lane(out, ch, start, SS_CHIP);
            let pn = pn_chips(seed, unit, SS_CHIP);
            let l = SS_CHIP as f64;
            let rho: f64 = x.iter().zip(&pn).map(|(a, p)| a * p).sum::<f64>() / l;
            let rms = (x.iter().map(|v| v * v).sum::<f64>() / l).sqrt();
            let b = if bits[unit] == 1 { 1.0 } else { -1.0 };
            let mut a = (SS_REL_AMPLITUDE * rms).max(SS_MIN_AMPLITUDE);
            let mut done = false;
            for _ in 0..6 {
                let k = if b * rho >= a { 0.0 } else { b * a - rho };
                let y: Vec<f64> = x
                    .iter()
                    .zip(&pn)
                    .map(|(v, p)| layout.quantize(v + k * p))
                    .collect();
                let c: f64 = y.iter().zip(&pn).map(|(v, p)| v * p).sum::<f64>() / l;
                if b * c >= a * 0.5 && b * c > 0.0 {
                    layout.write_lane(out, ch, start, &x, &y);
                    done = true;
                    break;
                }
                a *= 2.0;
            }
            if !done {
                return Err(invalid(format!(
                    "SpreadSpectrum: carrier cannot hold bit {unit} reliably \
                     (clipping or insufficient headroom in channel {ch})"
                )));
            }
            unit += 1;
        }
    }
    Ok(())
}

fn embed_echo(layout: &SignalLayout, out: &mut [u8], bits: &[u8], seed: u64) -> Result<()> {
    let fft = FftPair::new(ECHO_SEGMENT);
    let (d0, d1) = layout.echo_delays();
    let units_per_lane = layout.frames() / ECHO_SEGMENT;
    // Read the original lanes once: the echo source is always the *original* host.
    let mut unit = 0usize;
    'outer: for ch in 0..layout.channels {
        if unit >= bits.len() {
            break;
        }
        let needed = (bits.len() - unit).min(units_per_lane) * ECHO_SEGMENT;
        let host = layout.read_lane(out, ch, 0, needed);
        for u in 0..units_per_lane {
            if unit >= bits.len() {
                break 'outer;
            }
            let start = u * ECHO_SEGMENT;
            let x = &host[start..start + ECHO_SEGMENT];
            let s = echo_sign(seed, unit);
            let bit = bits[unit];
            let d = if bit == 1 { d1 } else { d0 };
            let mut accepted = None;
            for &alpha in &ECHO_ALPHAS {
                let y: Vec<f64> = (0..ECHO_SEGMENT)
                    .map(|i| {
                        let n = start + i;
                        let echo = if n >= d { host[n - d] } else { 0.0 };
                        layout.quantize(x[i] + s * alpha * echo)
                    })
                    .collect();
                let m = s * cepstral_diff(&fft, &y, d0, d1);
                let ok = if bit == 1 {
                    m >= ECHO_MARGIN
                } else {
                    m <= -ECHO_MARGIN
                };
                if ok {
                    accepted = Some(y);
                    break;
                }
            }
            let y = accepted.ok_or_else(|| {
                invalid(format!(
                    "EchoHiding: segment {unit} (channel {ch}) cannot carry a bit reliably \
                     (too quiet, clipped, or strongly tonal); use another strategy"
                ))
            })?;
            layout.write_lane(out, ch, start, x, &y);
            unit += 1;
        }
    }
    Ok(())
}

fn bits_of(bytes: &[u8]) -> Vec<u8> {
    bytes
        .iter()
        .flat_map(|&b| (0..8).map(move |i| (b >> i) & 1))
        .collect()
}

fn bytes_of(bits: &[u8]) -> Vec<u8> {
    bits.chunks(8)
        .map(|c| {
            c.iter()
                .enumerate()
                .fold(0u8, |acc, (i, &b)| acc | (b << i))
        })
        .collect()
}

impl SignalHandler {
    /// Create a new signal handler. Encryption is configured per call through
    /// [`EmbedParams::encryption_key`].
    pub fn new() -> Self {
        Self
    }

    fn select_strategy(params: &EmbedParams) -> SignalEmbedStrategy {
        let over = params
            .format_specific
            .get("signal_strategy")
            .and_then(|v| v.as_str())
            .and_then(|s| match s.to_lowercase().as_str() {
                "timedomain" | "timedomainlsb" | "lsb" => Some(SignalEmbedStrategy::TimeDomainLSB),
                "frequency" | "frequencydomain" => Some(SignalEmbedStrategy::FrequencyDomain),
                "phase" | "phasemodulation" => Some(SignalEmbedStrategy::PhaseModulation),
                "spread" | "spreadspectrum" => Some(SignalEmbedStrategy::SpreadSpectrum),
                "echo" | "echohiding" | "echo_hiding" => Some(SignalEmbedStrategy::EchoHiding),
                _ => None,
            });
        over.unwrap_or(match params.stealth_level {
            StealthLevel::Low => SignalEmbedStrategy::TimeDomainLSB,
            StealthLevel::Medium => SignalEmbedStrategy::FrequencyDomain,
            StealthLevel::High => SignalEmbedStrategy::PhaseModulation,
            StealthLevel::Maximum => SignalEmbedStrategy::SpreadSpectrum,
        })
    }

    fn capacity_info(layout: &SignalLayout, strategy: SignalEmbedStrategy) -> CapacityInfo {
        let total = layout.capacity_bits(strategy);
        let mut info = CapacityInfo::new(total, total.saturating_sub(HEADER_BITS));
        info.format_specific_info.insert(
            "overhead_bytes".to_string(),
            serde_json::Value::from(HEADER_LEN),
        );
        info.format_specific_info.insert(
            "strategy".to_string(),
            serde_json::Value::String(format!("{strategy:?}")),
        );
        info.format_specific_info
            .insert("sample_rate".to_string(), layout.sample_rate.into());
        info.format_specific_info
            .insert("channels".to_string(), layout.channels.into());
        info.format_specific_info
            .insert("frames".to_string(), layout.frames().into());
        info
    }

    async fn embed_with_strategy(
        &self,
        carrier: &mut CarrierData,
        payload: &[u8],
        params: &EmbedParams,
        strategy: SignalEmbedStrategy,
    ) -> Result<EmbedResult> {
        let start = Instant::now();
        let layout = SignalLayout::from_carrier(carrier)?;

        let enc_start = Instant::now();
        let encoded = codec::encode_payload(payload, params)?;
        let encryption_time_ms = if params.encryption_key.is_empty() {
            0
        } else {
            (enc_start.elapsed().as_millis() as u64).max(1)
        };
        let len_u32 = u32::try_from(encoded.len())
            .map_err(|_| invalid("payload too large for signal header"))?;

        let mut stream = Vec::with_capacity(HEADER_LEN + encoded.len());
        stream.extend_from_slice(MAGIC);
        stream.extend_from_slice(&len_u32.to_le_bytes());
        stream.push(strategy.to_u8());
        stream.push(FORMAT_VERSION);
        stream.extend_from_slice(&encoded);

        let needed = stream
            .len()
            .checked_mul(8)
            .ok_or_else(|| invalid("payload too large"))?;
        let available = layout.capacity_bits(strategy);
        if needed > available {
            return Err(SteganoError::InsufficientCapacity { needed, available });
        }
        let bits = bits_of(&stream);
        let seed = pn_seed(&params.encryption_key);

        let mut out = carrier.raw_data.clone();
        match strategy {
            SignalEmbedStrategy::TimeDomainLSB => embed_lsb(&layout, &mut out, &bits),
            SignalEmbedStrategy::FrequencyDomain | SignalEmbedStrategy::PhaseModulation => {
                embed_fft(&layout, &mut out, &bits, strategy)?
            }
            SignalEmbedStrategy::SpreadSpectrum => embed_spread(&layout, &mut out, &bits, seed)?,
            SignalEmbedStrategy::EchoHiding => embed_echo(&layout, &mut out, &bits, seed)?,
        }

        // Self-verify on the final quantised bytes.
        let check = extract_bits(&layout, &out, strategy, bits.len(), seed);
        if check != bits {
            let errors = check.iter().zip(&bits).filter(|(a, b)| a != b).count();
            return Err(invalid(format!(
                "{strategy:?}: {errors} of {} bits would not extract from this carrier \
                 (clipping near full scale?); try another strategy",
                bits.len()
            )));
        }

        carrier.raw_data = out;
        Ok(EmbedResult::success(
            payload.len(),
            PerformanceStats {
                embedding_time_ms: start.elapsed().as_millis() as u64,
                encryption_time_ms,
                gpu_acceleration_used: false,
                memory_usage_bytes: carrier.raw_data.len() * 2,
            },
        ))
    }

    /// Returns `None` if no header for `strategy` is present. Otherwise returns
    /// the result of decoding the payload, which is an `Err` if the header
    /// is valid but the payload is not.
    fn extract_with_strategy(
        layout: &SignalLayout,
        data: &[u8],
        strategy: SignalEmbedStrategy,
        params: &EmbedParams,
    ) -> Option<Result<Vec<u8>>> {
        let cap = layout.capacity_bits(strategy);
        if cap < HEADER_BITS {
            return None;
        }
        let seed = pn_seed(&params.encryption_key);
        let header = bytes_of(&extract_bits(layout, data, strategy, HEADER_BITS, seed));
        if &header[0..4] != MAGIC || header[8] != strategy.to_u8() {
            return None;
        }
        if header[9] != FORMAT_VERSION {
            return Some(Err(SteganoError::IntegrityError(format!(
                "unsupported signal payload version {} (expected {FORMAT_VERSION})",
                header[9]
            ))));
        }
        let len = le_u32(&header, 4) as usize;
        let total_bits = match len
            .checked_add(HEADER_LEN)
            .and_then(|t| t.checked_mul(8))
            .filter(|&t| t <= cap)
        {
            Some(t) => t,
            None => {
                return Some(Err(SteganoError::IntegrityError(format!(
                    "declared payload length {len} exceeds carrier capacity ({cap} bits)"
                ))))
            }
        };
        let all = bytes_of(&extract_bits(layout, data, strategy, total_bits, seed));
        Some(codec::decode_payload(&all[HEADER_LEN..], params))
    }
}

#[async_trait]
impl DataHandler for SignalHandler {
    fn detect_format(&self, data: &[u8]) -> Option<DataFormat> {
        if data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WAVE" {
            Some(DataFormat::WAV)
        } else {
            None
        }
    }

    fn estimate_capacity(&self, carrier: &CarrierData) -> Result<CapacityInfo> {
        // The main value is for FrequencyDomain (Medium stealth). Breakdowns for
        // the other stealth levels are in format_specific_info.
        let layout = SignalLayout::from_carrier(carrier)?;
        let mut info = Self::capacity_info(&layout, SignalEmbedStrategy::FrequencyDomain);
        let per = [
            (
                "capacity_low_stealth_bits",
                SignalEmbedStrategy::TimeDomainLSB,
            ),
            (
                "capacity_medium_stealth_bits",
                SignalEmbedStrategy::FrequencyDomain,
            ),
            (
                "capacity_high_stealth_bits",
                SignalEmbedStrategy::PhaseModulation,
            ),
            (
                "capacity_maximum_stealth_bits",
                SignalEmbedStrategy::SpreadSpectrum,
            ),
            ("capacity_echo_bits", SignalEmbedStrategy::EchoHiding),
        ];
        for (key, s) in per {
            let bits = layout.capacity_bits(s).saturating_sub(HEADER_BITS);
            info.format_specific_info
                .insert(key.to_string(), serde_json::Value::from(bits));
        }
        Ok(info)
    }

    async fn embed(
        &self,
        carrier: &mut CarrierData,
        payload: &[u8],
        params: &EmbedParams,
    ) -> Result<EmbedResult> {
        let strategy = Self::select_strategy(params);
        self.embed_with_strategy(carrier, payload, params, strategy)
            .await
    }

    async fn extract(&self, carrier: &CarrierData, params: &EmbedParams) -> Result<Vec<u8>> {
        // The extraction is self-describing. Each strategy's header is probed
        // in turn, which is cheap: a strategy matches only if both the magic and
        // the strategy tag agree. The first match is decoded exactly once, and
        // its result is returned, including codec errors such as
        // EncryptionError for a wrong or missing password.
        //
        // SpreadSpectrum and EchoHiding headers can only be found with the key
        // that was used to embed, because the PN sequence depends on it.
        let layout = SignalLayout::from_carrier(carrier)?;
        for s in SignalEmbedStrategy::ALL {
            if let Some(result) = Self::extract_with_strategy(&layout, &carrier.raw_data, s, params)
            {
                return result;
            }
        }
        Err(invalid("No valid extraction strategy found"))
    }

    fn verify_integrity(&self, carrier: &CarrierData) -> Result<bool> {
        // Without a key, only unkeyed (or default-seeded) headers can be found.
        let Ok(layout) = SignalLayout::from_carrier(carrier) else {
            return Ok(false);
        };
        let seed = pn_seed(&[]);
        Ok(SignalEmbedStrategy::ALL.into_iter().any(|s| {
            layout.capacity_bits(s) >= HEADER_BITS && {
                let h = bytes_of(&extract_bits(
                    &layout,
                    &carrier.raw_data,
                    s,
                    HEADER_BITS,
                    seed,
                ));
                &h[0..4] == MAGIC && h[8] == s.to_u8()
            }
        }))
    }
}

impl Default for SignalHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::Cursor;

    // ---- carrier generation ------------------------------------------------

    /// Multi-tone plus white noise, about −10 dBFS peak. Returns values in [−1, 1).
    fn synth(frames: usize, channels: usize, rate: u32, seed: u32) -> Vec<f64> {
        let mut st = seed.max(1);
        let mut noise = move || {
            st ^= st << 13;
            st ^= st >> 17;
            st ^= st << 5;
            (st as f64 / u32::MAX as f64) * 2.0 - 1.0
        };
        let tones = [
            (220.0, 0.10),
            (445.0, 0.08),
            (1003.0, 0.05),
            (3301.0, 0.03),
            (7019.0, 0.02),
        ];
        let mut out = Vec::with_capacity(frames * channels);
        for f in 0..frames {
            let t = f as f64 / rate as f64;
            for c in 0..channels {
                let mut v = 0.0;
                for (i, &(hz, a)) in tones.iter().enumerate() {
                    let ph = (i + c) as f64 * 0.7;
                    v += a * (2.0 * std::f64::consts::PI * hz * t + ph).sin();
                }
                v += 0.03 * noise();
                out.push(v);
            }
        }
        out
    }

    fn make_wav(bits: u16, channels: u16, rate: u32, secs: f64, float: bool) -> Vec<u8> {
        let frames = (secs * rate as f64) as usize;
        let spec = hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: bits,
            sample_format: if float {
                hound::SampleFormat::Float
            } else {
                hound::SampleFormat::Int
            },
        };
        let mut cur = Cursor::new(Vec::new());
        {
            let mut w = hound::WavWriter::new(&mut cur, spec).unwrap();
            let full = (1i64 << (bits - 1)) as f64;
            for v in synth(frames, channels as usize, rate, 12345 + bits as u32) {
                if float {
                    w.write_sample(v as f32).unwrap();
                } else {
                    w.write_sample((v * full).round() as i32).unwrap();
                }
            }
            w.finalize().unwrap();
        }
        cur.into_inner()
    }

    fn read_spec_and_ints(bytes: &[u8]) -> (hound::WavSpec, Vec<i32>) {
        let mut r = hound::WavReader::new(Cursor::new(bytes)).unwrap();
        let spec = r.spec();
        let s = r.samples::<i32>().map(|s| s.unwrap()).collect();
        (spec, s)
    }

    fn carrier(format: DataFormat, raw: Vec<u8>) -> CarrierData {
        CarrierData {
            format,
            raw_data: raw,
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        }
    }

    fn params(strategy: &str, password: Option<&str>) -> EmbedParams {
        let mut p = EmbedParams::with_error_correction(
            password.map(|s| s.as_bytes().to_vec()).unwrap_or_default(),
            ECLevel::None,
        );
        p.format_specific
            .insert("signal_strategy".into(), serde_json::Value::from(strategy));
        p
    }

    /// Seconds of audio for each strategy, enough for a short encrypted payload.
    fn secs_for(strategy: &str) -> f64 {
        match strategy {
            "timedomain" => 0.5,
            "frequency" | "phase" => 1.5,
            "spread" => 9.5,
            _ => 18.5,
        }
    }

    const STRATS: [&str; 5] = ["timedomain", "frequency", "phase", "spread", "echo"];

    async fn roundtrip_case(strategy: &str, bits: u16, channels: u16, password: Option<&str>) {
        let handler = SignalHandler::new();
        let wav = make_wav(bits, channels, 44100, secs_for(strategy), false);
        let (spec0, s0) = read_spec_and_ints(&wav);
        let mut c = carrier(DataFormat::WAV, wav.clone());
        let p = params(strategy, password);
        let payload = b"signals #43 roundtrip";
        handler
            .embed(&mut c, payload, &p)
            .await
            .unwrap_or_else(|e| panic!("{strategy} {bits}b {channels}ch embed: {e}"));

        assert_eq!(
            &c.raw_data[0..4],
            b"RIFF",
            "{strategy}: RIFF header destroyed"
        );
        assert_eq!(c.raw_data.len(), wav.len());
        let (spec1, s1) = read_spec_and_ints(&c.raw_data);
        assert_eq!(spec0, spec1, "{strategy}: spec changed");
        assert_eq!(s0.len(), s1.len(), "{strategy}: sample count changed");
        if strategy == "timedomain" {
            assert!(s0.iter().zip(&s1).all(|(a, b)| (a - b).abs() <= 1));
        }
        assert_ne!(s0, s1, "{strategy}: nothing was embedded");
        let sig: f64 = s0.iter().map(|&a| (a as f64).powi(2)).sum();
        let err: f64 = s0
            .iter()
            .zip(&s1)
            .map(|(&a, &b)| ((a - b) as f64).powi(2))
            .sum();
        // Whole-file SNR. On this test signal it measures about 88 dB (LSB),
        // 65 dB (frequency), 49 dB (phase), 29 dB (spread) and 15 dB (echo) when
        // the payload fills the carrier. The echo figure counts the audible echo
        // itself as "noise".
        let snr = 10.0 * (sig / err).log10();
        assert!(snr > 12.0, "{strategy}: SNR {snr:.1} dB");

        let out = handler.extract(&c, &p).await.unwrap();
        assert_eq!(out, payload, "{strategy} {bits}b {channels}ch");

        if password.is_some() {
            let wrong = params(strategy, Some("wrong password"));
            assert!(handler.extract(&c, &wrong).await.is_err());
            // Unkeyed strategies still find the header without a password, so
            // the codec's EncryptionError comes through. (Keyed SS/echo cannot
            // find their header at all without the key.)
            if matches!(strategy, "timedomain" | "frequency" | "phase") {
                let none = params(strategy, None);
                let r = handler.extract(&c, &none).await;
                assert!(
                    matches!(r, Err(SteganoError::EncryptionError(_))),
                    "{strategy}: {r:?}"
                );
            }
        }
    }

    #[tokio::test]
    async fn test_all_strategies_16bit_mono() {
        for s in STRATS {
            roundtrip_case(s, 16, 1, None).await;
            roundtrip_case(s, 16, 1, Some("pw")).await;
        }
    }

    #[tokio::test]
    async fn test_all_strategies_16bit_stereo() {
        for s in STRATS {
            roundtrip_case(s, 16, 2, None).await;
            roundtrip_case(s, 16, 2, Some("pw")).await;
        }
    }

    #[tokio::test]
    async fn test_all_strategies_24bit_mono() {
        for s in STRATS {
            roundtrip_case(s, 24, 1, None).await;
            roundtrip_case(s, 24, 1, Some("pw")).await;
        }
    }

    #[tokio::test]
    async fn test_lsb_8bit_32bit_and_float() {
        let handler = SignalHandler::new();
        for (bits, float) in [(8u16, false), (32, false), (32, true)] {
            let wav = make_wav(bits, 2, 22050, 0.5, float);
            let mut c = carrier(DataFormat::WAV, wav.clone());
            let p = params("timedomain", Some("pw"));
            handler.embed(&mut c, b"depth", &p).await.unwrap();
            let r0 = hound::WavReader::new(Cursor::new(&wav)).unwrap();
            let r1 = hound::WavReader::new(Cursor::new(&c.raw_data)).unwrap();
            assert_eq!(r0.spec(), r1.spec());
            assert_eq!(r0.len(), r1.len());
            assert_eq!(handler.extract(&c, &p).await.unwrap(), b"depth");
        }
        // Float carriers also work with the FFT strategies.
        let wav = make_wav(32, 1, 44100, 1.0, true);
        let mut c = carrier(DataFormat::WAV, wav);
        let p = params("frequency", None);
        handler.embed(&mut c, b"float fft", &p).await.unwrap();
        assert_eq!(handler.extract(&c, &p).await.unwrap(), b"float fft");
    }

    #[tokio::test]
    async fn test_extra_chunks_preserved() {
        let wav = make_wav(16, 1, 44100, 0.5, false);
        // Insert an odd-sized LIST chunk (padded) between fmt and data.
        let fmt_end = 12 + 8 + le_u32(&wav, 16) as usize;
        let mut list = b"LIST".to_vec();
        list.extend_from_slice(&5u32.to_le_bytes());
        list.extend_from_slice(b"INFOx\0");
        let mut riff = wav[..fmt_end].to_vec();
        riff.extend_from_slice(&list);
        riff.extend_from_slice(&wav[fmt_end..]);
        riff.extend_from_slice(b"junk"); // trailing bytes
        let size = (riff.len() - 8) as u32;
        riff[4..8].copy_from_slice(&size.to_le_bytes());

        let handler = SignalHandler::new();
        let mut c = carrier(DataFormat::WAV, riff.clone());
        let p = params("timedomain", None);
        handler.embed(&mut c, b"chunks", &p).await.unwrap();
        let data_body = fmt_end + list.len() + 8;
        assert_eq!(&c.raw_data[..data_body], &riff[..data_body]);
        assert_eq!(&c.raw_data[c.raw_data.len() - 4..], b"junk");
        assert_eq!(handler.extract(&c, &p).await.unwrap(), b"chunks");
    }

    /// A tiny RIFF with an arbitrary sample rate and a data chunk of `n` 16-bit samples.
    fn crafted_wav(rate: u32, samples: &[i16]) -> Vec<u8> {
        let mut d = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
        d.extend_from_slice(&16u32.to_le_bytes());
        d.extend_from_slice(&1u16.to_le_bytes());
        d.extend_from_slice(&1u16.to_le_bytes());
        d.extend_from_slice(&rate.to_le_bytes());
        d.extend_from_slice(&(rate.wrapping_mul(2)).to_le_bytes());
        d.extend_from_slice(&2u16.to_le_bytes());
        d.extend_from_slice(&16u16.to_le_bytes());
        d.extend_from_slice(b"data");
        d.extend_from_slice(&((samples.len() * 2) as u32).to_le_bytes());
        for s in samples {
            d.extend_from_slice(&s.to_le_bytes());
        }
        let size = (d.len() - 8) as u32;
        d[4..8].copy_from_slice(&size.to_le_bytes());
        d
    }

    #[tokio::test]
    async fn test_crafted_low_rate_huge_length_is_error() {
        // Header bits: "SGSJ", len = 0xFFFF_FFF0, strategy 0, version 2, in the LSBs.
        let mut hdr = MAGIC.to_vec();
        hdr.extend_from_slice(&0xFFFF_FFF0u32.to_le_bytes());
        hdr.extend_from_slice(&[0, FORMAT_VERSION]);
        let mut samples = vec![0i16; 4096];
        for (i, b) in bits_of(&hdr).into_iter().enumerate() {
            samples[i] = b as i16;
        }
        for rate in [8u32, 1, 999] {
            let c = carrier(DataFormat::WAV, crafted_wav(rate, &samples));
            let handler = SignalHandler::new();
            let r = handler.extract(&c, &EmbedParams::default()).await;
            assert!(r.is_err(), "rate {rate}: expected Err, got {r:?}");
            // Embedding with every strategy at this rate must not panic either.
            for s in STRATS {
                let mut c2 = carrier(DataFormat::WAV, crafted_wav(rate, &samples));
                let _ = handler.embed(&mut c2, b"x", &params(s, None)).await;
            }
            assert!(handler.estimate_capacity(&c).is_ok());
            let _ = handler.verify_integrity(&c).unwrap();
        }
        // Sample rate 0 is rejected outright.
        let c = carrier(DataFormat::WAV, crafted_wav(0, &samples));
        assert!(SignalHandler::new()
            .extract(&c, &EmbedParams::default())
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_malformed_wavs_do_not_panic() {
        let handler = SignalHandler::new();
        let good = make_wav(16, 1, 8000, 0.1, false);
        let mut cases: Vec<Vec<u8>> = vec![
            b"RIFF\0\0\0\0WAVE".to_vec(),
            b"RIFF\0\0\0\0WAVEfmt \xff\xff\xff\xff".to_vec(),
            b"not a wav at all".to_vec(),
        ];
        for cut in [13, 20, 36, 44, 45] {
            cases.push(good[..cut.min(good.len())].to_vec());
        }
        let mut bad_align = good.clone();
        bad_align[32] = 7; // block_align
        cases.push(bad_align);
        for raw in cases {
            let c = carrier(DataFormat::WAV, raw.clone());
            let _ = handler.extract(&c, &EmbedParams::default()).await;
            let _ = handler.estimate_capacity(&c);
            let _ = handler.verify_integrity(&c);
            let mut c2 = carrier(DataFormat::WAV, raw);
            let _ = handler.embed(&mut c2, b"x", &params("echo", None)).await;
        }
    }

    #[tokio::test]
    async fn test_silent_carrier_echo_errors_at_embed() {
        let handler = SignalHandler::new();
        let mut c = carrier(DataFormat::WAV, crafted_wav(44100, &vec![0i16; 44100 * 4]));
        let r = handler.embed(&mut c, b"x", &params("echo", None)).await;
        assert!(r.is_err());
    }

    #[tokio::test]
    async fn test_raw_binary_signal_odd_length_keeps_last_byte() {
        let handler = SignalHandler::new();
        let mut raw: Vec<u8> = (0..20001u32).map(|i| (i * 7 % 251) as u8).collect();
        raw[20000] = 0xAB;
        let mut c = carrier(DataFormat::BinarySignal, raw.clone());
        let p = params("timedomain", Some("pw"));
        handler.embed(&mut c, b"raw odd", &p).await.unwrap();
        assert_eq!(c.raw_data.len(), raw.len());
        assert_eq!(c.raw_data[20000], 0xAB);
        assert_eq!(handler.extract(&c, &p).await.unwrap(), b"raw odd");
    }

    #[test]
    fn test_audio_format_detection() {
        let handler = SignalHandler::new();
        assert_eq!(
            handler.detect_format(b"RIFF\x24\x08\x00\x00WAVEfmt "),
            Some(DataFormat::WAV)
        );
        assert_eq!(handler.detect_format(b"fLaCxxxxxxxx"), None);
        assert_eq!(handler.detect_format(b"unknown_fmt"), None);
    }

    #[test]
    fn test_capacity_estimation_ordering() {
        let handler = SignalHandler::new();
        let c = carrier(DataFormat::WAV, make_wav(16, 2, 44100, 1.0, false));
        let cap = handler.estimate_capacity(&c).unwrap();
        let get = |k: &str| cap.format_specific_info[k].as_u64().unwrap();
        assert_eq!(cap.format_specific_info["sample_rate"], 44100);
        assert_eq!(cap.format_specific_info["channels"], 2);
        assert!(get("capacity_low_stealth_bits") >= get("capacity_medium_stealth_bits"));
        assert!(get("capacity_medium_stealth_bits") >= get("capacity_high_stealth_bits"));
        assert!(get("capacity_high_stealth_bits") >= get("capacity_maximum_stealth_bits"));
        assert!(get("capacity_maximum_stealth_bits") > 0);
    }

    #[tokio::test]
    async fn test_insufficient_capacity() {
        let handler = SignalHandler::new();
        let mut c = carrier(DataFormat::WAV, make_wav(16, 1, 44100, 0.01, false));
        let r = handler
            .embed(&mut c, &[0u8; 5000], &params("timedomain", None))
            .await;
        assert!(matches!(r, Err(SteganoError::InsufficientCapacity { .. })));
    }

    #[tokio::test]
    async fn test_verify_integrity() {
        let handler = SignalHandler::new();
        let wav = make_wav(16, 1, 44100, 0.5, false);
        let mut c = carrier(DataFormat::WAV, wav.clone());
        assert!(!handler.verify_integrity(&c).unwrap());
        handler
            .embed(&mut c, b"hi", &params("timedomain", None))
            .await
            .unwrap();
        assert!(handler.verify_integrity(&c).unwrap());
    }

    #[test]
    fn test_bit_ordering() {
        assert_eq!(bytes_of(&bits_of(b"SGSJ")), b"SGSJ");
    }

    #[test]
    fn test_strategy_tag_roundtrip() {
        for s in SignalEmbedStrategy::ALL {
            assert_eq!(SignalEmbedStrategy::from_u8(s.to_u8()), Some(s));
        }
        assert_eq!(SignalEmbedStrategy::from_u8(9), None);
    }
}
