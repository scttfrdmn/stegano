//! Shows embedding into a 16-bit WAV recording with the signals handler.

use stegano::carrier::carrier_from_bytes;
use stegano::*;

/// One second of a 440 Hz sine tone, 16-bit mono at 44.1 kHz, as WAV bytes.
fn synthetic_wav() -> Vec<u8> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44_100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(&mut cursor, spec).expect("wav header");
        for n in 0..spec.sample_rate {
            let t = n as f32 / spec.sample_rate as f32;
            let sample = (t * 440.0 * std::f32::consts::TAU).sin() * 0.5 * i16::MAX as f32;
            writer.write_sample(sample as i16).expect("wav sample");
        }
        writer.finalize().expect("wav finalize");
    }
    cursor.into_inner()
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    println!("Stegano signals demo (WAV carrier)\n");

    // carrier_from_bytes also reads the sample rate from the WAV header.
    let mut carrier = carrier_from_bytes(DataFormat::WAV, synthetic_wav());
    println!(
        "WAV: {} bytes, sample rate {:?} Hz",
        carrier.raw_data.len(),
        carrier.sampling_rate
    );

    let secret = b"Sensor array 3, calibration offset +0.012 V";
    let params = EmbedParams::new(b"signals_key".to_vec());

    let embedder = SteganoEmbedder::new();
    let capacity = embedder.estimate_capacity(&carrier)?;
    println!(
        "Recommended payload: {} bytes",
        capacity.recommended_bits / 8
    );

    let result = embedder.embed(&mut carrier, secret, &params).await?;
    println!("Embedded {} bytes", result.bytes_embedded);

    let extracted = embedder.extract(&carrier, &params).await?;
    println!("Extracted: '{}'", String::from_utf8_lossy(&extracted));

    if extracted != secret {
        return Err(SteganoError::IntegrityError(
            "round-trip mismatch".to_string(),
        ));
    }
    println!("\nRound trip OK");
    Ok(())
}
