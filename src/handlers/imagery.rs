//! Image data format handlers (PNG / TIFF) with pixel-domain steganography.
//!
//! # How embedding works
//!
//! The carrier file is **decoded to pixel samples first** (PNG via the `png`
//! crate, TIFF via the `tiff` crate).  Payload bits are written only into the
//! samples of the colour channels (alpha is never touched), and the image is
//! then re-encoded with the same dimensions, colour type and bit depth.  For
//! PNG, every non-pixel chunk of the original (text, gamma, ICC, pHYs, …) is
//! copied verbatim; only `IHDR`/`IDAT` are regenerated.  For TIFF the
//! compression scheme (none / LZW / Deflate / PackBits) and predictor are
//! preserved; other tags are not.
//!
//! The file format is determined from magic bytes (authoritative), so a PNG
//! whose `CarrierData::format` says TIFF is still handled as PNG.  Data that is
//! neither PNG nor TIFF is only accepted when the caller explicitly declares
//! [`DataFormat::BINARY`]; it is then treated as a raw 8-bit sample stream.
//!
//! # Supported layouts
//!
//! | Container | Colour types                       | Bit depths |
//! |-----------|------------------------------------|------------|
//! | PNG       | Gray, GrayAlpha, RGB, RGBA          | 8, 16      |
//! | TIFF      | Gray (BlackIsZero), RGB, RGBA       | 8, 16      |
//!
//! Palette/indexed images, sub-byte depths, signed/float samples, planar TIFF,
//! multi-page TIFF, CMYK/YCbCr/Lab and APNG are rejected with
//! [`SteganoError::UnsupportedFormat`] / [`SteganoError::InvalidParameter`].
//! Interlaced PNGs are accepted but re-encoded non-interlaced.
//!
//! # Strategies and exact capacity
//!
//! Let `S` be the number of colour samples (`width × height × colour_channels`).
//!
//! * **SimpleLSB** – 1 bit in the LSB of every colour sample: `S` bits.
//! * **AdaptiveLSB** – 1 bit per colour sample whose local variation (sum of
//!   absolute differences to its 4 same-channel neighbours, computed on
//!   LSB-masked values) exceeds [`ADAPTIVE_THRESHOLD`].
//! * **EdgeBased** – 1 bit per colour sample of every pixel whose luma gradient
//!   (central differences on LSB-masked values) exceeds [`EDGE_THRESHOLD`].
//! * **DCTFrequency** – 8-sample horizontal blocks per row and colour channel;
//!   4 bits per block (QIM on DCT coefficients 2–5):
//!   `height × colour_channels × ⌊width / 8⌋ × 4` bits.
//!
//! Because LSB-strategy eligibility only looks at LSB-masked values, embedding
//! never changes which samples are eligible, so the decoder sees exactly the
//! same positions as the encoder and the reported capacity is exact.
//!
//! In all cases 80 bits are used by the `IMSJ` header; the remaining bits hold
//! the output of [`crate::codec::encode_payload`].

use super::DataHandler;
use crate::core::*;
use async_trait::async_trait;
use std::io::Cursor;
use std::time::Instant;

/// Handler for image carrier files (PNG, TIFF, TIFF-based microscopy).
///
/// See the [module documentation](self) for supported layouts and strategies.
pub struct ImageryHandler;

/// Image embedding strategy.
///
/// This enum is `#[non_exhaustive]` — future strategies can be added without
/// breaking existing match arms.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageEmbedStrategy {
    /// LSB embedding in every colour sample.  Highest capacity.
    SimpleLSB,
    /// LSB embedding restricted to colour samples in high-variation regions.
    AdaptiveLSB,
    /// QIM in mid-frequency coefficients of 8-sample DCT-II blocks.
    /// Lower capacity; larger (bounded) per-sample change.
    DCTFrequency,
    /// LSB embedding restricted to pixels on luminance edges.
    EdgeBased,
}

impl ImageEmbedStrategy {
    fn to_u8(self) -> u8 {
        match self {
            ImageEmbedStrategy::SimpleLSB => 0,
            ImageEmbedStrategy::AdaptiveLSB => 1,
            ImageEmbedStrategy::DCTFrequency => 2,
            // 3 was PVD (removed in v0.4.0) — slot intentionally unused.
            ImageEmbedStrategy::EdgeBased => 4,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(ImageEmbedStrategy::SimpleLSB),
            1 => Some(ImageEmbedStrategy::AdaptiveLSB),
            2 => Some(ImageEmbedStrategy::DCTFrequency),
            4 => Some(ImageEmbedStrategy::EdgeBased),
            _ => None,
        }
    }

    const ALL: [ImageEmbedStrategy; 4] = [
        ImageEmbedStrategy::SimpleLSB,
        ImageEmbedStrategy::AdaptiveLSB,
        ImageEmbedStrategy::DCTFrequency,
        ImageEmbedStrategy::EdgeBased,
    ];
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Header magic.
const MAGIC: &[u8; 4] = b"IMSJ";
/// Header: 4 magic + 4 length (LE) + 1 strategy + 1 codec mode.
const HEADER_LEN: usize = 10;
const HEADER_BITS: usize = HEADER_LEN * 8;
/// Codec mode byte: payload is the output of `codec::encode_payload`.
const CODEC_MODE: u8 = 2;

/// AdaptiveLSB threshold (8-bit units; scaled ×256 for 16-bit images).
pub const ADAPTIVE_THRESHOLD: i64 = 16;
/// EdgeBased gradient-magnitude threshold (8-bit luma units; scaled for 16-bit).
pub const EDGE_THRESHOLD: i64 = 20;

/// QIM step for DCT embedding.  The decoder reads bit = round(c / Δ) mod 2.
const DCT_DELTA: f64 = 8.0;
/// Mid-frequency coefficient indices carrying one bit each.
const DCT_COEFF_INDICES: [usize; 4] = [2, 3, 4, 5];
const DCT_BLOCK: usize = 8;
/// Each coefficient moves by at most Δ, each basis function has amplitude
/// ≤ √(2/8) = 0.5, so a block sample moves by at most 4·0.5·Δ = 2Δ.  Samples
/// of a used block are first clamped into `[2Δ, max − 2Δ]` so that the
/// inverse DCT can never leave the valid range (no output clamping → no
/// QIM desynchronisation).  Rounding to integers perturbs each coefficient by
/// at most √8·0.5 ≈ 1.41 < Δ/2, so parity is always recovered.
pub const DCT_MAX_SAMPLE_CHANGE: u32 = 16;

/// Refuse to decode images whose sample buffer would exceed this many bytes.
const MAX_DECODED_BYTES: usize = 512 * 1024 * 1024;

// ---------------------------------------------------------------------------
// 8-point DCT-II / IDCT-II helpers (orthonormal)
// ---------------------------------------------------------------------------

fn dct8(input: &[f64; 8]) -> [f64; 8] {
    use std::f64::consts::PI;
    let mut out = [0f64; 8];
    for (k, out_k) in out.iter_mut().enumerate() {
        let alpha = if k == 0 {
            (1.0f64 / 8.0).sqrt()
        } else {
            (2.0f64 / 8.0).sqrt()
        };
        *out_k = alpha
            * (0..8usize)
                .map(|j| input[j] * (PI * (2 * j + 1) as f64 * k as f64 / 16.0).cos())
                .sum::<f64>();
    }
    out
}

fn idct8(input: &[f64; 8]) -> [f64; 8] {
    use std::f64::consts::PI;
    let mut out = [0f64; 8];
    for (j, out_j) in out.iter_mut().enumerate() {
        *out_j = (0..8usize)
            .map(|k| {
                let alpha = if k == 0 {
                    (1.0f64 / 8.0).sqrt()
                } else {
                    (2.0f64 / 8.0).sqrt()
                };
                alpha * input[k] * (PI * (2 * j + 1) as f64 * k as f64 / 16.0).cos()
            })
            .sum::<f64>();
    }
    out
}

/// Move `c` to the nearest point of the lattice `Δ·(2ℤ + bit)`.
fn qim_quantise(c: f64, bit: u8) -> f64 {
    let b = bit as f64;
    DCT_DELTA * (2.0 * ((c / DCT_DELTA - b) / 2.0).round() + b)
}

fn qim_decode(c: f64) -> u8 {
    ((c / DCT_DELTA).round() as i64).rem_euclid(2) as u8
}

// ---------------------------------------------------------------------------
// Decoded image model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TiffLayout {
    Gray,
    Rgb,
    Rgba,
}

