//! FITS and WAV stream handlers: byte-identical interop with the in-memory
//! handlers, error parity, and bounded reads.

use super::*;
use crate::codec::encode_payload;
use crate::core::{CarrierData, ECLevel, StealthLevel};
use crate::handlers::fits::tests::{
    card, ext_header, float_data, int_card, int_data, multi_hdu, padded, primary_header,
    simple_image,
};
use crate::handlers::fits::FitsHandler;
use crate::handlers::signals::SignalHandler;
use crate::handlers::DataHandler;
use std::io::Cursor;

const NOT_STREAMABLE: &str = "is not streamable; use the in-memory API";

fn params(password: &[u8], key: &str, strategy: Option<&str>) -> EmbedParams {
    let mut p = EmbedParams {
        encryption_key: password.to_vec(),
        error_correction_level: ECLevel::None,
        stealth_level: StealthLevel::Medium,
        ..Default::default()
    };
    if let Some(s) = strategy {
        p.format_specific
            .insert(key.to_string(), serde_json::Value::from(s));
    }
    p
}

fn carrier(format: DataFormat, raw: Vec<u8>) -> CarrierData {
    CarrierData {
        format,
        raw_data: raw,
        metadata: Default::default(),
        dimensions: None,
        sampling_rate: None,
    }
}

fn stream_embed(format: DataFormat, data: &[u8], encoded: &[u8], p: &EmbedParams) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    embed_encoded_stream(&format, &mut Cursor::new(data), &mut out, encoded, p)?;
    Ok(out)
}

fn payloads() -> Vec<Vec<u8>> {
    [0usize, 1, 37, 120]
        .iter()
        .map(|&n| (0..n as u32).map(|i| (i * 31 % 251) as u8).collect())
        .collect()
}

// ---------------------------------------------------------------------------
// FITS
// ---------------------------------------------------------------------------

const FITS_KEY: &str = "fits_strategy";

/// (name, file, strategy) FITS carriers.
fn fits_carriers() -> Vec<(&'static str, Vec<u8>, Option<&'static str>)> {
    let mut v = vec![("multi-hdu", multi_hdu().0, None)];
    for bitpix in [8i64, 16, 32, 64] {
        v.push(("int", simple_image(bitpix, &[60, 40], int_data(bitpix, 2400)).0, None));
    }
    for bitpix in [-32i64, -64] {
        v.push((
            "float",
            simple_image(bitpix, &[3000], float_data(bitpix, 3000)).0,
            Some("float_mantissa"),
        ));
    }
    // BLANK skipping.
    let n = 3000;
    let mut data = int_data(16, n);
    for i in (0..n).step_by(5) {
        data[2 * i..2 * i + 2].copy_from_slice(&(-32768i16 + (i % 2) as i16).to_be_bytes());
    }
    let mut blank = primary_header(16, &[n], &[int_card("BLANK", -32768)]);
    blank.extend(padded(data));
    v.push(("blank", blank, None));
    // Float image first, int image second; trailing special records.
    let mut mixed = primary_header(-32, &[1500], &[]);
    mixed.extend(padded(float_data(-32, 1500)));
    mixed.extend(ext_header("IMAGE", 16, &[2000], &[card("EXTNAME = 'SCI     '")]));
    mixed.extend(padded(int_data(16, 2000)));
    mixed.extend(vec![0x5Au8; 2880]);
    v.push(("float-then-int", mixed.clone(), None));
    v.push(("float-then-int/float", mixed, Some("float_mantissa")));
    // Last HDU's padding truncated at EOF.
    let (mut short, ..) = simple_image(16, &[2000], int_data(16, 2000));
    short.truncate(short.len() - 100);
    v.push(("short-padding", short, None));
    v
}

