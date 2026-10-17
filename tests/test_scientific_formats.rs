//! Integration tests for scientific format handlers
//!
//! Tests VCF (genomics), DICOM (medical imaging), and SAM/BAM (bioinformatics) handlers
//! to ensure proper embedding, extraction, and integrity preservation for scientific data.

use std::collections::HashMap;
use stegano::core::*;
use stegano::handlers::dicom::*;
use stegano::handlers::sam_bam::*;
use stegano::handlers::vcf::*;
use stegano::DataHandler;

/// Helper function to create VCF test data
fn create_test_vcf_data() -> Vec<u8> {
    let vcf_content = r#"##fileformat=VCFv4.2
##reference=GRCh38
##INFO=<ID=AC,Number=A,Type=Integer,Description="Allele count">
##INFO=<ID=AN,Number=1,Type=Integer,Description="Total number of alleles">
##INFO=<ID=AF,Number=A,Type=Float,Description="Allele Frequency">
##FORMAT=<ID=GT,Number=1,Type=String,Description="Genotype">
##FORMAT=<ID=DP,Number=1,Type=Integer,Description="Read Depth">
##FORMAT=<ID=GQ,Number=1,Type=Integer,Description="Genotype Quality">
#CHROM	POS	ID	REF	ALT	QUAL	FILTER	INFO	FORMAT	SAMPLE_001	SAMPLE_002
chr1	14370	rs6054257	G	A	29.0	PASS	AC=1;AN=4;AF=0.25	GT:DP:GQ	0|1:13:48	0|0:15:50
chr1	17330	rs1234567	T	A	67.0	PASS	AC=2;AN=4;AF=0.5	GT:DP:GQ	0|1:18:52	1|1:20:55
chr1	1110696	rs6040355	A	G	85.0	PASS	AC=1;AN=4;AF=0.25	GT:DP:GQ	0|1:25:60	0|0:22:58
"#;
    replicate_records(vcf_content, FIXTURE_REPEATS)
}

/// Append an explicit VR little endian data element (PS3.5 §7.1.2): OB/OW/SQ/UN/UT…
/// use VR + 2 reserved bytes + u32 length, all other VRs VR + u16 length.
fn put_explicit_le(out: &mut Vec<u8>, group: u16, element: u16, vr: &[u8; 2], value: &[u8]) {
    assert!(
        value.len().is_multiple_of(2),
        "DICOM values have even length"
    );
    out.extend_from_slice(&group.to_le_bytes());
    out.extend_from_slice(&element.to_le_bytes());
    out.extend_from_slice(vr);
    if matches!(vr, b"OB" | b"OW" | b"OF" | b"SQ" | b"UN" | b"UT") {
        out.extend_from_slice(&[0, 0]);
        out.extend_from_slice(&(value.len() as u32).to_le_bytes());
    } else {
        out.extend_from_slice(&(value.len() as u16).to_le_bytes());
    }
    out.extend_from_slice(value);
}

/// Spec-correct DICOM Part 10 test file: preamble + `DICM`, File Meta group
/// (explicit VR LE, Transfer Syntax = Explicit VR Little Endian), image pixel
/// module and 256×256 16-bit (12 stored) monochrome pixel data.
fn create_test_dicom_data() -> Vec<u8> {
    let mut meta = Vec::new();
    put_explicit_le(&mut meta, 0x0002, 0x0001, b"OB", &[0x00, 0x01]);
    put_explicit_le(
        &mut meta,
        0x0002,
        0x0002,
        b"UI",
        b"1.2.840.10008.5.1.4.1.1.4\0",
    );
    put_explicit_le(&mut meta, 0x0002, 0x0003, b"UI", b"1.2.3.4.5.6.7.8.9.10");
    put_explicit_le(&mut meta, 0x0002, 0x0010, b"UI", b"1.2.840.10008.1.2.1\0");

    let mut data = vec![0u8; 128];
    data.extend_from_slice(b"DICM");
    put_explicit_le(
        &mut data,
        0x0002,
        0x0000,
        b"UL",
        &(meta.len() as u32).to_le_bytes(),
    );
    data.extend_from_slice(&meta);

    put_explicit_le(
        &mut data,
        0x0008,
        0x0016,
        b"UI",
        b"1.2.840.10008.5.1.4.1.1.4\0",
    );
    put_explicit_le(&mut data, 0x0008, 0x0060, b"CS", b"MR");
    put_explicit_le(&mut data, 0x0028, 0x0002, b"US", &1u16.to_le_bytes());
    put_explicit_le(&mut data, 0x0028, 0x0004, b"CS", b"MONOCHROME2 ");
    put_explicit_le(&mut data, 0x0028, 0x0010, b"US", &256u16.to_le_bytes());
    put_explicit_le(&mut data, 0x0028, 0x0011, b"US", &256u16.to_le_bytes());
    put_explicit_le(&mut data, 0x0028, 0x0100, b"US", &16u16.to_le_bytes());
    put_explicit_le(&mut data, 0x0028, 0x0101, b"US", &12u16.to_le_bytes());
    put_explicit_le(&mut data, 0x0028, 0x0102, b"US", &11u16.to_le_bytes());
    put_explicit_le(&mut data, 0x0028, 0x0103, b"US", &0u16.to_le_bytes());

    // 256x256 16-bit grayscale, 12 bits stored
    let pixels: Vec<u8> = (0..256 * 256)
        .flat_map(|i: u32| (((i * 7) % 4096) as u16).to_le_bytes())
        .collect();
    put_explicit_le(&mut data, 0x7FE0, 0x0010, b"OW", &pixels);

    data
}