#[derive(Debug, Clone)]
enum Container {
    Png {
        color: png::ColorType,
    },
    Tiff {
        layout: TiffLayout,
        compression: u16,
        predictor: u16,
    },
    Raw,
}

/// Pixel samples of a decoded carrier image (row-major, interleaved).
#[derive(Debug, Clone)]
struct DecodedImage {
    width: usize,
    height: usize,
    /// Samples per pixel (including alpha).
    channels: usize,
    /// Samples per pixel that carry colour (alpha is the last channel, if any).
    color_channels: usize,
    bit_depth: u8,
    samples: Vec<u16>,
    container: Container,
}

fn invalid(msg: impl Into<String>) -> SteganoError {
    SteganoError::InvalidParameter(msg.into())
}

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

fn is_png(data: &[u8]) -> bool {
    data.starts_with(&PNG_SIGNATURE)
}

fn is_tiff(data: &[u8]) -> bool {
    data.starts_with(&[0x49, 0x49, 0x2A, 0x00]) || data.starts_with(&[0x4D, 0x4D, 0x00, 0x2A])
}

impl DecodedImage {
    /// Decode a carrier.  Magic bytes decide PNG vs TIFF; anything else is
    /// only accepted as raw 8-bit samples when `format == BINARY`.
    fn decode(carrier: &CarrierData) -> Result<Self> {
        let data = &carrier.raw_data;
        if is_png(data) {
            Self::decode_png(data)
        } else if is_tiff(data) {
            Self::decode_tiff(data)
        } else if carrier.format == DataFormat::BINARY {
            Ok(Self::decode_raw(carrier))
        } else {
            Err(invalid(format!(
                "Imagery carrier declared as {:?} is neither a PNG nor a TIFF file \
                 (only PNG and TIFF pixel data are supported; use DataFormat::BINARY \
                 for raw byte-LSB embedding)",
                carrier.format
            )))
        }
    }

    fn decode_raw(carrier: &CarrierData) -> Self {
        let len = carrier.raw_data.len();
        let (width, height, channels) = carrier
            .dimensions
            .as_ref()
            .and_then(|d| {
                let w = d.width as usize;
                let h = d.height as usize;
                let c = d.channels.unwrap_or(1).max(1) as usize;
                (w.checked_mul(h)?.checked_mul(c)? == len && len > 0).then_some((w, h, c))
            })
            .unwrap_or((len, 1, 1));
        DecodedImage {
            width,
            height,
            channels,
            color_channels: channels,
            bit_depth: 8,
            samples: carrier.raw_data.iter().map(|&b| b as u16).collect(),
            container: Container::Raw,
        }
    }

    fn decode_png(data: &[u8]) -> Result<Self> {
        let mut decoder = png::Decoder::new(Cursor::new(data));
        decoder.set_transformations(png::Transformations::IDENTITY);
        let mut reader = decoder
            .read_info()
            .map_err(|e| invalid(format!("PNG decode error: {e}")))?;
        let info = reader.info();
        if info.animation_control.is_some() {
            return Err(invalid("Animated PNG (APNG) carriers are not supported"));
        }
        let (width, height) = (info.width as usize, info.height as usize);
        let color = info.color_type;
        let (channels, color_channels) = match color {
            png::ColorType::Grayscale => (1, 1),
            png::ColorType::GrayscaleAlpha => (2, 1),
            png::ColorType::Rgb => (3, 3),
            png::ColorType::Rgba => (4, 3),
            png::ColorType::Indexed => {
                return Err(invalid(
                    "Palette/indexed PNG carriers are not supported \
                     (LSB changes would alter palette indices); convert to RGB first",
                ))
            }
        };
        let bit_depth = match info.bit_depth {
            png::BitDepth::Eight => 8u8,
            png::BitDepth::Sixteen => 16u8,
            d => {
                return Err(invalid(format!(
                    "PNG bit depth {} is not supported (only 8 and 16 bits per sample)",
                    d as u8
                )))
            }
        };
        let buf_len = reader
            .output_buffer_size()
            .ok_or_else(|| invalid("PNG dimensions overflow"))?;
        if buf_len > MAX_DECODED_BYTES {
            return Err(invalid(format!(
                "PNG too large to decode ({buf_len} bytes > {MAX_DECODED_BYTES})"
            )));
        }
        let mut buf = vec![0u8; buf_len];
        let out = reader
            .next_frame(&mut buf)
            .map_err(|e| invalid(format!("PNG decode error: {e}")))?;
        let n = width
            .checked_mul(height)
            .and_then(|p| p.checked_mul(channels))
            .ok_or_else(|| invalid("PNG dimensions overflow"))?;
        let bytes_per_sample = (bit_depth / 8) as usize;
        let needed = n
            .checked_mul(bytes_per_sample)
            .ok_or_else(|| invalid("PNG dimensions overflow"))?;
        if out.buffer_size() < needed || buf.len() < needed {
            return Err(invalid("PNG frame shorter than declared dimensions"));
        }
        let samples: Vec<u16> = if bit_depth == 8 {
            buf[..n].iter().map(|&b| b as u16).collect()
        } else {
            let (pairs, _) = buf[..needed].as_chunks::<2>();
            pairs.iter().map(|&c| u16::from_be_bytes(c)).collect()
        };
        Ok(DecodedImage {
            width,
            height,
            channels,
            color_channels,
            bit_depth,
            samples,
            container: Container::Png { color },
        })
    }