#[test]
fn test_fits_interop_byte_identical_with_in_memory() {
    for (name, orig, strategy) in fits_carriers() {
        for pw in [&b""[..], b"pw"] {
            let p = params(pw, FITS_KEY, strategy);
            for payload in payloads() {
                let encoded = encode_payload(&payload, &p).unwrap();
                let mut mem = orig.clone();
                if let Err(e) = FitsHandler::embed_encoded(&mut mem, &encoded, &p) {
                    let st = stream_embed(DataFormat::FITS, &orig, &encoded, &p).unwrap_err();
                    assert_eq!(st.to_string(), e.to_string(), "{name}");
                    continue;
                }
                let streamed = stream_embed(DataFormat::FITS, &orig, &encoded, &p).unwrap();
                assert_eq!(streamed.len(), orig.len(), "{name}");
                assert!(streamed == mem, "{name}/pw={}/n={}", !pw.is_empty(), payload.len());
                assert_ne!(streamed, orig);
                // Stream extract returns the same codec bytes.
                let raw = extract_encoded_stream(&DataFormat::FITS, &mut Cursor::new(&mem), &p)
                    .unwrap();
                assert_eq!(raw, encoded, "{name}");
            }
        }
    }
}

#[tokio::test]
async fn test_fits_stream_and_in_memory_interoperate() {
    let handler = FitsHandler::new();
    for (name, orig, strategy) in fits_carriers() {
        for pw in [&b""[..], b"hunter2"] {
            let p = params(pw, FITS_KEY, strategy);
            let secret = format!("FITS secret in {name}").into_bytes();

            // stream embed -> in-memory extract
            let mut out = Vec::new();
            embed_stream(DataFormat::FITS, Cursor::new(&orig), &mut out, &secret, &p).unwrap();
            let c = carrier(DataFormat::FITS, out.clone());
            assert_eq!(handler.extract(&c, &p).await.unwrap(), secret, "{name}");
            assert!(handler.verify_integrity(&c).unwrap());

            // in-memory embed -> stream extract
            let mut c = carrier(DataFormat::FITS, orig.clone());
            handler.embed(&mut c, &secret, &p).await.unwrap();
            let got = extract_stream(DataFormat::FITS, Cursor::new(&c.raw_data), &p).unwrap();
            assert_eq!(got, secret, "{name}");

            if !pw.is_empty() {
                let wrong = params(b"wrong", FITS_KEY, strategy);
                assert!(extract_stream(DataFormat::FITS, Cursor::new(&out), &wrong).is_err());
            }
        }
    }
}

#[test]
fn test_fits_multi_hdu_only_image_lsbs_change() {
    let (orig, img_start, img_len) = multi_hdu();
    let p = params(b"", FITS_KEY, None);
    let mut out = Vec::new();
    embed_stream(DataFormat::FITS, Cursor::new(&orig), &mut out, b"Andromeda", &p).unwrap();
    assert_eq!(out[..img_start], orig[..img_start]);
    assert_eq!(out[img_start + img_len..], orig[img_start + img_len..]);
    assert_ne!(out, orig);
}

#[test]
fn test_fits_errors_match_in_memory() {
    let p = params(b"", FITS_KEY, None);
    let pf = params(b"", FITS_KEY, Some("float_mantissa"));
    let encoded = encode_payload(&[7u8; 64], &p).unwrap();

    // Float data with the default strategy, int data with float_mantissa,
    // no pixel data, insufficient capacity.
    let float = simple_image(-32, &[3000], float_data(-32, 3000)).0;
    let int = simple_image(16, &[3000], int_data(16, 3000)).0;
    let small = simple_image(16, &[300], int_data(16, 300)).0;
    let empty = primary_header(8, &[], &[]);
    for (file, p) in [(&float, &p), (&int, &pf), (&empty, &p), (&small, &p)] {
        let mem = FitsHandler::embed_encoded(&mut file.clone(), &encoded, p).unwrap_err();
        let st = stream_embed(DataFormat::FITS, file, &encoded, p).unwrap_err();
        assert_eq!(st.to_string(), mem.to_string());
    }

    // Malformed / truncated input.
    let (multi, img_start, _) = multi_hdu();
    for cut in [0, 5, 100, 2880 + 7, img_start + 10] {
        let t = &multi[..cut];
        assert!(FitsHandler::embed_encoded(&mut t.to_vec(), &encoded, &p).is_err());
        assert!(stream_embed(DataFormat::FITS, t, &encoded, &p).is_err(), "cut {cut}");
        assert!(extract_stream(DataFormat::FITS, Cursor::new(t), &p).is_err());
    }
    assert!(stream_embed(DataFormat::FITS, &[0xFF; 6000], &encoded, &p).is_err());
    // No payload.
    let err = extract_stream(DataFormat::FITS, Cursor::new(&int), &p).unwrap_err();
    assert!(err.to_string().contains("No FITS"), "{err}");
    // Unknown strategy is rejected up front.
    assert!(!is_streamable(&DataFormat::FITS, &params(b"", FITS_KEY, Some("bogus"))));
    assert!(is_streamable(&DataFormat::FITS, &p));
    assert!(is_streamable(&DataFormat::FITS, &pf));
}