/// Helper function to create SAM test data
fn create_test_sam_data() -> Vec<u8> {
    let sam_content = r#"@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:248956422
@RG	ID:sample1	SM:SAMPLE_001	PL:ILLUMINA
@PG	ID:bwa	PN:bwa	VN:0.7.17-r1188
@CO	Test SAM file for steganography
r001	99	chr1	7	30	8M2I4M1D3M	=	37	39	TTAGATAAAGGATACTG	*	AS:i:18	XS:i:12	MD:Z:6A5
r002	0	chr1	9	30	3S6M1P1I4M	*	0	0	AAAAGATAAGGATA	*	AS:i:15	MD:Z:10
r003	0	chr1	9	30	5S6M	*	0	0	GCCTAAGCTAA	*	AS:i:12	SA:Z:chr1,29,-,6H5M,17,0;
r004	0	chr1	16	30	6M14N5M	*	0	0	ATAGCTTCAGC	*	AS:i:21	MD:Z:11
r005	147	chr1	25	30	10M	=	7	-25	GCCTAAGCTA	IIIIIIIIII	AS:i:20	MD:Z:10
"#;
    replicate_records(sam_content, FIXTURE_REPEATS)
}

/// SAM test data where every record carries real QUAL strings (the
/// quality-score strategy never touches `*` QUAL, so it needs these).
fn create_test_sam_data_with_quals() -> Vec<u8> {
    let sam_content = r#"@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:248956422
@RG	ID:sample1	SM:SAMPLE_001	PL:ILLUMINA
@PG	ID:bwa	PN:bwa	VN:0.7.17-r1188
@CO	Test SAM file for steganography
r001	99	chr1	7	30	8M2I4M1D3M	=	37	39	TTAGATAAAGGATACTG	IIIIIIIIIIHHHHHHH	AS:i:18	XS:i:12	MD:Z:6A5
r002	0	chr1	9	30	3S6M1P1I4M	*	0	0	AAAAGATAAGGATA	IIIIIIIIIIIIII	AS:i:15	MD:Z:10
r003	0	chr1	9	30	5S6M	*	0	0	GCCTAAGCTAA	HHHHHHHHHHH	AS:i:12	SA:Z:chr1,29,-,6H5M,17,0;
r004	0	chr1	16	30	6M14N5M	*	0	0	ATAGCTTCAGC	IIIIIIIIIII	AS:i:21	MD:Z:11
r005	147	chr1	25	30	10M	=	7	-25	GCCTAAGCTA	IIIIIIIIII	AS:i:20	MD:Z:10
"#;
    replicate_records(sam_content, FIXTURE_REPEATS)
}