    fn decode_tiff(data: &[u8]) -> Result<Self> {
        use tiff::decoder::{Decoder, DecodingResult};
        use tiff::tags::Tag;
        use tiff::ColorType;

        let terr = |e: tiff::TiffError| invalid(format!("TIFF decode error: {e}"));
        let mut dec = Decoder::new(Cursor::new(data)).map_err(terr)?;
        if dec.more_images() {
            return Err(invalid(
                "Multi-page TIFF carriers are not supported (only single-image files)",
            ));
        }
        let (w, h) = dec.dimensions().map_err(terr)?;
        let photometric: u16 = dec
            .find_tag_unsigned(Tag::PhotometricInterpretation)
            .map_err(terr)?
            .unwrap_or(1);
        let planar: u16 = dec
            .find_tag_unsigned(Tag::PlanarConfiguration)
            .map_err(terr)?
            .unwrap_or(1);
        if planar != 1 {
            return Err(invalid(
                "Planar-configuration TIFF carriers are not supported (only chunky/interleaved)",
            ));
        }
        let compression: u16 = dec
            .find_tag_unsigned(Tag::Compression)
            .map_err(terr)?
            .unwrap_or(1);
        let predictor: u16 = dec
            .find_tag_unsigned(Tag::Predictor)
            .map_err(terr)?
            .unwrap_or(1);
        let color = dec.colortype().map_err(terr)?;
        let (layout, bits) = match (color, photometric) {
            (ColorType::Gray(b), 1) => (TiffLayout::Gray, b),
            (ColorType::RGB(b), 2) => (TiffLayout::Rgb, b),
            (ColorType::RGBA(b), 2) => (TiffLayout::Rgba, b),
            (ColorType::Palette(_), _) => {
                return Err(invalid(
                    "Palette/indexed TIFF carriers are not supported; convert to RGB first",
                ))
            }
            (c, p) => {
                return Err(invalid(format!(
                    "TIFF layout {c:?} (photometric {p}) is not supported; \
                     supported: Gray (BlackIsZero), RGB, RGBA"
                )))
            }
        };
        if bits != 8 && bits != 16 {
            return Err(invalid(format!(
                "TIFF bit depth {bits} is not supported (only 8 and 16 bits per sample)"
            )));
        }
        let (channels, color_channels) = match layout {
            TiffLayout::Gray => (1, 1),
            TiffLayout::Rgb => (3, 3),
            TiffLayout::Rgba => (4, 3),
        };
        let (width, height) = (w as usize, h as usize);
        let n = width
            .checked_mul(height)
            .and_then(|p| p.checked_mul(channels))
            .ok_or_else(|| invalid("TIFF dimensions overflow"))?;
        if n.saturating_mul(bits as usize / 8) > MAX_DECODED_BYTES {
            return Err(invalid("TIFF too large to decode"));
        }
        let samples: Vec<u16> = match dec.read_image().map_err(terr)? {
            DecodingResult::U8(v) if bits == 8 => v.into_iter().map(|b| b as u16).collect(),
            DecodingResult::U16(v) if bits == 16 => v,
            _ => {
                return Err(invalid(
                    "TIFF sample format is not supported (only unsigned 8/16-bit integers)",
                ))
            }
        };
        if samples.len() != n {
            return Err(invalid(format!(
                "TIFF decoded {} samples, expected {n}",
                samples.len()
            )));
        }
        Ok(DecodedImage {
            width,
            height,
            channels,
            color_channels,
            bit_depth: bits,
            samples,
            container: Container::Tiff {
                layout,
                compression,
                predictor,
            },
        })
    }

    fn layout_name(&self) -> String {
        let kind = match &self.container {
            Container::Png { color } => format!("PNG {color:?}"),
            Container::Tiff { layout, .. } => format!("TIFF {layout:?}"),
            Container::Raw => "raw bytes".to_string(),
        };
        format!(
            "{kind} {}-bit {}x{}",
            self.bit_depth, self.width, self.height
        )
    }

    fn max_value(&self) -> i64 {
        (1i64 << self.bit_depth) - 1
    }

    /// Scale factor from 8-bit threshold units to native units.
    fn scale(&self) -> i64 {
        1i64 << (self.bit_depth - 8)
    }

    /// LSB-masked value — invariant under LSB embedding.
    #[inline]
    fn masked(&self, idx: usize) -> i64 {
        (self.samples[idx] & !1) as i64
    }

    #[inline]
    fn is_color(&self, idx: usize) -> bool {
        idx % self.channels < self.color_channels
    }

    fn adaptive_eligible(&self, idx: usize) -> bool {
        if !self.is_color(idx) {
            return false;
        }
        let pixel = idx / self.channels;
        let (x, y) = (pixel % self.width, pixel / self.width);
        let row = self.width * self.channels;
        let v = self.masked(idx);
        let mut var = 0i64;
        if x > 0 {
            var += (v - self.masked(idx - self.channels)).abs();
        }
        if x + 1 < self.width {
            var += (v - self.masked(idx + self.channels)).abs();
        }
        if y > 0 {
            var += (v - self.masked(idx - row)).abs();
        }
        if y + 1 < self.height {
            var += (v - self.masked(idx + row)).abs();
        }
        var > ADAPTIVE_THRESHOLD * self.scale()
    }

    /// Sum of LSB-masked colour samples of pixel (x, y), coordinates clamped.
    fn luma_sum(&self, x: isize, y: isize) -> i64 {
        let x = x.clamp(0, self.width as isize - 1) as usize;
        let y = y.clamp(0, self.height as isize - 1) as usize;
        let base = (y * self.width + x) * self.channels;
        (0..self.color_channels)
            .map(|c| self.masked(base + c))
            .sum()
    }

    fn edge_eligible(&self, idx: usize) -> bool {
        if !self.is_color(idx) {
            return false;
        }
        let pixel = idx / self.channels;
        let (x, y) = ((pixel % self.width) as isize, (pixel / self.width) as isize);
        let gx = self.luma_sum(x + 1, y) - self.luma_sum(x - 1, y);
        let gy = self.luma_sum(x, y + 1) - self.luma_sum(x, y - 1);
        let t = EDGE_THRESHOLD * self.scale() * self.color_channels as i64;
        gx * gx + gy * gy > t * t
    }

    fn lsb_eligible(&self, strategy: ImageEmbedStrategy, idx: usize) -> bool {
        match strategy {
            ImageEmbedStrategy::SimpleLSB => self.is_color(idx),
            ImageEmbedStrategy::AdaptiveLSB => self.adaptive_eligible(idx),
            ImageEmbedStrategy::EdgeBased => self.edge_eligible(idx),
            ImageEmbedStrategy::DCTFrequency => false,
        }
    }

