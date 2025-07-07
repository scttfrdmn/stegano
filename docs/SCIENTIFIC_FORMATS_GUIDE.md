# Scientific Formats Guide

This guide covers steganography techniques for scientific data formats including VCF (genomics), DICOM (medical imaging), and SAM/BAM (bioinformatics) while preserving data integrity and domain compliance.

## Table of Contents

1. [Overview](#overview)
2. [VCF Format (Genomics)](#vcf-format-genomics)
3. [DICOM Format (Medical Imaging)](#dicom-format-medical-imaging)
4. [SAM/BAM Format (Bioinformatics)](#sambam-format-bioinformatics)
5. [Integrity Preservation](#integrity-preservation)
6. [Performance Considerations](#performance-considerations)
7. [Best Practices](#best-practices)
8. [Compliance Guidelines](#compliance-guidelines)

## Overview

Stegano supports advanced steganography for scientific data formats, implementing domain-specific embedding strategies that preserve:

- **Biological meaning** and data relationships
- **Clinical accuracy** and diagnostic value
- **Research validity** and reproducibility
- **Format compliance** with industry standards

Each format handler implements multiple embedding strategies optimized for different capacity and stealth requirements.

## VCF Format (Genomics)

### Description

VCF (Variant Call Format) files store genomic variation data including SNPs, insertions, deletions, and structural variants. The VCF handler preserves biological significance while enabling secure data embedding.

### Embedding Strategies

#### 1. INFO Field Steganography
Embeds data in non-critical INFO field values while preserving essential annotations.

```rust
use stegano::handlers::vcf::*;
use stegano::core::*;

async fn vcf_info_field_example() -> Result<()> {
    let vcf_data = std::fs::read("samples.vcf")?;
    let carrier = CarrierData::new(vcf_data, DataFormat::VCF);
    
    let params = EmbedParams {
        format_specific: {
            let mut map = std::collections::HashMap::new();
            map.insert("vcf_strategy".to_string(), 
                      serde_json::Value::String("info_fields".to_string()));
            map.insert("preserve_critical_annotations".to_string(),
                      serde_json::Value::Bool(true));
            map
        },
        stealth_level: StealthLevel::High,
        error_correction_level: ECLevel::Medium,
        ..Default::default()
    };
    
    let handler = VcfHandler::new();
    let payload = b"genomic_research_data";
    
    let mut carrier_mut = carrier;
    let result = handler.embed(&mut carrier_mut, payload, &params).await?;
    
    println!("Embedded {} bytes in VCF INFO fields", result.bytes_embedded);
    println!("Preserved {} critical annotations", result.metadata.get("preserved_annotations").unwrap_or(&0));
    
    Ok(())
}
```

#### 2. Quality Score Precision
Uses least significant digits of quality scores for data embedding.

```rust
async fn vcf_quality_precision_example() -> Result<()> {
    let vcf_data = std::fs::read("high_coverage_variants.vcf")?;
    let carrier = CarrierData::new(vcf_data, DataFormat::VCF);
    
    let params = EmbedParams {
        format_specific: {
            let mut map = std::collections::HashMap::new();
            map.insert("vcf_strategy".to_string(), 
                      serde_json::Value::String("quality_precision".to_string()));
            map.insert("max_precision_loss".to_string(),
                      serde_json::Value::Number(serde_json::Number::from_f64(0.01).unwrap()));
            map.insert("min_quality_threshold".to_string(),
                      serde_json::Value::Number(serde_json::Number::from(30)));
            map
        },
        stealth_level: StealthLevel::Maximum,
        error_correction_level: ECLevel::High,
        ..Default::default()
    };
    
    let handler = VcfHandler::new();
    let secret_data = b"patient_metadata_encrypted";
    
    let mut carrier_mut = carrier;
    let result = handler.embed(&mut carrier_mut, secret_data, &params).await?;
    
    println!("Embedded data with max quality impact: {:.3}", 
             result.metadata.get("max_quality_impact").unwrap_or(&0.0));
    
    Ok(())
}
```

#### 3. Sample FORMAT Fields
Embeds data in sample-specific FORMAT field values.

```rust
async fn vcf_format_fields_example() -> Result<()> {
    let vcf_data = std::fs::read("multi_sample.vcf")?;
    let carrier = CarrierData::new(vcf_data, DataFormat::VCF);
    
    let params = EmbedParams {
        format_specific: {
            let mut map = std::collections::HashMap::new();
            map.insert("vcf_strategy".to_string(), 
                      serde_json::Value::String("format_fields".to_string()));
            map.insert("target_samples".to_string(),
                      serde_json::Value::Array(vec![
                          serde_json::Value::String("SAMPLE_001".to_string()),
                          serde_json::Value::String("SAMPLE_002".to_string()),
                      ]));
            map.insert("preserve_genotypes".to_string(),
                      serde_json::Value::Bool(true));
            map
        },
        stealth_level: StealthLevel::Medium,
        error_correction_level: ECLevel::Low,
        ..Default::default()
    };
    
    let handler = VcfHandler::new();
    let payload = b"sample_correlation_data";
    
    let mut carrier_mut = carrier;
    let result = handler.embed(&mut carrier_mut, payload, &params).await?;
    
    println!("Used {} samples for embedding", 
             result.metadata.get("samples_used").unwrap_or(&0));
    
    Ok(())
}
```

### Capacity Estimation

```rust
async fn vcf_capacity_analysis() -> Result<()> {
    let vcf_data = std::fs::read("large_cohort.vcf")?;
    let carrier = CarrierData::new(vcf_data, DataFormat::VCF);
    let handler = VcfHandler::new();
    
    let capacity_info = handler.estimate_capacity(&carrier)?;
    
    println!("VCF Capacity Analysis:");
    println!("Total capacity: {} bits ({} bytes)", 
             capacity_info.total_bits, capacity_info.total_bits / 8);
    println!("Recommended capacity: {} bits ({} bytes)", 
             capacity_info.recommended_bits, capacity_info.recommended_bits / 8);
    println!("Steganographic ratio: {:.2}%", 
             capacity_info.steganographic_ratio * 100.0);
    
    // Strategy-specific capacities
    if let Some(strategies) = capacity_info.format_specific_info.get("strategies") {
        if let serde_json::Value::Object(strategy_map) = strategies {
            for (strategy, capacity) in strategy_map {
                println!("{}: {} bits", strategy, capacity);
            }
        }
    }
    
    Ok(())
}
```

## DICOM Format (Medical Imaging)

### Description

DICOM (Digital Imaging and Communications in Medicine) is the standard for medical imaging. The DICOM handler maintains clinical accuracy and regulatory compliance while enabling secure data embedding.

### Embedding Strategies

#### 1. Private Data Elements
Uses private DICOM tags for data embedding without affecting clinical interpretation.

```rust
use stegano::handlers::dicom::*;

async fn dicom_private_elements_example() -> Result<()> {
    let dicom_data = std::fs::read("mri_scan.dcm")?;
    let carrier = CarrierData::new(dicom_data, DataFormat::DICOM);
    
    let params = EmbedParams {
        format_specific: {
            let mut map = std::collections::HashMap::new();
            map.insert("dicom_strategy".to_string(), 
                      serde_json::Value::String("private_elements".to_string()));
            map.insert("private_group_range".to_string(),
                      serde_json::Value::Array(vec![
                          serde_json::Value::Number(serde_json::Number::from(0x0009)),
                          serde_json::Value::Number(serde_json::Number::from(0x00FF)),
                      ]));
            map.insert("preserve_clinical_data".to_string(),
                      serde_json::Value::Bool(true));
            map
        },
        stealth_level: StealthLevel::High,
        error_correction_level: ECLevel::High,
        ..Default::default()
    };
    
    let handler = DicomHandler::new();
    let medical_metadata = b"encrypted_patient_history";
    
    let mut carrier_mut = carrier;
    let result = handler.embed(&mut carrier_mut, medical_metadata, &params).await?;
    
    println!("Embedded {} bytes in private DICOM elements", result.bytes_embedded);
    println!("Clinical data integrity preserved: {}", 
             result.metadata.get("clinical_integrity_preserved").unwrap_or(&true));
    
    Ok(())
}
```

#### 2. Pixel Data LSB
Embeds data in least significant bits of pixel values with medical-grade precision control.

```rust
async fn dicom_pixel_lsb_example() -> Result<()> {
    let dicom_data = std::fs::read("ct_scan.dcm")?;
    let carrier = CarrierData::new(dicom_data, DataFormat::DICOM);
    
    let params = EmbedParams {
        format_specific: {
            let mut map = std::collections::HashMap::new();
            map.insert("dicom_strategy".to_string(), 
                      serde_json::Value::String("pixel_lsb".to_string()));
            map.insert("max_pixel_change".to_string(),
                      serde_json::Value::Number(serde_json::Number::from(1))); // Max 1 HU change
            map.insert("preserve_diagnostic_regions".to_string(),
                      serde_json::Value::Bool(true));
            map.insert("bits_per_pixel".to_string(),
                      serde_json::Value::Number(serde_json::Number::from(1))); // Use 1 LSB
            map
        },
        stealth_level: StealthLevel::Maximum,
        error_correction_level: ECLevel::Medium,
        ..Default::default()
    };
    
    let handler = DicomHandler::new();
    let research_data = b"clinical_trial_encrypted_results";
    
    let mut carrier_mut = carrier;
    let result = handler.embed(&mut carrier_mut, research_data, &params).await?;
    
    println!("Embedded data in {} pixels", 
             result.metadata.get("pixels_modified").unwrap_or(&0));
    println!("Max pixel value change: {} HU", 
             result.metadata.get("max_pixel_change").unwrap_or(&0));
    
    Ok(())
}
```

#### 3. Unused Metadata Fields
Embeds data in unused or optional DICOM metadata fields.

```rust
async fn dicom_metadata_example() -> Result<()> {
    let dicom_data = std::fs::read("ultrasound.dcm")?;
    let carrier = CarrierData::new(dicom_data, DataFormat::DICOM);
    
    let params = EmbedParams {
        format_specific: {
            let mut map = std::collections::HashMap::new();
            map.insert("dicom_strategy".to_string(), 
                      serde_json::Value::String("unused_metadata".to_string()));
            map.insert("preserve_required_tags".to_string(),
                      serde_json::Value::Bool(true));
            map.insert("max_field_length".to_string(),
                      serde_json::Value::Number(serde_json::Number::from(256)));
            map
        },
        stealth_level: StealthLevel::Medium,
        error_correction_level: ECLevel::Low,
        ..Default::default()
    };
    
    let handler = DicomHandler::new();
    let device_logs = b"ultrasound_device_calibration_log";
    
    let mut carrier_mut = carrier;
    let result = handler.embed(&mut carrier_mut, device_logs, &params).await?;
    
    println!("Used {} metadata fields for embedding", 
             result.metadata.get("fields_used").unwrap_or(&0));
    
    Ok(())
}
```

### Medical Compliance Verification

```rust
async fn dicom_compliance_check() -> Result<()> {
    let dicom_data = std::fs::read("medical_image.dcm")?;
    let carrier = CarrierData::new(dicom_data, DataFormat::DICOM);
    let handler = DicomHandler::new();
    
    // Verify integrity before embedding
    let integrity_ok = handler.verify_integrity(&carrier)?;
    println!("Pre-embedding integrity: {}", integrity_ok);
    
    // Perform embedding with strict compliance
    let params = EmbedParams {
        format_specific: {
            let mut map = std::collections::HashMap::new();
            map.insert("dicom_strategy".to_string(), 
                      serde_json::Value::String("private_elements".to_string()));
            map.insert("strict_compliance".to_string(),
                      serde_json::Value::Bool(true));
            map.insert("preserve_sop_class".to_string(),
                      serde_json::Value::Bool(true));
            map
        },
        stealth_level: StealthLevel::High,
        error_correction_level: ECLevel::High,
        ..Default::default()
    };
    
    let payload = b"hipaa_compliant_metadata";
    let mut carrier_mut = carrier;
    let result = handler.embed(&mut carrier_mut, payload, &params).await?;
    
    // Verify integrity after embedding
    let post_integrity_ok = handler.verify_integrity(&carrier_mut)?;
    println!("Post-embedding integrity: {}", post_integrity_ok);
    
    // Check compliance metrics
    if let Some(compliance) = result.metadata.get("compliance_metrics") {
        println!("DICOM compliance maintained: {}", compliance);
    }
    
    Ok(())
}
```

## SAM/BAM Format (Bioinformatics)

### Description

SAM (Sequence Alignment/Map) and BAM (Binary Alignment/Map) formats store DNA/RNA sequence alignments. The handler preserves biological relationships and alignment quality while enabling data embedding.

### Embedding Strategies

#### 1. Optional Fields
Embeds data in optional SAM fields that don't affect core alignment information.

```rust
use stegano::handlers::sam_bam::*;

async fn sam_optional_fields_example() -> Result<()> {
    let sam_data = std::fs::read("alignments.sam")?;
    let carrier = CarrierData::new(sam_data, DataFormat::SAM);
    
    let params = EmbedParams {
        format_specific: {
            let mut map = std::collections::HashMap::new();
            map.insert("sam_strategy".to_string(), 
                      serde_json::Value::String("optional_fields".to_string()));
            map.insert("preserve_core_alignment".to_string(),
                      serde_json::Value::Bool(true));
            map.insert("max_field_size".to_string(),
                      serde_json::Value::Number(serde_json::Number::from(255)));
            map
        },
        stealth_level: StealthLevel::High,
        error_correction_level: ECLevel::Medium,
        ..Default::default()
    };
    
    let handler = SamBamHandler;
    let sequence_metadata = b"sample_origin_encrypted";
    
    let mut carrier_mut = carrier;
    let result = handler.embed(&mut carrier_mut, sequence_metadata, &params).await?;
    
    println!("Embedded data in {} optional fields", 
             result.metadata.get("fields_used").unwrap_or(&0));
    
    Ok(())
}
```

#### 2. Quality Score Precision
Uses precision in base quality scores for data embedding.

```rust
async fn sam_quality_scores_example() -> Result<()> {
    let sam_data = std::fs::read("high_quality_reads.sam")?;
    let carrier = CarrierData::new(sam_data, DataFormat::SAM);
    
    let params = EmbedParams {
        format_specific: {
            let mut map = std::collections::HashMap::new();
            map.insert("sam_strategy".to_string(), 
                      serde_json::Value::String("quality_scores".to_string()));
            map.insert("min_quality_threshold".to_string(),
                      serde_json::Value::Number(serde_json::Number::from(20))); // Phred 20+
            map.insert("max_quality_change".to_string(),
                      serde_json::Value::Number(serde_json::Number::from(2)));  // Max 2 points
            map.insert("preserve_low_quality".to_string(),
                      serde_json::Value::Bool(true));
            map
        },
        stealth_level: StealthLevel::Maximum,
        error_correction_level: ECLevel::Low,
        ..Default::default()
    };
    
    let handler = SamBamHandler;
    let pipeline_metadata = b"bioinformatics_pipeline_log";
    
    let mut carrier_mut = carrier;
    let result = handler.embed(&mut carrier_mut, pipeline_metadata, &params).await?;
    
    println!("Modified quality scores in {} reads", 
             result.metadata.get("reads_modified").unwrap_or(&0));
    println!("Average quality impact: {:.2}", 
             result.metadata.get("avg_quality_impact").unwrap_or(&0.0));
    
    Ok(())
}
```

#### 3. Comment Lines
Embeds data in SAM comment lines without affecting alignment data.

```rust
async fn sam_comment_lines_example() -> Result<()> {
    let sam_data = std::fs::read("annotated_alignments.sam")?;
    let carrier = CarrierData::new(sam_data, DataFormat::SAM);
    
    let params = EmbedParams {
        format_specific: {
            let mut map = std::collections::HashMap::new();
            map.insert("sam_strategy".to_string(), 
                      serde_json::Value::String("comment_lines".to_string()));
            map.insert("preserve_existing_comments".to_string(),
                      serde_json::Value::Bool(true));
            map.insert("max_comment_length".to_string(),
                      serde_json::Value::Number(serde_json::Number::from(1000)));
            map
        },
        stealth_level: StealthLevel::Low,  // Comments are visible
        error_correction_level: ECLevel::High,
        ..Default::default()
    };
    
    let handler = SamBamHandler;
    let analysis_notes = b"variant_calling_parameters_and_thresholds";
    
    let mut carrier_mut = carrier;
    let result = handler.embed(&mut carrier_mut, analysis_notes, &params).await?;
    
    println!("Added {} comment lines", 
             result.metadata.get("comments_added").unwrap_or(&0));
    
    Ok(())
}
```

### Alignment Integrity Verification

```rust
async fn sam_integrity_verification() -> Result<()> {
    let sam_data = std::fs::read("important_alignments.sam")?;
    let carrier = CarrierData::new(sam_data, DataFormat::SAM);
    let handler = SamBamHandler;
    
    // Verify alignment integrity
    let integrity_ok = handler.verify_integrity(&carrier)?;
    println!("Alignment integrity: {}", integrity_ok);
    
    // Get detailed capacity analysis
    let capacity_info = handler.estimate_capacity(&carrier)?;
    println!("Alignment file capacity analysis:");
    println!("Total reads: {}", 
             capacity_info.format_specific_info
                         .get("total_reads")
                         .and_then(|v| v.as_u64())
                         .unwrap_or(0));
    println!("Average read length: {}", 
             capacity_info.format_specific_info
                         .get("avg_read_length")
                         .and_then(|v| v.as_f64())
                         .unwrap_or(0.0));
    
    Ok(())
}
```

## Integrity Preservation

### Biological Data Validation

All scientific format handlers implement domain-specific validation to ensure biological and clinical meaning is preserved:

```rust
async fn comprehensive_integrity_check() -> Result<()> {
    // VCF integrity checks
    let vcf_data = std::fs::read("variants.vcf")?;
    let vcf_carrier = CarrierData::new(vcf_data, DataFormat::VCF);
    let vcf_handler = VcfHandler::new();
    
    println!("VCF Integrity Checks:");
    println!("- Format compliance: {}", vcf_handler.verify_integrity(&vcf_carrier)?);
    
    // DICOM integrity checks  
    let dicom_data = std::fs::read("scan.dcm")?;
    let dicom_carrier = CarrierData::new(dicom_data, DataFormat::DICOM);
    let dicom_handler = DicomHandler::new();
    
    println!("DICOM Integrity Checks:");
    println!("- Medical compliance: {}", dicom_handler.verify_integrity(&dicom_carrier)?);
    
    // SAM integrity checks
    let sam_data = std::fs::read("reads.sam")?;
    let sam_carrier = CarrierData::new(sam_data, DataFormat::SAM);
    let sam_handler = SamBamHandler;
    
    println!("SAM Integrity Checks:");
    println!("- Alignment validity: {}", sam_handler.verify_integrity(&sam_carrier)?);
    
    Ok(())
}
```

### Custom Validation Rules

```rust
use stegano::core::*;

// Custom validator for genomics data
struct GenomicsValidator {
    check_allele_frequencies: bool,
    validate_chromosomes: bool,
    verify_reference_genome: bool,
}

impl GenomicsValidator {
    fn validate_vcf_post_embedding(&self, original: &CarrierData, embedded: &CarrierData) -> Result<bool> {
        // Implement custom validation logic
        let original_variants = self.extract_variants(original)?;
        let embedded_variants = self.extract_variants(embedded)?;
        
        // Check that critical variant information is preserved
        let variants_preserved = original_variants.len() == embedded_variants.len();
        
        if self.check_allele_frequencies {
            let freq_preserved = self.compare_allele_frequencies(&original_variants, &embedded_variants)?;
            if !freq_preserved {
                return Ok(false);
            }
        }
        
        if self.validate_chromosomes {
            let chr_preserved = self.validate_chromosome_consistency(&embedded_variants)?;
            if !chr_preserved {
                return Ok(false);
            }
        }
        
        Ok(variants_preserved)
    }
    
    fn extract_variants(&self, _data: &CarrierData) -> Result<Vec<Variant>> {
        // Implementation would parse VCF and extract variants
        Ok(vec![])
    }
    
    fn compare_allele_frequencies(&self, _original: &[Variant], _embedded: &[Variant]) -> Result<bool> {
        // Implementation would compare allele frequencies
        Ok(true)
    }
    
    fn validate_chromosome_consistency(&self, _variants: &[Variant]) -> Result<bool> {
        // Implementation would validate chromosome references
        Ok(true)
    }
}

#[derive(Debug)]
struct Variant {
    chromosome: String,
    position: u64,
    reference: String,
    alternate: String,
    quality: f64,
}
```

## Performance Considerations

### Format-Specific Optimizations

Each format handler is optimized for different performance characteristics:

```rust
async fn performance_comparison() -> Result<()> {
    let test_files = vec![
        ("small.vcf", DataFormat::VCF, 1024),      // 1KB VCF
        ("medium.dcm", DataFormat::DICOM, 1048576), // 1MB DICOM
        ("large.sam", DataFormat::SAM, 10485760),   // 10MB SAM
    ];
    
    for (filename, format, expected_size) in test_files {
        let data = std::fs::read(filename)?;
        let carrier = CarrierData::new(data, format.clone());
        
        let start_time = std::time::Instant::now();
        
        let capacity_info = match format {
            DataFormat::VCF => {
                let handler = VcfHandler::new();
                handler.estimate_capacity(&carrier)?
            },
            DataFormat::DICOM => {
                let handler = DicomHandler::new();
                handler.estimate_capacity(&carrier)?
            },
            DataFormat::SAM => {
                let handler = SamBamHandler;
                handler.estimate_capacity(&carrier)?
            },
            _ => continue,
        };
        
        let duration = start_time.elapsed();
        
        println!("{:?} Performance:", format);
        println!("  File size: {} bytes", expected_size);
        println!("  Capacity estimation: {:.2}ms", duration.as_millis());
        println!("  Capacity ratio: {:.2}%", capacity_info.steganographic_ratio * 100.0);
        println!("  Throughput: {:.2} MB/s", 
                 expected_size as f64 / (1024.0 * 1024.0) / duration.as_secs_f64());
    }
    
    Ok(())
}
```

### Memory Usage Optimization

```rust
use std::sync::Arc;

async fn memory_efficient_processing() -> Result<()> {
    // For large files, use streaming processing
    let large_sam_file = "large_genome_alignment.sam";
    
    // Process in chunks to minimize memory usage
    let chunk_size = 1024 * 1024; // 1MB chunks
    let mut file = std::fs::File::open(large_sam_file)?;
    let mut buffer = vec![0u8; chunk_size];
    
    let mut total_capacity = 0u64;
    let mut chunk_count = 0;
    
    loop {
        let bytes_read = std::io::Read::read(&mut file, &mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        
        let chunk_data = &buffer[..bytes_read];
        let carrier = CarrierData::new(chunk_data.to_vec(), DataFormat::SAM);
        let handler = SamBamHandler;
        
        let capacity_info = handler.estimate_capacity(&carrier)?;
        total_capacity += capacity_info.total_bits as u64;
        chunk_count += 1;
        
        if chunk_count % 100 == 0 {
            println!("Processed {} chunks, total capacity: {} bits", 
                     chunk_count, total_capacity);
        }
    }
    
    println!("Final analysis: {} chunks, {} total bits capacity", 
             chunk_count, total_capacity);
    
    Ok(())
}
```

## Best Practices

### 1. Format Selection

Choose the appropriate format based on your requirements:

```rust
fn select_optimal_format(data_type: &str, capacity_needs: usize, stealth_requirements: StealthLevel) -> DataFormat {
    match (data_type, capacity_needs, stealth_requirements) {
        ("genomics", _, StealthLevel::Maximum) => DataFormat::VCF,  // INFO fields very stealthy
        ("medical", _, _) => DataFormat::DICOM,                     // Private elements compliant
        ("sequencing", large_capacity, _) if large_capacity > 1024*1024 => DataFormat::SAM, // Good capacity
        ("genomics", _, _) => DataFormat::VCF,                      // Default for genomics
        _ => DataFormat::DICOM,                                     // Safe default
    }
}
```

### 2. Parameter Optimization

Use ML optimization for best results:

```rust
async fn optimized_scientific_embedding() -> Result<()> {
    // Configure optimization for scientific data
    let config = OptimizationConfig {
        algorithm: OptimizationAlgorithm::BayesianOptimization {
            acquisition_function: AcquisitionFunction::ExpectedImprovement,
        },
        max_iterations: 50,
        population_size: 10,
        tolerance: 0.001,
        multi_objective: MultiObjectiveConfig {
            enabled: true,
            objective_weights: {
                let mut weights = std::collections::HashMap::new();
                weights.insert("stealth".to_string(), 1.0);
                weights.insert("integrity".to_string(), 1.2);  // Prioritize integrity
                weights.insert("capacity".to_string(), 0.8);
                weights
            },
            pareto_method: ParetoMethod::NSGA2,
            max_pareto_solutions: 25,
        },
        parallel_config: ParallelOptimizationConfig {
            enabled: true,
            num_workers: 4,
            batch_size: 5,
        },
    };
    
    let mut optimizer = OptimizationEngine::new(config);
    
    // Add scientific data specific objectives
    optimizer.add_objective(Box::new(ScientificIntegrityObjective { weight: 1.0 }));
    optimizer.add_objective(Box::new(StealthObjective { weight: 0.8 }));
    optimizer.add_objective(Box::new(CapacityObjective { weight: 0.6 }));
    
    // Add compliance constraints
    optimizer.add_constraint(Box::new(MedicalComplianceConstraint));
    optimizer.add_constraint(Box::new(BiologicalValidityConstraint));
    
    let carrier = CarrierData::from_file("clinical_study_data.vcf")?;
    let payload = std::fs::read("research_results.json")?;
    
    let result = optimizer.multi_objective_optimization(&carrier, &payload).await?;
    
    println!("Found {} compliant solutions", result.pareto_front.len());
    
    Ok(())
}

// Custom objectives for scientific data
struct ScientificIntegrityObjective {
    weight: f32,
}

impl ObjectiveFunction for ScientificIntegrityObjective {
    fn evaluate(&self, params: &EmbedParams, carrier: &CarrierData, _payload: &[u8]) -> Result<f32> {
        // Evaluate how well parameters preserve scientific data integrity
        let integrity_score = match carrier.format {
            DataFormat::VCF => self.evaluate_vcf_integrity(params, carrier)?,
            DataFormat::DICOM => self.evaluate_dicom_integrity(params, carrier)?,
            DataFormat::SAM | DataFormat::BAM => self.evaluate_sam_integrity(params, carrier)?,
            _ => 0.5, // Default score
        };
        
        Ok(integrity_score * self.weight)
    }
    
    fn name(&self) -> &str {
        "scientific_integrity"
    }
    
    fn maximize(&self) -> bool {
        true
    }
    
    fn weight(&self) -> f32 {
        self.weight
    }
}

impl ScientificIntegrityObjective {
    fn evaluate_vcf_integrity(&self, _params: &EmbedParams, _carrier: &CarrierData) -> Result<f32> {
        // Check VCF-specific integrity metrics
        Ok(0.9) // Placeholder
    }
    
    fn evaluate_dicom_integrity(&self, _params: &EmbedParams, _carrier: &CarrierData) -> Result<f32> {
        // Check DICOM-specific integrity metrics
        Ok(0.95) // Placeholder
    }
    
    fn evaluate_sam_integrity(&self, _params: &EmbedParams, _carrier: &CarrierData) -> Result<f32> {
        // Check SAM-specific integrity metrics
        Ok(0.85) // Placeholder
    }
}

struct MedicalComplianceConstraint;
struct BiologicalValidityConstraint;
struct CapacityObjective { weight: f32 }

impl ObjectiveFunction for CapacityObjective {
    fn evaluate(&self, _params: &EmbedParams, _carrier: &CarrierData, _payload: &[u8]) -> Result<f32> {
        Ok(0.7 * self.weight) // Placeholder
    }
    fn name(&self) -> &str { "capacity" }
    fn maximize(&self) -> bool { true }
    fn weight(&self) -> f32 { self.weight }
}

impl OptimizationConstraint for MedicalComplianceConstraint {
    fn is_satisfied(&self, _params: &EmbedParams, carrier: &CarrierData) -> bool {
        matches!(carrier.format, DataFormat::DICOM) // Simplified check
    }
    fn name(&self) -> &str { "medical_compliance" }
}

impl OptimizationConstraint for BiologicalValidityConstraint {
    fn is_satisfied(&self, _params: &EmbedParams, carrier: &CarrierData) -> bool {
        matches!(carrier.format, DataFormat::VCF | DataFormat::SAM | DataFormat::BAM)
    }
    fn name(&self) -> &str { "biological_validity" }
}
```

### 3. Error Handling and Recovery

```rust
async fn robust_scientific_embedding() -> Result<()> {
    let files = vec![
        ("variants.vcf", DataFormat::VCF),
        ("scan.dcm", DataFormat::DICOM),
        ("reads.sam", DataFormat::SAM),
    ];
    
    for (filename, format) in files {
        match process_scientific_file(filename, format).await {
            Ok(result) => {
                println!("Successfully processed {}: {} bytes embedded", 
                         filename, result.bytes_embedded);
            },
            Err(e) => {
                eprintln!("Error processing {}: {}", filename, e);
                
                // Attempt recovery with different parameters
                match recover_with_fallback_params(filename, format).await {
                    Ok(result) => {
                        println!("Recovery successful for {}: {} bytes embedded", 
                                 filename, result.bytes_embedded);
                    },
                    Err(recovery_error) => {
                        eprintln!("Recovery failed for {}: {}", filename, recovery_error);
                    }
                }
            }
        }
    }
    
    Ok(())
}

async fn process_scientific_file(filename: &str, format: DataFormat) -> Result<EmbedResult> {
    let data = std::fs::read(filename)?;
    let carrier = CarrierData::new(data, format);
    
    let params = EmbedParams {
        stealth_level: StealthLevel::High,
        error_correction_level: ECLevel::Medium,
        ..Default::default()
    };
    
    let payload = b"scientific_data_payload";
    let mut carrier_mut = carrier;
    
    match format {
        DataFormat::VCF => {
            let handler = VcfHandler::new();
            handler.embed(&mut carrier_mut, payload, &params).await
        },
        DataFormat::DICOM => {
            let handler = DicomHandler::new();
            handler.embed(&mut carrier_mut, payload, &params).await
        },
        DataFormat::SAM | DataFormat::BAM => {
            let handler = SamBamHandler;
            handler.embed(&mut carrier_mut, payload, &params).await
        },
        _ => Err(SteganoError::UnsupportedFormat(format)),
    }
}

async fn recover_with_fallback_params(filename: &str, format: DataFormat) -> Result<EmbedResult> {
    let data = std::fs::read(filename)?;
    let carrier = CarrierData::new(data, format);
    
    // Use more conservative parameters for recovery
    let fallback_params = EmbedParams {
        stealth_level: StealthLevel::Maximum,    // Maximum stealth
        error_correction_level: ECLevel::High,   // High error correction
        format_specific: {
            let mut map = std::collections::HashMap::new();
            match format {
                DataFormat::VCF => {
                    map.insert("vcf_strategy".to_string(), 
                              serde_json::Value::String("info_fields".to_string()));
                    map.insert("preserve_critical_annotations".to_string(),
                              serde_json::Value::Bool(true));
                },
                DataFormat::DICOM => {
                    map.insert("dicom_strategy".to_string(), 
                              serde_json::Value::String("private_elements".to_string()));
                    map.insert("strict_compliance".to_string(),
                              serde_json::Value::Bool(true));
                },
                DataFormat::SAM | DataFormat::BAM => {
                    map.insert("sam_strategy".to_string(), 
                              serde_json::Value::String("optional_fields".to_string()));
                    map.insert("preserve_core_alignment".to_string(),
                              serde_json::Value::Bool(true));
                },
                _ => {}
            }
            map
        },
        ..Default::default()
    };
    
    let payload = b"fallback_payload";
    let mut carrier_mut = carrier;
    
    match format {
        DataFormat::VCF => {
            let handler = VcfHandler::new();
            handler.embed(&mut carrier_mut, payload, &fallback_params).await
        },
        DataFormat::DICOM => {
            let handler = DicomHandler::new();
            handler.embed(&mut carrier_mut, payload, &fallback_params).await
        },
        DataFormat::SAM | DataFormat::BAM => {
            let handler = SamBamHandler;
            handler.embed(&mut carrier_mut, payload, &fallback_params).await
        },
        _ => Err(SteganoError::UnsupportedFormat(format)),
    }
}
```

## Compliance Guidelines

### Medical Data (DICOM)

For medical imaging data, ensure compliance with healthcare regulations:

```rust
struct HIPAACompliantConfig {
    encrypt_phi: bool,
    audit_trail: bool,
    access_controls: bool,
    data_minimization: bool,
}

impl HIPAACompliantConfig {
    fn for_medical_research() -> Self {
        Self {
            encrypt_phi: true,
            audit_trail: true,
            access_controls: true,
            data_minimization: true,
        }
    }
    
    fn to_embed_params(&self) -> EmbedParams {
        EmbedParams {
            format_specific: {
                let mut map = std::collections::HashMap::new();
                map.insert("dicom_strategy".to_string(), 
                          serde_json::Value::String("private_elements".to_string()));
                map.insert("encrypt_embedded_data".to_string(),
                          serde_json::Value::Bool(self.encrypt_phi));
                map.insert("maintain_audit_trail".to_string(),
                          serde_json::Value::Bool(self.audit_trail));
                map.insert("strict_compliance".to_string(),
                          serde_json::Value::Bool(true));
                map
            },
            stealth_level: StealthLevel::Maximum,
            error_correction_level: ECLevel::High,
            encryption_key: if self.encrypt_phi { 
                vec![1, 2, 3, 4] // Use proper key derivation in practice
            } else { 
                vec![] 
            },
            use_gpu: false, // Disable for compliance
        }
    }
}
```

### Genomics Data (VCF)

For genomics research, consider privacy and research ethics:

```rust
struct GenomicsPrivacyConfig {
    anonymize_samples: bool,
    preserve_population_data: bool,
    research_consent_level: String,
}

impl GenomicsPrivacyConfig {
    fn for_clinical_trial() -> Self {
        Self {
            anonymize_samples: true,
            preserve_population_data: false,
            research_consent_level: "broad".to_string(),
        }
    }
    
    fn to_embed_params(&self) -> EmbedParams {
        EmbedParams {
            format_specific: {
                let mut map = std::collections::HashMap::new();
                map.insert("vcf_strategy".to_string(), 
                          serde_json::Value::String("quality_precision".to_string()));
                map.insert("anonymize_samples".to_string(),
                          serde_json::Value::Bool(self.anonymize_samples));
                map.insert("preserve_population_structure".to_string(),
                          serde_json::Value::Bool(self.preserve_population_data));
                map
            },
            stealth_level: StealthLevel::High,
            error_correction_level: ECLevel::Medium,
            encryption_key: vec![5, 6, 7, 8], // Use proper key management
            use_gpu: true,
        }
    }
}
```

---

This comprehensive guide covers all aspects of scientific format steganography. The next step would be updating the API reference documentation to include all the new ML and format handling APIs.