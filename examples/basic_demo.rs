//! Shows the basic password-protected embed/extract round trip on a raw binary carrier.

use stegano::*;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    println!("Stegano basic demo (binary carrier)\n");

    // 16 KiB of pseudo-random bytes stands in for an arbitrary binary file.
    let mut state: u32 = 0x1234_5678;
    let raw: Vec<u8> = (0..16 * 1024)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 24) as u8
        })
        .collect();
    let original = raw.clone();
    let mut carrier = CarrierData::new(DataFormat::BINARY, raw);

    let secret = b"This is confidential research data that needs to be hidden!";
    // The password is used for authenticated encryption (Argon2id + AES-256-GCM).
    let params = EmbedParams::new(b"my_secret_password".to_vec());

    let embedder = SteganoEmbedder::new();
    let capacity = embedder.estimate_capacity(&carrier)?;
    println!("Carrier size:        {} bytes", carrier.raw_data.len());
    println!(
        "Recommended payload: {} bytes",
        capacity.recommended_bits / 8
    );
    println!("Secret size:         {} bytes", secret.len());

    let result = embedder.embed(&mut carrier, secret, &params).await?;
    let changed = original
        .iter()
        .zip(&carrier.raw_data)
        .filter(|(a, b)| a != b)
        .count();
    println!(
        "\nEmbedded {} bytes; {} carrier bytes changed",
        result.bytes_embedded, changed
    );

    let extracted = embedder.extract(&carrier, &params).await?;
    println!("Extracted: '{}'", String::from_utf8_lossy(&extracted));

    if extracted != secret {
        return Err(SteganoError::IntegrityError(
            "round-trip mismatch".to_string(),
        ));
    }
    println!("\nRound trip OK");

    // A wrong password must not recover the payload.
    let wrong = EmbedParams::new(b"wrong_password".to_vec());
    match embedder.extract(&carrier, &wrong).await {
        Ok(data) if data == secret => println!("Unexpected: wrong password succeeded"),
        _ => println!("Wrong password correctly rejected"),
    }

    Ok(())
}