    /// Eligible sample indices for an LSB strategy, in embedding order.
    fn lsb_positions(&self, strategy: ImageEmbedStrategy) -> impl Iterator<Item = usize> + '_ {
        (0..self.samples.len()).filter(move |&i| self.lsb_eligible(strategy, i))
    }

    fn dct_blocks_per_row(&self) -> usize {
        self.width / DCT_BLOCK
    }

    fn dct_block_count(&self) -> usize {
        self.height
            .saturating_mul(self.color_channels)
            .saturating_mul(self.dct_blocks_per_row())
    }

    /// Sample index of element `k` of DCT block `b`.
    fn dct_index(&self, b: usize, k: usize) -> usize {
        let bpr = self.dct_blocks_per_row();
        let bx = b % bpr;
        let rc = b / bpr;
        let (y, c) = (rc / self.color_channels, rc % self.color_channels);
        (y * self.width + bx * DCT_BLOCK + k) * self.channels + c
    }

    /// Exact embedding capacity in bits (including the header).
    fn capacity_bits(&self, strategy: ImageEmbedStrategy) -> usize {
        match strategy {
            ImageEmbedStrategy::DCTFrequency => self
                .dct_block_count()
                .saturating_mul(DCT_COEFF_INDICES.len()),
            s => self.lsb_positions(s).count(),
        }
    }

    /// Write `bits` (one bit per element) using `strategy`.
    fn write_bits(&mut self, strategy: ImageEmbedStrategy, bits: &[u8]) -> Result<()> {
        let available = self.capacity_bits(strategy);
        if bits.len() > available {
            return Err(SteganoError::InsufficientCapacity {
                needed: bits.len(),
                available,
            });
        }
        match strategy {
            ImageEmbedStrategy::DCTFrequency => {
                let lo = DCT_MAX_SAMPLE_CHANGE as f64;
                let hi = (self.max_value() - DCT_MAX_SAMPLE_CHANGE as i64) as f64;
                for (b, chunk) in bits.chunks(DCT_COEFF_INDICES.len()).enumerate() {
                    let idx: [usize; DCT_BLOCK] = std::array::from_fn(|k| self.dct_index(b, k));
                    let block: [f64; DCT_BLOCK] =
                        std::array::from_fn(|k| (self.samples[idx[k]] as f64).clamp(lo, hi));
                    let mut coeffs = dct8(&block);
                    for (&ci, &bit) in DCT_COEFF_INDICES.iter().zip(chunk) {
                        coeffs[ci] = qim_quantise(coeffs[ci], bit);
                    }
                    let restored = idct8(&coeffs);
                    let max = self.max_value() as f64;
                    for k in 0..DCT_BLOCK {
                        // Clamp is a no-op by construction (see DCT_MAX_SAMPLE_CHANGE).
                        self.samples[idx[k]] = restored[k].round().clamp(0.0, max) as u16;
                    }
                }
            }
            s => {
                let positions: Vec<usize> = self.lsb_positions(s).take(bits.len()).collect();
                for (&p, &bit) in positions.iter().zip(bits) {
                    self.samples[p] = (self.samples[p] & !1) | bit as u16;
                }
            }
        }
        Ok(())
    }

    /// Lazily read embedded bits in embedding order.
    fn bit_stream(&self, strategy: ImageEmbedStrategy) -> Box<dyn Iterator<Item = u8> + '_> {
        match strategy {
            ImageEmbedStrategy::DCTFrequency => {
                Box::new((0..self.dct_block_count()).flat_map(move |b| {
                    let block: [f64; DCT_BLOCK] =
                        std::array::from_fn(|k| self.samples[self.dct_index(b, k)] as f64);
                    let coeffs = dct8(&block);
                    DCT_COEFF_INDICES.map(|ci| qim_decode(coeffs[ci]))
                }))
            }
            s => Box::new(
                self.lsb_positions(s)
                    .map(move |p| (self.samples[p] & 1) as u8),
            ),
        }
    }

    /// Re-encode the (modified) samples into the original container format.
    fn encode(&self, original: &[u8]) -> Result<Vec<u8>> {
        match &self.container {
            Container::Raw => Ok(self.samples.iter().map(|&s| s as u8).collect()),
            Container::Png { color } => self.encode_png(*color, original),
            Container::Tiff {
                layout,
                compression,
                predictor,
            } => self.encode_tiff(*layout, *compression, *predictor),
        }
    }

    fn encode_png(&self, color: png::ColorType, original: &[u8]) -> Result<Vec<u8>> {
        let perr = |e: png::EncodingError| invalid(format!("PNG encode error: {e}"));
        let data: Vec<u8> = if self.bit_depth == 8 {
            self.samples.iter().map(|&s| s as u8).collect()
        } else {
            self.samples.iter().flat_map(|s| s.to_be_bytes()).collect()
        };
        let mut fresh = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut fresh, self.width as u32, self.height as u32);
            enc.set_color(color);
            enc.set_depth(if self.bit_depth == 8 {
                png::BitDepth::Eight
            } else {
                png::BitDepth::Sixteen
            });
            let mut w = enc.write_header().map_err(perr)?;
            w.write_image_data(&data).map_err(perr)?;
            w.finish().map_err(perr)?;
        }
        splice_png(original, &fresh)
    }

    fn encode_tiff(&self, layout: TiffLayout, compression: u16, predictor: u16) -> Result<Vec<u8>> {
        use tiff::encoder::{colortype, compression::DeflateLevel, Compression, TiffEncoder};
        use tiff::tags::Predictor;
        let terr = |e: tiff::TiffError| invalid(format!("TIFF encode error: {e}"));
        let comp = match compression {
            5 => Compression::Lzw,
            8 | 32946 => Compression::Deflate(DeflateLevel::default()),
            32773 => Compression::Packbits,
            _ => Compression::Uncompressed,
        };
        let pred = if predictor == 2 && compression != 1 {
            Predictor::Horizontal
        } else {
            Predictor::None
        };
        let mut out = Cursor::new(Vec::new());
        {
            let mut enc = TiffEncoder::new(&mut out)
                .map_err(terr)?
                .with_compression(comp)
                .with_predictor(pred);
            let (w, h) = (self.width as u32, self.height as u32);
            if self.bit_depth == 8 {
                let d: Vec<u8> = self.samples.iter().map(|&s| s as u8).collect();
                match layout {
                    TiffLayout::Gray => enc.write_image::<colortype::Gray8>(w, h, &d),
                    TiffLayout::Rgb => enc.write_image::<colortype::RGB8>(w, h, &d),
                    TiffLayout::Rgba => enc.write_image::<colortype::RGBA8>(w, h, &d),
                }
            } else {
                let d = &self.samples;
                match layout {
                    TiffLayout::Gray => enc.write_image::<colortype::Gray16>(w, h, d),
                    TiffLayout::Rgb => enc.write_image::<colortype::RGB16>(w, h, d),
                    TiffLayout::Rgba => enc.write_image::<colortype::RGBA16>(w, h, d),
                }
            }
            .map_err(terr)?;
        }
        Ok(out.into_inner())
    }
}

/// Iterate PNG chunks as `(type, full_chunk_bytes)` (length+type+data+crc).
fn png_chunks(data: &[u8]) -> Result<Vec<([u8; 4], &[u8])>> {
    let mut chunks = Vec::new();
    let mut pos = PNG_SIGNATURE.len();
    while pos < data.len() {
        let hdr_end = pos
            .checked_add(8)
            .filter(|&e| e <= data.len())
            .ok_or_else(|| invalid("Truncated PNG chunk header"))?;
        let len = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        let ty = [data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]];
        let end = usize::try_from(len)
            .ok()
            .and_then(|l| hdr_end.checked_add(l))
            .and_then(|e| e.checked_add(4))
            .filter(|&e| e <= data.len())
            .ok_or_else(|| invalid("Truncated PNG chunk"))?;
        chunks.push((ty, &data[pos..end]));
        pos = end;
        if &ty == b"IEND" {
            break;
        }
    }
    Ok(chunks)
}

/// Build the output PNG: the freshly encoded IHDR + IDAT chunks, with every
/// other chunk of the original copied verbatim in its original position.
fn splice_png(original: &[u8], fresh: &[u8]) -> Result<Vec<u8>> {
    let orig = png_chunks(original)?;
    let new = png_chunks(fresh)?;
    let new_ihdr = new
        .iter()
        .find(|(t, _)| t == b"IHDR")
        .ok_or_else(|| invalid("PNG encoder produced no IHDR"))?;
    let mut out = Vec::with_capacity(original.len() + fresh.len() / 8);
    out.extend_from_slice(&PNG_SIGNATURE);
    out.extend_from_slice(new_ihdr.1);
    let mut idat_written = false;
    for (ty, bytes) in &orig {
        match ty {
            b"IHDR" | b"IEND" => {}
            b"IDAT" => {
                if !idat_written {
                    for (_, nb) in new.iter().filter(|(t, _)| t == b"IDAT") {
                        out.extend_from_slice(nb);
                    }
                    idat_written = true;
                }
            }
            _ => out.extend_from_slice(bytes),
        }
    }
    if !idat_written {
        return Err(invalid("Original PNG has no IDAT chunk"));
    }
    let iend = new
        .iter()
        .find(|(t, _)| t == b"IEND")
        .ok_or_else(|| invalid("PNG encoder produced no IEND"))?;
    out.extend_from_slice(iend.1);
    Ok(out)
}

