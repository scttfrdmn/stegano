//! Shows embedding into a FASTA file with the genomics handler while keeping the sequences intact.

use stegano::*;

/// Build a synthetic multi-record FASTA file (60-column lines).
fn synthetic_fasta(records: usize, bases_per_record: usize) -> String {
    let mut state: u32 = 42;
    let mut out = String::new();
    for r in 0..records {
        out.push_str(&format!(">seq_{r} synthetic sequence for demonstration\n"));
        let seq: Vec<u8> = (0..bases_per_record)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                b"ACGT"[(state >> 30) as usize]
            })
            .collect();
        for line in seq.chunks(60) {
            out.push_str(std::str::from_utf8(line).unwrap());
            out.push('\n');
        }
    }
    out
}

/// Sequence lines only (headers removed), used to check biological content is unchanged.
fn sequences(fasta: &[u8]) -> String {
    String::from_utf8_lossy(fasta)
        .lines()
        .filter(|l| !l.starts_with('>'))
        .collect()
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    println!("Stegano genomics demo (FASTA carrier)\n");

    let fasta = synthetic_fasta(40, 600);
    let mut carrier = CarrierData::new(DataFormat::FASTA, fasta.into_bytes());
    let original_sequences = sequences(&carrier.raw_data);

    let secret = b"Variant chr7:117559590 A>G, sample S-0042";
    let params = EmbedParams::new(b"genomics_research_key".to_vec());

    let embedder = SteganoEmbedder::new();
    let capacity = embedder.estimate_capacity(&carrier)?;
    println!("FASTA size:          {} bytes", carrier.raw_data.len());
    println!(
        "Recommended payload: {} bytes",
        capacity.recommended_bits / 8
    );

    let result = embedder.embed(&mut carrier, secret, &params).await?;
    println!("Embedded {} bytes", result.bytes_embedded);

    let records = String::from_utf8_lossy(&carrier.raw_data)
        .matches('>')
        .count();
    let sequences_intact = sequences(&carrier.raw_data) == original_sequences;
    println!("Records after embedding: {records}");
    println!("Sequence data unchanged: {sequences_intact}");

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
