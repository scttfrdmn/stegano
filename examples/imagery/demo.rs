//! Shows embedding into a PNG image with the imagery handler; the output stays a valid PNG.

use stegano::carrier::carrier_from_bytes;
use stegano::*;

/// Encode a noisy RGB gradient as a PNG file in memory.
fn synthetic_png(width: u32, height: u32) -> Vec<u8> {
    // A gradient plus sensor-like noise: adaptive LSB embeds only in textured
    // regions, so a perfectly smooth image would have almost no capacity.
    let mut state: u32 = 7;
    let mut noise = || {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (state >> 27) as u8 // 0..=31
    };
    let mut pixels = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        for x in 0..width {
            pixels.push((x * 200 / width) as u8 + noise());
            pixels.push((y * 200 / height) as u8 + noise());
            pixels.push(((x + y) * 100 / (width + height)) as u8 + 64 + noise());
        }
    }
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("png header");
        writer.write_image_data(&pixels).expect("png data");
    }
    out
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    println!("Stegano imagery demo (PNG carrier)\n");

    // carrier_from_bytes also parses width/height from the PNG header.
    let mut carrier = carrier_from_bytes(DataFormat::PNG, synthetic_png(256, 256));
    let dims = carrier.dimensions.clone().expect("PNG dimensions");
    println!(
        "Image: {}x{} PNG, {} bytes",
        dims.width,
        dims.height,
        carrier.raw_data.len()
    );

    let secret = b"Microscopy run 2026-03-14, slide 7, operator AB";
    let params = EmbedParams::new(b"imagery_key".to_vec());

    let embedder = SteganoEmbedder::new();
    let capacity = embedder.estimate_capacity(&carrier)?;
    println!(
        "Recommended payload: {} bytes",
        capacity.recommended_bits / 8
    );

    let result = embedder.embed(&mut carrier, secret, &params).await?;
    println!("Embedded {} bytes", result.bytes_embedded);

    // The carrier must still decode as a PNG of the same size.
    let decoder = png::Decoder::new(std::io::Cursor::new(&carrier.raw_data));
    let reader = decoder
        .read_info()
        .map_err(|e| SteganoError::IntegrityError(format!("output is not a PNG: {e}")))?;
    let info = reader.info();
    println!("Output decodes as {}x{} PNG", info.width, info.height);

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