fn bytes_to_bits(bytes: &[u8]) -> Vec<u8> {
    bytes
        .iter()
        .flat_map(|&byte| (0..8).map(move |i| (byte >> i) & 1))
        .collect()
}

/// Pull `n` bytes (LSB-first bit order) from a bit stream.
fn take_bytes(stream: &mut dyn Iterator<Item = u8>, n: usize) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let mut byte = 0u8;
        for i in 0..8 {
            byte |= stream.next()? << i;
        }
        out.push(byte);
    }
    Some(out)
}

/// Parsed `IMSJ` header.
struct Header {
    payload_len: usize,
    codec_mode: u8,
}

fn parse_strategy(s: &str) -> Result<ImageEmbedStrategy> {
    match s.to_lowercase().as_str() {
        "lsb" | "simple" | "simplelsb" => Ok(ImageEmbedStrategy::SimpleLSB),
        "adaptive" | "adaptivelsb" => Ok(ImageEmbedStrategy::AdaptiveLSB),
        "dct" | "dctfrequency" => Ok(ImageEmbedStrategy::DCTFrequency),
        "edge" | "edgebased" => Ok(ImageEmbedStrategy::EdgeBased),
        other => Err(invalid(format!(
            "Unknown imagery_strategy '{other}' (expected lsb, adaptive, edge or dct)"
        ))),
    }
}

impl ImageryHandler {
    pub fn new() -> Self {
        ImageryHandler
    }

    /// Detect image format from file headers
    fn detect_image_format(&self, data: &[u8]) -> Option<DataFormat> {
        if data.len() < 8 {
            return None;
        }
        if is_png(data) {
            return Some(DataFormat::PNG);
        }
        if is_tiff(data) {
            return Some(DataFormat::TIFF);
        }
        // JPEG / BMP: recognised but not supported for pixel embedding.
        if data.starts_with(&[0xFF, 0xD8, 0xFF]) || data.starts_with(&[0x42, 0x4D]) {
            return Some(DataFormat::BINARY);
        }
        None
    }

    fn capacity_info(img: &DecodedImage, strategy: ImageEmbedStrategy) -> CapacityInfo {
        let capacity_bits = img.capacity_bits(strategy);
        let recommended_bits = capacity_bits.saturating_sub(HEADER_BITS);
        let mut info = CapacityInfo::new(capacity_bits, recommended_bits);
        info.format_specific_info.insert(
            "overhead_bytes".to_string(),
            serde_json::Value::from(HEADER_LEN),
        );
        info.format_specific_info.insert(
            "strategy".to_string(),
            serde_json::Value::String(format!("{strategy:?}")),
        );
        info.format_specific_info.insert(
            "image_layout".to_string(),
            serde_json::Value::String(img.layout_name()),
        );
        info
    }

    /// Exact capacity for one strategy.  `total_bits` counts every carrier
    /// bit; `recommended_bits` excludes the 80-bit header and is the maximum
    /// size (in bits) of the codec-encoded payload.
    pub fn estimate_image_capacity(
        &self,
        carrier: &CarrierData,
        strategy: &ImageEmbedStrategy,
    ) -> Result<CapacityInfo> {
        let img = DecodedImage::decode(carrier)?;
        Ok(Self::capacity_info(&img, *strategy))
    }

    async fn embed_with_strategy(
        &self,
        carrier: &mut CarrierData,
        payload: &[u8],
        params: &EmbedParams,
        strategy: ImageEmbedStrategy,
    ) -> Result<EmbedResult> {
        let embed_start = Instant::now();
        let mut img = DecodedImage::decode(carrier)?;

        let enc_start = Instant::now();
        let encoded = crate::codec::encode_payload(payload, params)?;
        let encryption_time_ms = if params.encryption_key.is_empty() {
            0
        } else {
            (enc_start.elapsed().as_millis() as u64).max(1)
        };

        let payload_len = u32::try_from(encoded.len())
            .map_err(|_| invalid("Payload too large for imagery header (> 4 GiB)"))?;
        let mut total = Vec::with_capacity(HEADER_LEN + encoded.len());
        total.extend_from_slice(MAGIC);
        total.extend_from_slice(&payload_len.to_le_bytes());
        total.push(strategy.to_u8());
        total.push(CODEC_MODE);
        total.extend_from_slice(&encoded);

        let bits_needed = total
            .len()
            .checked_mul(8)
            .ok_or_else(|| invalid("Payload too large"))?;
        let available = img.capacity_bits(strategy);
        if bits_needed > available {
            return Err(SteganoError::InsufficientCapacity {
                needed: bits_needed,
                available,
            });
        }
        img.write_bits(strategy, &bytes_to_bits(&total))?;
        let new_data = img.encode(&carrier.raw_data)?;
        carrier.raw_data = new_data;

        Ok(EmbedResult::success(
            payload.len(),
            PerformanceStats {
                embedding_time_ms: embed_start.elapsed().as_millis() as u64,
                encryption_time_ms,
                gpu_acceleration_used: false,
                memory_usage_bytes: img.samples.len() * 2 + total.len(),
            },
        ))
    }

    /// Try to read a valid header for `strategy`.
    fn read_header(img: &DecodedImage, strategy: ImageEmbedStrategy) -> Option<Header> {
        let mut stream = img.bit_stream(strategy);
        let h = take_bytes(&mut *stream, HEADER_LEN)?;
        if &h[0..4] != MAGIC || h[8] != strategy.to_u8() {
            return None;
        }
        Some(Header {
            payload_len: u32::from_le_bytes([h[4], h[5], h[6], h[7]]) as usize,
            codec_mode: h[9],
        })
    }

    /// Extract with a strategy whose header has already been validated.
    fn extract_payload(
        img: &DecodedImage,
        strategy: ImageEmbedStrategy,
        header: &Header,
        params: &EmbedParams,
    ) -> Result<Vec<u8>> {
        if header.codec_mode != CODEC_MODE {
            return Err(SteganoError::IntegrityError(format!(
                "Unsupported imagery codec mode {} (payloads written by stegano < 0.6 \
                 with mode 0/1 are no longer readable)",
                header.codec_mode
            )));
        }
        let capacity_bytes = img.capacity_bits(strategy) / 8;
        let max_payload = capacity_bytes.saturating_sub(HEADER_LEN);
        if header.payload_len > max_payload {
            return Err(SteganoError::IntegrityError(format!(
                "Embedded length {} exceeds carrier capacity {max_payload} bytes \
                 (truncated or corrupted carrier)",
                header.payload_len
            )));
        }
        let mut stream = img.bit_stream(strategy);
        take_bytes(&mut *stream, HEADER_LEN)
            .ok_or_else(|| SteganoError::IntegrityError("Truncated header".to_string()))?;
        let raw = take_bytes(&mut *stream, header.payload_len).ok_or_else(|| {
            SteganoError::IntegrityError("Carrier ended before the payload".to_string())
        })?;
        crate::codec::decode_payload(&raw, params)
    }
}