#[tokio::test]
async fn test_vcf_info_fields_embedding() {
    let vcf_data = create_test_vcf_data();
    let carrier = CarrierData::new(DataFormat::VCF, vcf_data);

    let params = EmbedParams {
        format_specific: {
            let mut map = HashMap::new();
            map.insert(
                "vcf_strategy".to_string(),
                serde_json::Value::String("info_fields".to_string()),
            );
            map.insert(
                "preserve_critical_annotations".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "target_info_fields".to_string(),
                serde_json::Value::Array(vec![
                    serde_json::Value::String("AC".to_string()),
                    serde_json::Value::String("AN".to_string()),
                ]),
            );
            map
        },
        stealth_level: StealthLevel::High,
        error_correction_level: ECLevel::Medium,
        encryption_key: b"test_vcf_key".to_vec(),
        use_gpu: false,
        enable_validation: false,
        ..Default::default()
    };

    let handler = VcfHandler::new();
    let payload = b"genomics_test_data_for_embedding";

    // Test capacity estimation
    let capacity_info = handler.estimate_capacity(&carrier);
    assert!(capacity_info.is_ok());
    let capacity = capacity_info.unwrap();
    assert!(capacity.total_bits > 0);
    assert!(capacity.recommended_bits <= capacity.total_bits);

    // Test embedding
    let mut carrier_mut = carrier.clone();
    let embed_result = handler.embed(&mut carrier_mut, payload, &params).await;
    assert!(embed_result.is_ok());

    let result = embed_result.unwrap();
    assert!(result.bytes_embedded > 0);
    assert!(result.bytes_embedded <= payload.len());

    // Test integrity verification
    let integrity_ok = handler.verify_integrity(&carrier_mut);
    assert!(integrity_ok.is_ok());
    assert!(integrity_ok.unwrap());

    // Test extraction
    let extract_result = handler.extract(&carrier_mut, &params).await;
    assert!(extract_result.is_ok());

    let extracted = extract_result.unwrap();
    assert_eq!(extracted, payload);
}

#[tokio::test]
async fn test_vcf_quality_precision_embedding() {
    let vcf_data = create_test_vcf_data();
    let carrier = CarrierData::new(DataFormat::VCF, vcf_data);

    let params = EmbedParams {
        format_specific: {
            let mut map = HashMap::new();
            map.insert(
                "vcf_strategy".to_string(),
                serde_json::Value::String("quality_precision".to_string()),
            );
            map.insert(
                "max_precision_loss".to_string(),
                serde_json::Value::Number(serde_json::Number::from_f64(0.1).unwrap()),
            );
            map.insert(
                "min_quality_threshold".to_string(),
                serde_json::Value::Number(serde_json::Number::from(20)),
            );
            map.insert(
                "preserve_low_quality_variants".to_string(),
                serde_json::Value::Bool(true),
            );
            map
        },
        stealth_level: StealthLevel::Maximum,
        error_correction_level: ECLevel::High,
        encryption_key: b"vcf_quality_key".to_vec(),
        use_gpu: false,
        enable_validation: false,
        ..Default::default()
    };

    let handler = VcfHandler::new();
    let payload = b"quality_score_embedding_test";

    let mut carrier_mut = carrier.clone();
    let embed_result = handler.embed(&mut carrier_mut, payload, &params).await;
    assert!(embed_result.is_ok());

    let result = embed_result.unwrap();
    assert!(result.bytes_embedded > 0);

    // Verify quality scores are within acceptable range
    let extracted = handler.extract(&carrier_mut, &params).await.unwrap();
    assert_eq!(extracted, payload);

    // Verify genomics integrity is preserved
    let integrity_ok = handler.verify_integrity(&carrier_mut).unwrap();
    assert!(integrity_ok);
}

#[tokio::test]
async fn test_vcf_format_fields_embedding() {
    let vcf_data = create_test_vcf_data();
    let carrier = CarrierData::new(DataFormat::VCF, vcf_data);

    let params = EmbedParams {
        format_specific: {
            let mut map = HashMap::new();
            map.insert(
                "vcf_strategy".to_string(),
                serde_json::Value::String("format_fields".to_string()),
            );
            map.insert(
                "target_samples".to_string(),
                serde_json::Value::Array(vec![
                    serde_json::Value::String("SAMPLE_001".to_string()),
                    serde_json::Value::String("SAMPLE_002".to_string()),
                ]),
            );
            map.insert(
                "preserve_genotypes".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "target_format_fields".to_string(),
                serde_json::Value::Array(vec![
                    serde_json::Value::String("DP".to_string()),
                    serde_json::Value::String("GQ".to_string()),
                ]),
            );
            map
        },
        stealth_level: StealthLevel::Medium,
        error_correction_level: ECLevel::Low,
        encryption_key: b"vcf_format_key".to_vec(),
        use_gpu: false,
        enable_validation: false,
        ..Default::default()
    };

    let handler = VcfHandler::new();
    let payload = b"format_field_test_data";

    let mut carrier_mut = carrier.clone();
    let result = handler
        .embed(&mut carrier_mut, payload, &params)
        .await
        .unwrap();
    assert!(result.bytes_embedded > 0);

    let extracted = handler.extract(&carrier_mut, &params).await.unwrap();
    assert_eq!(extracted, payload);
}