#[test]
fn test_fits_extract_reads_only_a_prefix() {
    let n = 2_000_000;
    let (orig, start) = simple_image(16, &[n], int_data(16, n));
    let p = params(b"", FITS_KEY, None);
    let mut out = Vec::new();
    let stats = {
        let encoded = encode_payload(b"tiny", &p).unwrap();
        embed_encoded_stream(&DataFormat::FITS, &mut Cursor::new(&orig), &mut out, &encoded, &p)
            .unwrap()
    };
    assert_eq!(stats.bytes_read, orig.len() as u64);
    assert_eq!(stats.bytes_written, orig.len() as u64);
    assert!(stats.peak_buffer_bytes < 256 * 1024, "{}", stats.peak_buffer_bytes);
    let (raw, read) = fits::extract(&mut Cursor::new(&out)).unwrap();
    assert!(!raw.is_empty());
    assert!(read <= (start + fits::CHUNK) as u64, "{read}");
}

// ---------------------------------------------------------------------------
// WAV
// ---------------------------------------------------------------------------

const WAV_KEY: &str = "signal_strategy";

#[derive(Clone, Copy)]
struct WavSpec {
    bits: u16,
    channels: u16,
    float: bool,
    /// WAVE_FORMAT_EXTENSIBLE with this many valid bits.
    valid_bits: Option<u16>,
}

fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut c = id.to_vec();
    c.extend_from_slice(&(body.len() as u32).to_le_bytes());
    c.extend_from_slice(body);
    if body.len() % 2 == 1 {
        c.push(0);
    }
    c
}

fn fmt_body(s: WavSpec) -> Vec<u8> {
    let tag: u16 = if s.float { 3 } else { 1 };
    let block = s.channels * s.bits / 8;
    let mut f = Vec::new();
    f.extend_from_slice(&(if s.valid_bits.is_some() { 0xFFFE } else { tag }).to_le_bytes());
    f.extend_from_slice(&s.channels.to_le_bytes());
    f.extend_from_slice(&44_100u32.to_le_bytes());
    f.extend_from_slice(&(44_100 * block as u32).to_le_bytes());
    f.extend_from_slice(&block.to_le_bytes());
    f.extend_from_slice(&s.bits.to_le_bytes());
    if let Some(vb) = s.valid_bits {
        f.extend_from_slice(&22u16.to_le_bytes());
        f.extend_from_slice(&vb.to_le_bytes());
        f.extend_from_slice(&3u32.to_le_bytes());
        f.extend_from_slice(&tag.to_le_bytes());
        f.extend_from_slice(&[
            0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71,
        ]);
    }
    f
}

/// Deterministic sample bytes for `frames` frames (+ `extra` stray bytes).
fn samples(s: WavSpec, frames: usize, extra: usize) -> Vec<u8> {
    let n = frames * s.channels as usize;
    let mut x: u64 = 0x2545_F491_4F6C_DD1D;
    let mut out = Vec::new();
    for i in 0..n {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        if s.float {
            let v = ((i as f64 * 0.01).sin() * 0.5) as f32;
            if s.bits == 32 {
                out.extend_from_slice(&v.to_le_bytes());
            } else {
                out.extend_from_slice(&(v as f64).to_le_bytes());
            }
        } else {
            out.extend_from_slice(&x.to_le_bytes()[..s.bits as usize / 8]);
        }
    }
    out.extend(std::iter::repeat_n(0xEEu8, extra));
    out
}

/// RIFF/WAVE file: `fmt `, optional LIST before and after `data`.
fn wav(s: WavSpec, frames: usize, list_before: bool, list_after: bool, extra: usize) -> Vec<u8> {
    let mut body = b"WAVE".to_vec();
    body.extend(chunk(b"fmt ", &fmt_body(s)));
    if list_before {
        body.extend(chunk(b"LIST", b"INFOINAM\x05\x00\x00\x00Song\x00"));
    }
    body.extend(chunk(b"data", &samples(s, frames, extra)));
    if list_after {
        body.extend(chunk(b"LIST", b"INFOICMT\x07\x00\x00\x00comment"));
    }
    let mut f = b"RIFF".to_vec();
    f.extend_from_slice(&(body.len() as u32).to_le_bytes());
    f.extend(body);
    f
}