#[async_trait]
impl DataHandler for ImageryHandler {
    fn detect_format(&self, data: &[u8]) -> Option<DataFormat> {
        self.detect_image_format(data)
    }

    /// Main value: AdaptiveLSB (Medium stealth).  Exact per-strategy values
    /// (header excluded) are in `capacity_{low,medium,high,maximum}_stealth_bits`.
    fn estimate_capacity(&self, carrier: &CarrierData) -> Result<CapacityInfo> {
        let img = DecodedImage::decode(carrier)?;
        let mut info = Self::capacity_info(&img, ImageEmbedStrategy::AdaptiveLSB);
        for (key, s) in [
            ("capacity_low_stealth_bits", ImageEmbedStrategy::SimpleLSB),
            (
                "capacity_medium_stealth_bits",
                ImageEmbedStrategy::AdaptiveLSB,
            ),
            ("capacity_high_stealth_bits", ImageEmbedStrategy::EdgeBased),
            (
                "capacity_maximum_stealth_bits",
                ImageEmbedStrategy::DCTFrequency,
            ),
        ] {
            let bits = img.capacity_bits(s).saturating_sub(HEADER_BITS);
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
        let strategy = match params
            .format_specific
            .get("imagery_strategy")
            .and_then(|v| v.as_str())
        {
            Some(s) => parse_strategy(s)?,
            None => match params.stealth_level {
                StealthLevel::Low => ImageEmbedStrategy::SimpleLSB,
                StealthLevel::Medium => ImageEmbedStrategy::AdaptiveLSB,
                StealthLevel::High => ImageEmbedStrategy::EdgeBased,
                StealthLevel::Maximum => ImageEmbedStrategy::DCTFrequency,
            },
        };
        self.embed_with_strategy(carrier, payload, params, strategy)
            .await
    }

    async fn extract(&self, carrier: &CarrierData, params: &EmbedParams) -> Result<Vec<u8>> {
        let img = DecodedImage::decode(carrier)?;
        // The first strategy whose header (magic + matching strategy tag)
        // validates is authoritative: its error (e.g. wrong password) is
        // returned rather than a generic "not found".
        for strategy in ImageEmbedStrategy::ALL {
            if let Some(header) = Self::read_header(&img, strategy) {
                return Self::extract_payload(&img, strategy, &header, params);
            }
        }
        Err(SteganoError::IntegrityError(format!(
            "No stegano imagery payload found in {} (no valid IMSJ header for any strategy)",
            img.layout_name()
        )))
    }

    fn verify_integrity(&self, carrier: &CarrierData) -> Result<bool> {
        let Ok(img) = DecodedImage::decode(carrier) else {
            return Ok(false);
        };
        Ok(ImageEmbedStrategy::ALL
            .iter()
            .any(|&s| Self::read_header(&img, s).is_some()))
    }
}

impl Default for ImageryHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // ---- cover generation ----

    #[derive(Debug, Clone, Copy)]
    enum Layout {
        PngRgb8,
        PngRgba8,
        PngGray16,
        TiffRgb8,
    }

    const LAYOUTS: [Layout; 4] = [
        Layout::PngRgb8,
        Layout::PngRgba8,
        Layout::PngGray16,
        Layout::TiffRgb8,
    ];