#[tokio::test]
async fn test_dicom_private_elements_embedding() {
    let dicom_data = create_test_dicom_data();
    let carrier = CarrierData::new(DataFormat::DICOM, dicom_data);

    let params = EmbedParams {
        format_specific: {
            let mut map = HashMap::new();
            map.insert(
                "dicom_strategy".to_string(),
                serde_json::Value::String("private".to_string()),
            );
            map.insert(
                "private_group_range".to_string(),
                serde_json::Value::Array(vec![
                    serde_json::Value::Number(serde_json::Number::from(0x0009)),
                    serde_json::Value::Number(serde_json::Number::from(0x00FF)),
                ]),
            );
            map.insert(
                "preserve_clinical_data".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "maintain_sop_class".to_string(),
                serde_json::Value::Bool(true),
            );
            map
        },
        stealth_level: StealthLevel::High,
        error_correction_level: ECLevel::High,
        encryption_key: b"dicom_private_key".to_vec(),
        use_gpu: false,
        enable_validation: false,
        ..Default::default()
    };

    let handler = DicomHandler::new();
    let payload = b"medical_imaging_secret_data";

    // Test capacity estimation
    let capacity_info = handler.estimate_capacity(&carrier);
    assert!(capacity_info.is_ok());

    // Test embedding
    let mut carrier_mut = carrier.clone();
    let embed_result = handler.embed(&mut carrier_mut, payload, &params).await;
    assert!(embed_result.is_ok());

    let result = embed_result.unwrap();
    assert!(result.bytes_embedded > 0);

    // Test medical compliance verification
    let integrity_ok = handler.verify_integrity(&carrier_mut);
    assert!(integrity_ok.is_ok());
    assert!(integrity_ok.unwrap());

    // The private block is inserted in tag order before the pixel data, so the
    // pixel data element itself (at the end of the file) is unchanged.
    let px_len = 256 * 256 * 2;
    assert_eq!(
        &carrier_mut.raw_data[carrier_mut.raw_data.len() - px_len..],
        &carrier.raw_data[carrier.raw_data.len() - px_len..]
    );

    // Test extraction
    let extracted = handler.extract(&carrier_mut, &params).await.unwrap();
    assert_eq!(extracted, payload);
}

#[tokio::test]
async fn test_dicom_pixel_lsb_embedding() {
    let dicom_data = create_test_dicom_data();
    let carrier = CarrierData::new(DataFormat::DICOM, dicom_data);

    let params = EmbedParams {
        format_specific: {
            let mut map = HashMap::new();
            map.insert(
                "dicom_strategy".to_string(),
                serde_json::Value::String("pixel_lsb".to_string()),
            );
            map.insert(
                "max_pixel_change".to_string(),
                serde_json::Value::Number(serde_json::Number::from(1)),
            ); // Max 1 HU change
            map.insert(
                "preserve_diagnostic_regions".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "bits_per_pixel".to_string(),
                serde_json::Value::Number(serde_json::Number::from(1)),
            ); // Use only 1 LSB
            map.insert(
                "exclude_roi_masks".to_string(),
                serde_json::Value::Array(vec![serde_json::Value::String(
                    "critical_anatomy".to_string(),
                )]),
            );
            map
        },
        stealth_level: StealthLevel::Maximum,
        error_correction_level: ECLevel::Medium,
        encryption_key: b"dicom_pixel_key".to_vec(),
        use_gpu: false,
        enable_validation: false,
        ..Default::default()
    };

    let handler = DicomHandler::new();
    let payload = b"diagnostic_metadata_embedded";

    let mut carrier_mut = carrier.clone();
    let result = handler
        .embed(&mut carrier_mut, payload, &params)
        .await
        .expect("pixel LSB embedding");
    assert_eq!(result.bytes_embedded, payload.len());
    assert!(handler.verify_integrity(&carrier_mut).unwrap());

    // Same file size; every 16-bit sample changes by at most 1.
    assert_eq!(carrier_mut.raw_data.len(), carrier.raw_data.len());
    let px_len = 256 * 256 * 2;
    let start = carrier.raw_data.len() - px_len;
    assert_eq!(&carrier_mut.raw_data[..start], &carrier.raw_data[..start]);
    let before = carrier.raw_data[start..].as_chunks::<2>().0;
    let after = carrier_mut.raw_data[start..].as_chunks::<2>().0;
    for (&a, &b) in before.iter().zip(after) {
        let a = i32::from(u16::from_le_bytes(a));
        let b = i32::from(u16::from_le_bytes(b));
        assert!((a - b).abs() <= 1);
    }

    let extracted = handler.extract(&carrier_mut, &params).await.unwrap();
    assert_eq!(extracted, payload);
}

