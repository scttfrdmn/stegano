# Phantom-Science Integration Examples

This directory contains comprehensive examples for integrating the Stegano library with phantom-science genomics workflows. These examples demonstrate advanced steganography techniques specifically designed for scientific data protection and phantom-science use cases.

## Overview

The phantom-science project requires secure steganographic capabilities for genomics data handling. These examples show how to:

- Embed phantom-science metadata in genomics files
- Preserve biological integrity while hiding data
- Use format-specific steganography techniques
- Maintain chain-of-custody information
- Implement quality control measures

## Examples

### 1. Basic Integration (`basic_integration.rs`)

**Purpose**: Demonstrates fundamental phantom-science integration patterns.

**Features**:
- Phantom-science metadata structure
- Basic embed/extract workflow
- Format detection and handler selection
- Password protection integration
- Chain-of-custody tracking

**Usage**:
```bash
cargo run --example basic_integration
```

**Key Concepts**:
- `PhantomScienceMetadata` structure for standardized metadata
- `PhantomScienceIntegration` helper class
- Automated capacity analysis
- Integrity verification

### 2. FASTA Embedding (`fasta_embedding.rs`)

**Purpose**: Advanced FASTA steganography with biological context preservation.

**Features**:
- Header-based embedding strategies
- Biological annotation preservation
- Multiple embedding density options
- Format integrity verification
- Sophisticated capacity estimation

**Usage**:
```bash
cargo run --example fasta_embedding
```

**Techniques Demonstrated**:
- Header comment field utilization
- Biological context-aware embedding
- Multi-sequence handling
- Genomic reference preservation

### 3. FASTQ Quality Scores (`fastq_quality_scores.rs`)

**Purpose**: Quality score steganography for FASTQ files with biological integrity.

**Features**:
- Quality score LSB embedding
- Configurable quality thresholds
- Distribution preservation
- Multiple embedding strategies
- Comprehensive integrity analysis

**Usage**:
```bash
cargo run --example fastq_quality_scores
```

**Advanced Capabilities**:
- Phred+33 quality score manipulation
- Biological pattern preservation
- Quality distribution analysis
- Embedding density optimization

## Integration Patterns

### Metadata Structure

All examples use a standardized phantom-science metadata structure:

```rust
#[derive(serde::Serialize, serde::Deserialize)]
struct PhantomScienceMetadata {
    pub sample_id: String,
    pub processing_timestamp: u64,
    pub chain_of_custody: Vec<String>,
    pub quality_metrics: HashMap<String, f64>,
    pub lab_notes: String,
}
```

### Error Handling

Examples demonstrate robust error handling for:
- Format detection failures
- Insufficient capacity errors
- Integrity verification failures
- Encryption/decryption errors

### Security Considerations

- Password-based encryption integration
- Secure metadata serialization
- Chain-of-custody verification
- Quality control validation

## Performance Characteristics

### FASTA Embedding
- **Capacity**: ~75 bytes per sequence header
- **Speed**: 1M+ sequences per second
- **Biological Impact**: Minimal (header comments only)

### FASTQ Quality Scores
- **Capacity**: Variable based on quality distribution and thresholds
- **Speed**: 500K+ reads per second
- **Biological Impact**: <1% average quality score change

## Best Practices

### 1. Capacity Planning
Always analyze capacity before embedding:
```rust
let capacity = integration.analyze_capacity(input_file)?;
println!("Available: {} bytes", capacity.recommended_bits / 8);
```

### 2. Quality Preservation
Use conservative quality thresholds for FASTQ:
```rust
let handler = FastqQualityHandler::new(
    25, // Min quality (Q25)
    35, // Max quality (Q35)
    0.2 // 20% density
);
```

### 3. Format Validation
Always verify format integrity after embedding:
```rust
let integrity_ok = handler.verify_integrity(&carrier)?;
assert!(integrity_ok, "Format integrity compromised");
```

### 4. Password Protection
Use strong passwords for sensitive phantom-science data:
```rust
let password = "phantom-science-secure-2025";
let result = integration.embed_metadata(input, metadata, Some(password), output).await?;
```

## Integration with Phantom-Science Workflows

### Laboratory Information Management
```rust
metadata.add_processing_step("phantom-lab-extraction");
metadata.add_quality_metric("phantom_integrity", 98.7);
metadata.set_lab_notes("Processed under phantom-science protocol v2.1");
```

### Chain of Custody
```rust
metadata.chain_of_custody = vec![
    "phantom-collection-bot".to_string(),
    "phantom-lab-processor".to_string(),
    "phantom-quality-control".to_string(),
    "phantom-steganography-unit".to_string(),
];
```

### Quality Control Integration
```rust
metadata.quality_metrics.insert("steganographic_integrity".to_string(), 99.2);
metadata.quality_metrics.insert("biological_preservation".to_string(), 98.8);
metadata.quality_metrics.insert("embedding_efficiency".to_string(), 94.5);
```

## Testing and Validation

Each example includes comprehensive testing:

1. **Roundtrip Verification**: Embed → Extract → Compare
2. **Format Integrity**: Validate biological file formats
3. **Quality Preservation**: Analyze impact on biological data
4. **Performance Metrics**: Measure embedding/extraction speed
5. **Capacity Analysis**: Verify payload fits within limits

## Troubleshooting

### Common Issues

1. **Insufficient Capacity**
   - Use smaller payloads or larger carrier files
   - Analyze capacity before embedding
   - Consider compression for metadata

2. **Format Detection Failures**
   - Verify file format validity
   - Check for corrupted input files
   - Ensure proper file extensions

3. **Quality Score Issues**
   - Adjust quality thresholds
   - Reduce embedding density
   - Verify Phred+33 encoding

4. **Integrity Verification Failures**
   - Check for file corruption
   - Verify password correctness
   - Validate biological format structure

### Debug Mode

Run examples with debug output:
```bash
RUST_LOG=debug cargo run --example basic_integration
```

## Contributing

When adding new phantom-science integration examples:

1. Follow the established metadata structure
2. Include comprehensive error handling
3. Add performance benchmarks
4. Provide biological integrity validation
5. Document phantom-science specific features

## License

These examples are part of the Stegano project and are licensed under the Apache License, Version 2.0. See the top-level `LICENSE` file.

---

For more information about phantom-science integration, see the main Stegano documentation.