fn int(bits: u16, channels: u16) -> WavSpec {
    WavSpec {
        bits,
        channels,
        float: false,
        valid_bits: None,
    }
}

fn wav_carriers() -> Vec<(&'static str, Vec<u8>)> {
    let ext = WavSpec {
        valid_bits: Some(24),
        ..int(32, 2)
    };
    let f32s = WavSpec {
        float: true,
        ..int(32, 1)
    };
    vec![
        ("16-mono", wav(int(16, 1), 4000, false, false, 0)),
        ("16-mono-lists", wav(int(16, 1), 4000, true, true, 0)),
        ("16-stereo-lists", wav(int(16, 2), 3000, true, true, 0)),
        ("24-stereo-lists", wav(int(24, 2), 3000, true, true, 0)),
        ("24-mono-partial-frame", wav(int(24, 1), 3001, true, false, 2)),
        ("8-mono", wav(int(8, 1), 5000, false, true, 0)),
        ("24in32-ext", wav(ext, 3000, true, true, 0)),
        ("f32-mono", wav(f32s, 3000, false, true, 0)),
    ]
}

fn td(pw: &[u8]) -> EmbedParams {
    params(pw, WAV_KEY, Some("timedomain"))
}

#[test]
fn test_wav_interop_byte_identical_with_in_memory() {
    let low = EmbedParams {
        stealth_level: StealthLevel::Low,
        ..params(b"", WAV_KEY, None)
    };
    for (name, orig) in wav_carriers() {
        for p in [td(b""), td(b"pw"), low.clone()] {
            for payload in payloads() {
                let encoded = encode_payload(&payload, &p).unwrap();
                let mem = SignalHandler::embed_encoded_wav(&orig, &encoded, &p).unwrap();
                let streamed = stream_embed(DataFormat::WAV, &orig, &encoded, &p).unwrap();
                assert!(streamed == mem, "{name}/n={}", payload.len());
                assert_eq!(streamed.len(), orig.len());
                assert_eq!(streamed[..44], orig[..44], "{name}: RIFF/fmt header changed");
                let raw = extract_encoded_stream(&DataFormat::WAV, &mut Cursor::new(&mem), &p)
                    .unwrap();
                assert_eq!(raw, encoded, "{name}");
            }
        }
    }
}

#[tokio::test]
async fn test_wav_stream_and_in_memory_interoperate() {
    let handler = SignalHandler::new();
    for (name, orig) in wav_carriers() {
        for pw in [&b""[..], b"hunter2"] {
            let p = td(pw);
            let secret = format!("audio secret in {name}").into_bytes();

            let mut out = Vec::new();
            embed_stream(DataFormat::WAV, Cursor::new(&orig), &mut out, &secret, &p).unwrap();
            let c = carrier(DataFormat::WAV, out.clone());
            assert_eq!(handler.extract(&c, &p).await.unwrap(), secret, "{name}");

            let mut c = carrier(DataFormat::WAV, orig.clone());
            handler.embed(&mut c, &secret, &p).await.unwrap();
            let got = extract_stream(DataFormat::WAV, Cursor::new(&c.raw_data), &p).unwrap();
            assert_eq!(got, secret, "{name}");

            if !pw.is_empty() {
                assert!(extract_stream(DataFormat::WAV, Cursor::new(&out), &td(b"no")).is_err());
            }
        }
    }
}