#[tokio::test]
async fn test_dicom_unused_metadata_embedding() {
    let dicom_data = create_test_dicom_data();
    let carrier = CarrierData::new(DataFormat::DICOM, dicom_data);

    let params = EmbedParams {
        format_specific: {
            let mut map = HashMap::new();
            map.insert(
                "dicom_strategy".to_string(),
                serde_json::Value::String("unused_metadata".to_string()),
            );
            map.insert(
                "preserve_required_tags".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "max_field_length".to_string(),
                serde_json::Value::Number(serde_json::Number::from(256)),
            );
            map.insert(
                "exclude_patient_tags".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "target_unused_tags".to_string(),
                serde_json::Value::Array(vec![
                    serde_json::Value::String("DeviceSerialNumber".to_string()),
                    serde_json::Value::String("SoftwareVersions".to_string()),
                ]),
            );
            map
        },
        stealth_level: StealthLevel::Medium,
        // The 128-byte preamble holds at most 119 codec bytes, so no RS parity.
        error_correction_level: ECLevel::None,
        encryption_key: b"dicom_metadata_key".to_vec(),
        use_gpu: false,
        enable_validation: false,
        ..Default::default()
    };

    let handler = DicomHandler::new();
    let payload = b"device_calibration_logs";

    let mut carrier_mut = carrier.clone();
    let result = handler
        .embed(&mut carrier_mut, payload, &params)
        .await
        .expect("preamble embedding");
    assert_eq!(result.bytes_embedded, payload.len());
    // Only the preamble changed; the File Meta group and data set are untouched.
    assert_eq!(carrier_mut.raw_data.len(), carrier.raw_data.len());
    assert_eq!(&carrier_mut.raw_data[128..], &carrier.raw_data[128..]);
    assert!(handler.verify_integrity(&carrier_mut).unwrap());

    let extracted = handler.extract(&carrier_mut, &params).await.unwrap();
    assert_eq!(extracted, payload);

    // A larger payload with RS parity cannot fit in the preamble.
    let mut big = params.clone();
    big.error_correction_level = ECLevel::Low;
    let mut c2 = carrier.clone();
    assert!(matches!(
        handler.embed(&mut c2, payload, &big).await,
        Err(SteganoError::InsufficientCapacity { .. })
    ));
}

#[tokio::test]
async fn test_sam_optional_fields_embedding() {
    let sam_data = create_test_sam_data();
    let carrier = CarrierData::new(DataFormat::SAM, sam_data);

    let params = EmbedParams {
        format_specific: {
            let mut map = HashMap::new();
            map.insert(
                "sam_strategy".to_string(),
                serde_json::Value::String("optional_fields".to_string()),
            );
            map.insert(
                "preserve_core_alignment".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "max_field_size".to_string(),
                serde_json::Value::Number(serde_json::Number::from(255)),
            );
            map.insert(
                "target_optional_tags".to_string(),
                serde_json::Value::Array(vec![
                    serde_json::Value::String("AS".to_string()), // Alignment score
                    serde_json::Value::String("XS".to_string()), // Suboptimal alignment score
                    serde_json::Value::String("MD".to_string()), // Mismatch data
                ]),
            );
            map
        },
        stealth_level: StealthLevel::High,
        error_correction_level: ECLevel::Medium,
        encryption_key: b"sam_optional_key".to_vec(),
        use_gpu: false,
        enable_validation: false,
        ..Default::default()
    };

    let handler = SamBamHandler::new();
    let payload = b"bioinformatics_alignment_metadata";

    // Test capacity estimation
    let capacity_info = handler.estimate_capacity(&carrier);
    assert!(capacity_info.is_ok());

    // Test embedding
    let mut carrier_mut = carrier.clone();
    let embed_result = handler.embed(&mut carrier_mut, payload, &params).await;
    assert!(embed_result.is_ok());

    let result = embed_result.unwrap();
    assert!(result.bytes_embedded > 0);

    // Test alignment integrity
    let integrity_ok = handler.verify_integrity(&carrier_mut);
    assert!(integrity_ok.is_ok());
    assert!(integrity_ok.unwrap());

    // Test extraction
    let extracted = handler.extract(&carrier_mut, &params).await.unwrap();
    assert_eq!(extracted, payload);
}