    /// Deterministic xorshift noise.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
    }

    /// Noise + gradient samples, including saturated (0 / max) regions in the
    /// first rows to exercise DCT clamping.
    fn cover_samples(w: usize, h: usize, ch: usize, depth: u8, alpha: bool) -> Vec<u16> {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        let max = (1u32 << depth) - 1;
        let mut out = Vec::with_capacity(w * h * ch);
        for y in 0..h {
            for x in 0..w {
                for c in 0..ch {
                    if alpha && c == ch - 1 {
                        out.push(max as u16);
                        continue;
                    }
                    let v = if y < 4 {
                        // saturated stripes
                        if (x / 8) % 2 == 0 {
                            0
                        } else {
                            max
                        }
                    } else {
                        let grad = ((x + y + c * 17) as u32 * max) / (w + h + 51) as u32;
                        let noise = (rng.next() % 61) as i64 - 30;
                        let noise = noise * (max as i64 / 255);
                        (grad as i64 + noise).clamp(0, max as i64) as u32
                    };
                    out.push(v as u16);
                }
            }
        }
        out
    }

    fn make_cover(layout: Layout, w: usize, h: usize) -> Vec<u8> {
        match layout {
            Layout::PngRgb8 | Layout::PngRgba8 | Layout::PngGray16 => {
                let (color, ch, depth, alpha) = match layout {
                    Layout::PngRgb8 => (png::ColorType::Rgb, 3, 8, false),
                    Layout::PngRgba8 => (png::ColorType::Rgba, 4, 8, true),
                    _ => (png::ColorType::Grayscale, 1, 16, false),
                };
                let s = cover_samples(w, h, ch, depth, alpha);
                let data: Vec<u8> = if depth == 8 {
                    s.iter().map(|&v| v as u8).collect()
                } else {
                    s.iter().flat_map(|v| v.to_be_bytes()).collect()
                };
                let mut out = Vec::new();
                {
                    let mut enc = png::Encoder::new(&mut out, w as u32, h as u32);
                    enc.set_color(color);
                    enc.set_depth(if depth == 8 {
                        png::BitDepth::Eight
                    } else {
                        png::BitDepth::Sixteen
                    });
                    enc.add_text_chunk("Comment".into(), "stegano cover".into())
                        .unwrap();
                    let mut wr = enc.write_header().unwrap();
                    wr.write_image_data(&data).unwrap();
                    wr.finish().unwrap();
                }
                out
            }
            Layout::TiffRgb8 => {
                let s: Vec<u8> = cover_samples(w, h, 3, 8, false)
                    .iter()
                    .map(|&v| v as u8)
                    .collect();
                let mut out = Cursor::new(Vec::new());
                tiff::encoder::TiffEncoder::new(&mut out)
                    .unwrap()
                    .write_image::<tiff::encoder::colortype::RGB8>(w as u32, h as u32, &s)
                    .unwrap();
                out.into_inner()
            }
        }
    }

    fn carrier_for(layout: Layout, data: Vec<u8>) -> CarrierData {
        let format = match layout {
            Layout::TiffRgb8 => DataFormat::TIFF,
            _ => DataFormat::PNG,
        };
        CarrierData {
            format,
            raw_data: data,
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        }
    }

    /// Independently decode with png/tiff crates → (w, h, descriptor, samples).
    fn independent_decode(data: &[u8]) -> (u32, u32, String, Vec<u16>) {
        if is_png(data) {
            let mut d = png::Decoder::new(Cursor::new(data));
            d.set_transformations(png::Transformations::IDENTITY);
            let mut r = d.read_info().expect("output must be a valid PNG");
            let mut buf = vec![0; r.output_buffer_size().unwrap()];
            r.next_frame(&mut buf).expect("PNG IDAT must decode");
            r.finish().expect("PNG must end cleanly");
            let info = r.info();
            let desc = format!("{:?}/{:?}", info.color_type, info.bit_depth);
            let s = if info.bit_depth == png::BitDepth::Sixteen {
                let (pairs, _) = buf.as_chunks::<2>();
                pairs.iter().map(|&c| u16::from_be_bytes(c)).collect()
            } else {
                buf.iter().map(|&b| b as u16).collect()
            };
            (info.width, info.height, desc, s)
        } else {
            let mut d = tiff::decoder::Decoder::new(Cursor::new(data)).expect("valid TIFF");
            let (w, h) = d.dimensions().unwrap();
            let desc = format!("{:?}", d.colortype().unwrap());
            let s = match d.read_image().unwrap() {
                tiff::decoder::DecodingResult::U8(v) => v.into_iter().map(|b| b as u16).collect(),
                tiff::decoder::DecodingResult::U16(v) => v,
                _ => panic!("unexpected TIFF sample type"),
            };
            (w, h, desc, s)
        }
    }

    fn params(password: Option<&str>, strategy: &str) -> EmbedParams {
        let mut p = EmbedParams::new(password.map(|s| s.as_bytes().to_vec()).unwrap_or_default());
        p.error_correction_level = ECLevel::None;
        p.format_specific.insert(
            "imagery_strategy".to_string(),
            serde_json::Value::String(strategy.to_string()),
        );
        p
    }

    const STRATEGIES: [(&str, ImageEmbedStrategy); 4] = [
        ("lsb", ImageEmbedStrategy::SimpleLSB),
        ("adaptive", ImageEmbedStrategy::AdaptiveLSB),
        ("edge", ImageEmbedStrategy::EdgeBased),
        ("dct", ImageEmbedStrategy::DCTFrequency),
    ];

    /// Every strategy × layout × {no password, password}: output decodes with
    /// the reference crates, keeps dims/colour type, changes samples only
    /// within the documented bound, never touches alpha, and round-trips.
    #[tokio::test]
    async fn test_matrix_roundtrip_valid_output() {
        let handler = ImageryHandler::new();
        let payload = b"pixel-domain steganography payload \x00\x01\xff";
        for layout in LAYOUTS {
            let cover = make_cover(layout, 128, 96);
            let (cw, chh, cdesc, csamples) = independent_decode(&cover);
            let channels = csamples.len() / (cw * chh) as usize;
            let has_alpha = matches!(layout, Layout::PngRgba8);
            for (name, _) in STRATEGIES {
                for pw in [None, Some("pw")] {
                    let p = params(pw, name);
                    let mut carrier = carrier_for(layout, cover.clone());
                    let res = handler
                        .embed(&mut carrier, payload, &p)
                        .await
                        .unwrap_or_else(|e| panic!("{layout:?}/{name}/{pw:?}: embed {e}"));
                    assert_eq!(res.bytes_embedded, payload.len());

                    let (w, h, desc, s) = independent_decode(&carrier.raw_data);
                    assert_eq!((w, h, &desc), (cw, chh, &cdesc), "{layout:?}/{name}");
                    assert_eq!(s.len(), csamples.len());
                    let bound = if name == "dct" {
                        2 * DCT_MAX_SAMPLE_CHANGE as i64
                    } else {
                        1
                    };
                    let mut changed = 0;
                    for (i, (&a, &b)) in csamples.iter().zip(&s).enumerate() {
                        let d = (a as i64 - b as i64).abs();
                        if has_alpha && i % channels == channels - 1 {
                            assert_eq!(d, 0, "alpha modified");
                        }
                        if name != "dct" {
                            assert_eq!(a & !1, b & !1, "{layout:?}/{name}: non-LSB change");
                        } else {
                            // ≤ 2Δ for samples in [2Δ, max−2Δ]; plus the
                            // pre-clamp distance for near-saturated samples.
                            let max = if desc.contains("Sixteen") { 65535 } else { 255 };
                            let m = DCT_MAX_SAMPLE_CHANGE as i64;
                            let pre = (m - a as i64).max(a as i64 - (max - m)).max(0);
                            assert!(d <= m + pre, "dct change {d} > {}", m + pre);
                        }
                        assert!(d <= bound);
                        changed += (d != 0) as usize;
                    }
                    assert!(changed > 0, "{layout:?}/{name}: nothing embedded");

                    let out = handler
                        .extract(&carrier, &p)
                        .await
                        .unwrap_or_else(|e| panic!("{layout:?}/{name}/{pw:?}: extract {e}"));
                    assert_eq!(out, payload, "{layout:?}/{name}/{pw:?}");
                    assert!(handler.verify_integrity(&carrier).unwrap());
                }
            }
        }
    }

    #[tokio::test]
    async fn test_png_ancillary_chunks_preserved() {
        let handler = ImageryHandler::new();
        let cover = make_cover(Layout::PngRgb8, 64, 64);
        let mut carrier = carrier_for(Layout::PngRgb8, cover);
        handler
            .embed(&mut carrier, b"hi", &params(None, "lsb"))
            .await
            .unwrap();
        let mut d = png::Decoder::new(Cursor::new(&carrier.raw_data));
        d.set_transformations(png::Transformations::IDENTITY);
        let r = d.read_info().unwrap();
        assert!(r
            .info()
            .uncompressed_latin1_text
            .iter()
            .any(|t| t.keyword == "Comment" && t.text == "stegano cover"));
    }

    /// estimate_capacity is exact: the largest payload that fits succeeds and
    /// one more byte fails with InsufficientCapacity.
    #[tokio::test]
    async fn test_capacity_is_exact() {
        let handler = ImageryHandler::new();
        let cover = make_cover(Layout::PngRgb8, 48, 40);
        let info = handler
            .estimate_capacity(&carrier_for(Layout::PngRgb8, cover.clone()))
            .unwrap();
        let keys = [
            ("lsb", "capacity_low_stealth_bits"),
            ("adaptive", "capacity_medium_stealth_bits"),
            ("edge", "capacity_high_stealth_bits"),
            ("dct", "capacity_maximum_stealth_bits"),
        ];
        assert_eq!(
            info.recommended_bits,
            info.format_specific_info["capacity_medium_stealth_bits"]
                .as_u64()
                .unwrap() as usize
        );
        for (name, key) in keys {
            let p = params(None, name);
            let bits = info.format_specific_info[key].as_u64().unwrap() as usize;
            let overhead = crate::codec::encode_payload(b"", &p).unwrap().len();
            let max_payload = bits / 8 - overhead;
            let mut c = carrier_for(Layout::PngRgb8, cover.clone());
            let payload = vec![0x5Au8; max_payload];
            handler.embed(&mut c, &payload, &p).await.unwrap();
            assert_eq!(handler.extract(&c, &p).await.unwrap(), payload, "{name}");
            let mut c = carrier_for(Layout::PngRgb8, cover.clone());
            let too_big = vec![0x5Au8; max_payload + 1];
            assert!(
                matches!(
                    handler.embed(&mut c, &too_big, &p).await,
                    Err(SteganoError::InsufficientCapacity { .. })
                ),
                "{name}: over-capacity must error"
            );
        }
    }

    #[tokio::test]
    async fn test_wrong_password_surfaces_crypto_error() {
        let handler = ImageryHandler::new();
        let mut carrier = carrier_for(Layout::PngRgb8, make_cover(Layout::PngRgb8, 64, 64));
        handler
            .embed(&mut carrier, b"secret", &params(Some("right"), "adaptive"))
            .await
            .unwrap();
        let err = handler
            .extract(&carrier, &params(Some("wrong"), "adaptive"))
            .await
            .unwrap_err();
        assert!(
            !err.to_string().contains("No stegano imagery payload"),
            "expected decrypt error, got {err}"
        );
    }

    #[tokio::test]
    async fn test_no_payload_and_verify_false() {
        let handler = ImageryHandler::new();
        let carrier = carrier_for(Layout::PngRgb8, make_cover(Layout::PngRgb8, 64, 64));
        assert!(!handler.verify_integrity(&carrier).unwrap());
        assert!(handler
            .extract(&carrier, &params(None, "lsb"))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_truncated_carrier_errors() {
        let handler = ImageryHandler::new();
        let mut carrier = carrier_for(Layout::PngRgb8, make_cover(Layout::PngRgb8, 64, 64));
        handler
            .embed(&mut carrier, b"truncate me", &params(None, "lsb"))
            .await
            .unwrap();
        for cut in [carrier.raw_data.len() / 2, 40, 7] {
            let mut t = carrier.clone();
            t.raw_data.truncate(cut);
            assert!(handler.extract(&t, &params(None, "lsb")).await.is_err());
            assert!(!handler.verify_integrity(&t).unwrap());
        }
    }

    /// A header claiming a ~4 GiB payload must be rejected before allocation.
    #[tokio::test]
    async fn test_crafted_huge_length_errors() {
        let handler = ImageryHandler::new();
        let cover = make_cover(Layout::PngRgb8, 32, 32);
        let mut img = DecodedImage::decode(&carrier_for(Layout::PngRgb8, cover.clone())).unwrap();
        let mut hdr = Vec::new();
        hdr.extend_from_slice(MAGIC);
        hdr.extend_from_slice(&u32::MAX.to_le_bytes());
        hdr.push(ImageEmbedStrategy::SimpleLSB.to_u8());
        hdr.push(CODEC_MODE);
        img.write_bits(ImageEmbedStrategy::SimpleLSB, &bytes_to_bits(&hdr))
            .unwrap();
        let mut carrier = carrier_for(Layout::PngRgb8, img.encode(&cover).unwrap());
        carrier.format = DataFormat::PNG;
        let err = handler
            .extract(&carrier, &params(None, "lsb"))
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("exceeds carrier capacity"),
            "{err}"
        );
    }

    #[tokio::test]
    async fn test_rejects_palette_png_and_non_image() {
        let handler = ImageryHandler::new();
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, 16, 16);
            enc.set_color(png::ColorType::Indexed);
            enc.set_depth(png::BitDepth::Eight);
            enc.set_palette(vec![0u8, 0, 0, 255, 255, 255]);
            let mut w = enc.write_header().unwrap();
            w.write_image_data(&[0u8; 256]).unwrap();
            w.finish().unwrap();
        }
        let mut c = carrier_for(Layout::PngRgb8, out);
        let err = handler
            .embed(&mut c, b"x", &params(None, "lsb"))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Palette"), "{err}");

        let mut fake = CarrierData::new(DataFormat::PNG, vec![0xAA; 10_000]);
        assert!(handler
            .embed(&mut fake, b"x", &params(None, "lsb"))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_binary_raw_mode_roundtrip() {
        let handler = ImageryHandler::new();
        let mut c = CarrierData::new(DataFormat::BINARY, vec![0x80; 4096]);
        let p = params(Some("k"), "lsb");
        handler.embed(&mut c, b"raw", &p).await.unwrap();
        assert_eq!(handler.extract(&c, &p).await.unwrap(), b"raw");
    }

    #[tokio::test]
    async fn test_unknown_strategy_errors() {
        let handler = ImageryHandler::new();
        let mut c = carrier_for(Layout::PngRgb8, make_cover(Layout::PngRgb8, 32, 32));
        assert!(handler
            .embed(&mut c, b"x", &params(None, "pvd"))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_stealth_level_mapping() {
        let handler = ImageryHandler::new();
        let cover = make_cover(Layout::PngRgb8, 64, 64);
        for (level, s) in [
            (StealthLevel::Low, ImageEmbedStrategy::SimpleLSB),
            (StealthLevel::Medium, ImageEmbedStrategy::AdaptiveLSB),
            (StealthLevel::High, ImageEmbedStrategy::EdgeBased),
            (StealthLevel::Maximum, ImageEmbedStrategy::DCTFrequency),
        ] {
            let mut p = EmbedParams::new(b"key".to_vec());
            p.stealth_level = level;
            let mut c = carrier_for(Layout::PngRgb8, cover.clone());
            handler.embed(&mut c, b"level", &p).await.unwrap();
            let img = DecodedImage::decode(&c).unwrap();
            assert!(ImageryHandler::read_header(&img, s).is_some(), "{level:?}");
            assert_eq!(handler.extract(&c, &p).await.unwrap(), b"level");
        }
    }

    #[test]
    fn test_image_format_detection() {
        let handler = ImageryHandler::new();
        let png_data = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00];
        assert_eq!(handler.detect_format(&png_data), Some(DataFormat::PNG));
        let jpeg_data = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46];
        assert_eq!(handler.detect_format(&jpeg_data), Some(DataFormat::BINARY));
        let tiff_data = [0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00];
        assert_eq!(handler.detect_format(&tiff_data), Some(DataFormat::TIFF));
        assert_eq!(handler.detect_format(&[0, 1, 2, 3]), None);
    }

    #[test]
    fn test_capacity_ordering_and_formula() {
        let handler = ImageryHandler::new();
        let carrier = carrier_for(Layout::PngRgb8, make_cover(Layout::PngRgb8, 64, 48));
        let simple = handler
            .estimate_image_capacity(&carrier, &ImageEmbedStrategy::SimpleLSB)
            .unwrap();
        assert_eq!(simple.total_bits, 64 * 48 * 3);
        let dct = handler
            .estimate_image_capacity(&carrier, &ImageEmbedStrategy::DCTFrequency)
            .unwrap();
        assert_eq!(dct.total_bits, 48 * 3 * (64 / 8) * 4);
        for s in [
            ImageEmbedStrategy::AdaptiveLSB,
            ImageEmbedStrategy::EdgeBased,
        ] {
            let c = handler.estimate_image_capacity(&carrier, &s).unwrap();
            assert!(
                c.total_bits > 0 && c.total_bits <= simple.total_bits,
                "{s:?}"
            );
        }
    }

    // ---- DCT unit tests ----

    #[test]
    fn test_dct8_idct8_roundtrip() {
        let original: [f64; 8] = [100.0, 120.0, 80.0, 90.0, 110.0, 70.0, 95.0, 105.0];
        let restored = idct8(&dct8(&original));
        for i in 0..8 {
            assert!((restored[i] - original[i]).abs() < 1e-9);
        }
    }

    #[test]
    fn test_qim_saturated_blocks() {
        // Fully saturated and full-swing blocks used to break QIM via clamping.
        for fill in [[0u16; 8], [255u16; 8], [0, 255, 0, 255, 0, 255, 0, 255]] {
            let samples: Vec<u16> = fill.iter().cycle().take(64).cloned().collect();
            let mut img = DecodedImage {
                width: 64,
                height: 1,
                channels: 1,
                color_channels: 1,
                bit_depth: 8,
                samples,
                container: Container::Raw,
            };
            let bits: Vec<u8> = (0..32).map(|i| ((i * 7 + 3) % 5 % 2) as u8).collect();
            img.write_bits(ImageEmbedStrategy::DCTFrequency, &bits)
                .unwrap();
            let got: Vec<u8> = img
                .bit_stream(ImageEmbedStrategy::DCTFrequency)
                .take(32)
                .collect();
            assert_eq!(got, bits, "fill {fill:?}");
        }
    }
}