#[tokio::test]
async fn test_wav_only_timedomain_streams() {
    assert!(is_streamable(&DataFormat::WAV, &td(b"")));
    for s in ["frequency", "phase", "spread", "echo"] {
        assert!(!is_streamable(&DataFormat::WAV, &params(b"", WAV_KEY, Some(s))), "{s}");
    }
    // Medium stealth without signal_strategy is FrequencyDomain.
    assert!(!is_streamable(&DataFormat::WAV, &params(b"", WAV_KEY, None)));
    assert!(!is_streamable(&DataFormat::WAV, &params(b"", WAV_KEY, Some("bogus"))));

    // A phase-embedded file: stream extraction says "not streamable" (so
    // the CLI falls back), the in-memory extractor finds it.
    let orig = wav(int(16, 1), 44_100 * 2, true, true, 0);
    let p = params(b"", WAV_KEY, Some("phase"));
    let mut c = carrier(DataFormat::WAV, orig);
    SignalHandler::new().embed(&mut c, b"phase", &p).await.unwrap();
    let err = extract_stream(DataFormat::WAV, Cursor::new(&c.raw_data), &p).unwrap_err();
    assert!(err.to_string().contains(NOT_STREAMABLE), "{err}");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("phase.wav");
    std::fs::write(&path, &c.raw_data).unwrap();
    assert!(crate::cli::files::try_extract_stream(&DataFormat::WAV, &path, &p).is_none());
}

#[test]
fn test_wav_errors_match_in_memory() {
    let p = td(b"");
    let encoded = encode_payload(&[3u8; 64], &p).unwrap();

    // Insufficient capacity: same numbers.
    let small = wav(int(16, 1), 300, true, true, 0);
    let mem = SignalHandler::embed_encoded_wav(&small, &encoded, &p).unwrap_err();
    let st = stream_embed(DataFormat::WAV, &small, &encoded, &p).unwrap_err();
    assert_eq!(st.to_string(), mem.to_string());
    assert!(matches!(st, SteganoError::InsufficientCapacity { .. }));

    // NaN in a float carrier, even far past the payload.
    let f32s = WavSpec {
        float: true,
        ..int(32, 1)
    };
    let mut nan = wav(f32s, 100_000, false, false, 0);
    let at = nan.len() - 40;
    nan[at..at + 4].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(SignalHandler::embed_encoded_wav(&nan, &encoded, &p).is_err());
    assert!(stream_embed(DataFormat::WAV, &nan, &encoded, &p).is_err());

    // Truncated / malformed.
    let good = wav(int(16, 2), 3000, true, true, 0);
    for cut in [0, 11, 20, 40, 60] {
        let t = &good[..cut];
        assert!(SignalHandler::embed_encoded_wav(t, &encoded, &p).is_err());
        assert!(stream_embed(DataFormat::WAV, t, &encoded, &p).is_err(), "cut {cut}");
    }
    // A data chunk truncated at EOF is clamped, as in memory.
    let cut = &good[..good.len() - 30];
    assert_eq!(
        stream_embed(DataFormat::WAV, cut, &encoded, &p).unwrap(),
        SignalHandler::embed_encoded_wav(cut, &encoded, &p).unwrap()
    );
    // data before fmt: in-memory only.
    let mut body = b"WAVE".to_vec();
    body.extend(chunk(b"data", &samples(int(16, 1), 3000, 0)));
    body.extend(chunk(b"fmt ", &fmt_body(int(16, 1))));
    let mut swapped = b"RIFF".to_vec();
    swapped.extend_from_slice(&(body.len() as u32).to_le_bytes());
    swapped.extend(body);
    assert!(SignalHandler::embed_encoded_wav(&swapped, &encoded, &p).is_ok());
    let err = stream_embed(DataFormat::WAV, &swapped, &encoded, &p).unwrap_err();
    assert!(err.to_string().contains(NOT_STREAMABLE), "{err}");
}

#[test]
fn test_wav_extract_reads_only_a_prefix() {
    let orig = wav(int(16, 2), 2_000_000, true, true, 0);
    let p = td(b"");
    let encoded = encode_payload(b"tiny", &p).unwrap();
    let mut out = Vec::new();
    let stats =
        embed_encoded_stream(&DataFormat::WAV, &mut Cursor::new(&orig), &mut out, &encoded, &p)
            .unwrap();
    assert_eq!(stats.bytes_read, orig.len() as u64);
    assert_eq!(stats.bytes_written, orig.len() as u64);
    assert!(stats.peak_buffer_bytes < 512 * 1024, "{}", stats.peak_buffer_bytes);
    let (raw, read) = wav::extract(&mut Cursor::new(&out)).unwrap();
    assert_eq!(raw, encoded);
    assert!(read <= 512 * 1024, "{read}");
}