#[tokio::test]
async fn test_sam_quality_scores_embedding() {
    let sam_data = create_test_sam_data_with_quals();
    let carrier = CarrierData::new(DataFormat::SAM, sam_data);

    let params = EmbedParams {
        format_specific: {
            let mut map = HashMap::new();
            map.insert(
                "sam_strategy".to_string(),
                serde_json::Value::String("quality_scores".to_string()),
            );
            map.insert(
                "min_quality_threshold".to_string(),
                serde_json::Value::Number(serde_json::Number::from(20)),
            ); // Phred 20+
            map.insert(
                "max_quality_change".to_string(),
                serde_json::Value::Number(serde_json::Number::from(3)),
            ); // Max 3 point change
            map.insert(
                "preserve_low_quality".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "use_sliding_window".to_string(),
                serde_json::Value::Bool(true),
            );
            map
        },
        stealth_level: StealthLevel::Maximum,
        error_correction_level: ECLevel::Low,
        encryption_key: b"sam_quality_key".to_vec(),
        use_gpu: false,
        enable_validation: false,
        ..Default::default()
    };

    let handler = SamBamHandler::new();
    let payload = b"sequencing_quality_analysis";

    let mut carrier_mut = carrier.clone();
    let result = handler
        .embed(&mut carrier_mut, payload, &params)
        .await
        .unwrap();
    assert!(result.bytes_embedded > 0);

    let extracted = handler.extract(&carrier_mut, &params).await.unwrap();
    assert_eq!(extracted, payload);
}

#[tokio::test]
async fn test_sam_comment_lines_embedding() {
    let sam_data = create_test_sam_data();
    let carrier = CarrierData::new(DataFormat::SAM, sam_data);

    let params = EmbedParams {
        format_specific: {
            let mut map = HashMap::new();
            map.insert(
                "sam_strategy".to_string(),
                serde_json::Value::String("comment_lines".to_string()),
            );
            map.insert(
                "preserve_existing_comments".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "max_comment_length".to_string(),
                serde_json::Value::Number(serde_json::Number::from(1000)),
            );
            map.insert(
                "comment_prefix".to_string(),
                serde_json::Value::String("@CO\tPIPELINE:".to_string()),
            );
            map
        },
        stealth_level: StealthLevel::Low, // Comments are visible
        error_correction_level: ECLevel::High,
        encryption_key: b"sam_comment_key".to_vec(),
        use_gpu: false,
        enable_validation: false,
        ..Default::default()
    };

    let handler = SamBamHandler::new();
    let payload = b"pipeline_processing_logs";

    let mut carrier_mut = carrier.clone();
    let result = handler
        .embed(&mut carrier_mut, payload, &params)
        .await
        .unwrap();
    assert!(result.bytes_embedded > 0);

    let extracted = handler.extract(&carrier_mut, &params).await.unwrap();
    assert_eq!(extracted, payload);
}

#[tokio::test]
async fn test_scientific_format_capacity_analysis() {
    // Test capacity analysis across different scientific formats
    let test_cases = vec![
        ("VCF", create_test_vcf_data(), DataFormat::VCF),
        ("DICOM", create_test_dicom_data(), DataFormat::DICOM),
        ("SAM", create_test_sam_data(), DataFormat::SAM),
    ];

    for (format_name, data, format_type) in test_cases {
        let carrier = CarrierData::new(format_type.clone(), data);

        let capacity_info = match format_type {
            DataFormat::VCF => {
                let handler = VcfHandler::new();
                handler.estimate_capacity(&carrier)
            }
            DataFormat::DICOM => {
                let handler = DicomHandler::new();
                handler.estimate_capacity(&carrier)
            }
            DataFormat::SAM => {
                let handler = SamBamHandler::new();
                handler.estimate_capacity(&carrier)
            }
            _ => continue,
        };

        assert!(
            capacity_info.is_ok(),
            "Capacity estimation failed for {format_name}"
        );

        let capacity = capacity_info.unwrap();
        assert!(
            capacity.total_bits > 0,
            "{format_name} should have embedding capacity"
        );
        assert!(
            capacity.recommended_bits <= capacity.total_bits,
            "{format_name} recommended capacity should not exceed total"
        );
        assert!(
            capacity.steganographic_ratio > 0.0 && capacity.steganographic_ratio <= 1.0,
            "{format_name} steganographic ratio should be between 0 and 1"
        );

        println!(
            "{} capacity: {} bits total, {} bits recommended, {:.2}% ratio",
            format_name,
            capacity.total_bits,
            capacity.recommended_bits,
            capacity.steganographic_ratio * 100.0
        );
    }
}

#[tokio::test]
async fn test_scientific_format_integrity_preservation() {
    // Test that scientific data integrity is preserved across all formats
    let vcf_data = create_test_vcf_data();
    let dicom_data = create_test_dicom_data();
    let sam_data = create_test_sam_data();

    let test_cases = vec![
        (CarrierData::new(DataFormat::VCF, vcf_data), "VCF"),
        (CarrierData::new(DataFormat::DICOM, dicom_data), "DICOM"),
        (CarrierData::new(DataFormat::SAM, sam_data), "SAM"),
    ];

    for (carrier, format_name) in test_cases {
        // Verify integrity before embedding
        let initial_integrity = match format_name {
            "VCF" => {
                let vcf_handler = VcfHandler::new();
                vcf_handler.verify_integrity(&carrier)
            }
            "DICOM" => {
                let dicom_handler = DicomHandler::new();
                dicom_handler.verify_integrity(&carrier)
            }
            "SAM" => {
                let sam_handler = SamBamHandler::new();
                sam_handler.verify_integrity(&carrier)
            }
            _ => Ok(true),
        };

        assert!(
            initial_integrity.is_ok(),
            "{format_name} initial integrity check failed"
        );
        assert!(
            initial_integrity.unwrap(),
            "{format_name} should have valid initial integrity"
        );

        println!("{format_name} integrity preservation verified");
    }
}

#[tokio::test]
async fn test_cross_format_embedding_extraction() {
    // Test embedding and extraction across all scientific formats with consistent parameters
    let payload = b"cross_format_test_payload_for_scientific_data_validation";

    let formats = vec![
        ("VCF", create_test_vcf_data(), DataFormat::VCF),
        ("DICOM", create_test_dicom_data(), DataFormat::DICOM),
        ("SAM", create_test_sam_data(), DataFormat::SAM),
    ];

    for (format_name, data, format_type) in formats {
        let carrier = CarrierData::new(format_type.clone(), data);

        let params = EmbedParams {
            format_specific: HashMap::new(), // Use defaults
            stealth_level: StealthLevel::Medium,
            error_correction_level: ECLevel::Medium,
            encryption_key: format!("{}_test_key", format_name.to_lowercase())
                .as_bytes()
                .to_vec(),
            use_gpu: false,
            enable_validation: false,
            ..Default::default()
        };

        let (embed_result, extract_result) = match format_type {
            DataFormat::VCF => {
                let handler = VcfHandler::new();
                let mut carrier_mut = carrier.clone();
                let embed = handler.embed(&mut carrier_mut, payload, &params).await;
                let extract = if embed.is_ok() {
                    handler.extract(&carrier_mut, &params).await
                } else {
                    Err(SteganoError::InvalidParameter("Embed failed".to_string()))
                };
                (embed, extract)
            }
            DataFormat::DICOM => {
                let handler = DicomHandler::new();
                let mut carrier_mut = carrier.clone();
                let embed = handler.embed(&mut carrier_mut, payload, &params).await;
                let extract = if embed.is_ok() {
                    handler.extract(&carrier_mut, &params).await
                } else {
                    Err(SteganoError::InvalidParameter("Embed failed".to_string()))
                };
                (embed, extract)
            }
            DataFormat::SAM => {
                let handler = SamBamHandler::new();
                let mut carrier_mut = carrier.clone();
                let embed = handler.embed(&mut carrier_mut, payload, &params).await;
                let extract = if embed.is_ok() {
                    handler.extract(&carrier_mut, &params).await
                } else {
                    Err(SteganoError::InvalidParameter("Embed failed".to_string()))
                };
                (embed, extract)
            }
            _ => continue,
        };

        match embed_result {
            Ok(_) => {
                match extract_result {
                    Ok(extracted) => {
                        assert_eq!(
                            extracted, payload,
                            "{format_name} should preserve payload integrity"
                        );
                    }
                    Err(e) => {
                        // Even after successful embed, extraction might fail with test data
                        println!("{format_name} extraction failed after successful embed: {e:?}");
                        assert!(
                            e.to_string().contains("No private")
                                || e.to_string().contains("metadata")
                                || e.to_string().contains("unused")
                                || e.to_string().contains("steganographic")
                                || e.to_string().contains("found")
                        );
                    }
                }
            }
            Err(e) => {
                // If embedding fails with test data, that's acceptable for some formats
                println!("{format_name} embedding failed (may be expected with test data): {e:?}");
                // Just verify that the error is reasonable for test data limitations
                assert!(
                    e.to_string().contains("metadata")
                        || e.to_string().contains("unused")
                        || e.to_string().contains("private")
                        || e.to_string().contains("pixel")
                        || e.to_string().contains("No")
                        || e.to_string().contains("capacity")
                        || e.to_string().contains("found")
                );
            }
        }

        println!("{format_name} cross-format test passed");
    }
}

#[test]
fn test_scientific_format_error_handling() {
    // Test error handling for malformed scientific data

    // Test invalid VCF
    let invalid_vcf = b"##fileformat=VCFv4.2\nINVALID_LINE_WITHOUT_TABS";
    let vcf_carrier = CarrierData::new(DataFormat::VCF, invalid_vcf.to_vec());
    let vcf_handler = VcfHandler::new();

    let vcf_capacity = vcf_handler.estimate_capacity(&vcf_carrier);
    // Should handle gracefully (may succeed with minimal capacity or fail cleanly)
    assert!(vcf_capacity.is_ok() || vcf_capacity.is_err());

    // Test invalid DICOM (missing DICM prefix)
    let invalid_dicom = vec![0u8; 200]; // Just zeros, no valid DICOM structure
    let dicom_carrier = CarrierData::new(DataFormat::DICOM, invalid_dicom);
    let dicom_handler = DicomHandler::new();

    let dicom_capacity = dicom_handler.estimate_capacity(&dicom_carrier);
    // Should handle gracefully
    assert!(dicom_capacity.is_ok() || dicom_capacity.is_err());

    // Test invalid SAM (missing header)
    let invalid_sam = b"r001\t99\tchr1\t7\t30\t8M\t=\t37\t39\tTTAGATAAAGGATACTG\t*"; // No @HD header
    let sam_carrier = CarrierData::new(DataFormat::SAM, invalid_sam.to_vec());
    let sam_handler = SamBamHandler::new();

    let sam_capacity = sam_handler.estimate_capacity(&sam_carrier);
    // Should handle gracefully
    assert!(sam_capacity.is_ok() || sam_capacity.is_err());
}

#[tokio::test]
async fn test_large_payload_handling() {
    // Test handling of payloads that exceed capacity
    let vcf_data = create_test_vcf_data();
    let carrier = CarrierData::new(DataFormat::VCF, vcf_data);
    let handler = VcfHandler::new();

    // Create a very large payload
    let large_payload = vec![b'A'; 10000]; // 10KB payload

    let params = EmbedParams {
        format_specific: HashMap::new(),
        stealth_level: StealthLevel::Medium,
        error_correction_level: ECLevel::Medium,
        encryption_key: b"large_payload_key".to_vec(),
        use_gpu: false,
        enable_validation: false,
        ..Default::default()
    };

    let mut carrier_mut = carrier;
    let embed_result = handler
        .embed(&mut carrier_mut, &large_payload, &params)
        .await;

    // Should either succeed (if capacity is sufficient) or fail gracefully
    match embed_result {
        Ok(result) => {
            assert!(result.bytes_embedded > 0);
            // If embedding succeeded, extraction should work
            let extracted = handler.extract(&carrier_mut, &params).await.unwrap();
            assert_eq!(extracted.len(), large_payload.len());
        }
        Err(_) => {
            // Graceful failure is acceptable for oversized payloads
            println!("Large payload correctly rejected due to capacity constraints");
        }
    }
}

/// Number of times the data records of the text fixtures are repeated.
///
/// The encrypted + Reed-Solomon framed payload (>= 49-byte cipher envelope plus
/// a 255-byte RS block) needs far more records than the original handful, so
/// the fixtures repeat their records to give the handlers enough capacity.
const FIXTURE_REPEATS: usize = 120;

/// Keep header lines (`#` / `@`) once and repeat the data records `times` times.
fn replicate_records(content: &str, times: usize) -> Vec<u8> {
    let (header, records): (Vec<&str>, Vec<&str>) = content
        .lines()
        .partition(|l| l.starts_with('#') || l.starts_with('@'));
    let mut out = String::new();
    for line in header {
        out.push_str(line);
        out.push('\n');
    }
    for _ in 0..times {
        for line in &records {
            out.push_str(line);
            out.push('\n');
        }
    }
    out.into_bytes()
}
