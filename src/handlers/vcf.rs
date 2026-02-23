//! VCF (Variant Call Format) handler for genomics variant data steganography
//!
//! Implements steganography techniques specifically designed for VCF files,
//! which contain genomic variant information including SNPs, indels, and structural variants.

use super::DataHandler;
use crate::core::*;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;

/// Handler for VCF (Variant Call Format) genomics carrier files.
///
/// Embeds payload data in the INFO and FORMAT fields of variant records,
/// preserving the VCF schema so the file remains valid for variant analysis tools.
pub struct VcfHandler;

/// VCF file structure for parsing
#[derive(Debug, Clone)]
pub struct VcfFile {
    /// Header lines starting with ##
    pub header_lines: Vec<String>,

    /// Column header line starting with #CHROM
    pub column_header: String,

    /// Variant records
    pub variants: Vec<VcfVariant>,
}

/// Individual variant record
#[derive(Debug, Clone)]
pub struct VcfVariant {
    /// Chromosome
    pub chrom: String,

    /// Position (1-based)
    pub pos: u64,

    /// Variant ID
    pub id: String,

    /// Reference allele
    pub ref_allele: String,

    /// Alternative allele(s)
    pub alt_alleles: Vec<String>,

    /// Quality score
    pub qual: Option<f32>,

    /// Filter status
    pub filter: String,

    /// INFO field data
    pub info: HashMap<String, String>,

    /// FORMAT field
    pub format: Option<String>,

    /// Sample data
    pub samples: Vec<HashMap<String, String>>,
}

/// VCF embedding strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VcfEmbedStrategy {
    /// Embed in INFO field custom annotations
    InfoFieldSteganography,

    /// Embed in quality score precision
    QualityScoreSteganography,

    /// Embed in sample-specific FORMAT fields
    SampleFormatSteganography,

    /// Embed in variant ID custom suffixes
    VariantIdSteganography,

    /// Embed across multiple strategies for redundancy
    MultiStrategy,
}

/// VCF embedding result
#[derive(Debug, Clone)]
pub struct VcfEmbedResult {
    pub success: bool,
    pub bytes_embedded: usize,
    pub strategy_used: VcfEmbedStrategy,
    pub variants_modified: usize,
    pub info_fields_added: usize,
    pub biological_integrity_preserved: bool,
}

/// Constants for VCF steganography
const VCF_MAGIC_HEADER: &str = "##fileformat=VCF";
const STEGANO_INFO_PREFIX: &str = "STEG";
#[allow(dead_code)]
const MAX_QUAL_PRECISION: u32 = 3; // Maximum decimal places for quality scores

impl Default for VcfHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl VcfHandler {
    pub fn new() -> Self {
        Self
    }

    /// Parse VCF file content
    fn parse_vcf(&self, data: &[u8]) -> Result<VcfFile> {
        let content = String::from_utf8_lossy(data);
        let lines: Vec<&str> = content.lines().collect();

        if lines.is_empty() {
            return Err(SteganoError::IntegrityError("Empty VCF file".to_string()));
        }

        // Verify VCF format
        if !lines[0].starts_with(VCF_MAGIC_HEADER) {
            return Err(SteganoError::IntegrityError(
                "Invalid VCF format - missing header".to_string(),
            ));
        }

        let mut header_lines = Vec::new();
        let mut column_header = String::new();
        let mut variants = Vec::new();
        let mut in_data_section = false;

        for line in lines {
            if line.starts_with("##") {
                header_lines.push(line.to_string());
            } else if line.starts_with("#CHROM") {
                column_header = line.to_string();
                in_data_section = true;
            } else if in_data_section && !line.trim().is_empty() {
                match self.parse_variant_line(line, &column_header) {
                    Ok(variant) => variants.push(variant),
                    Err(e) => {
                        log::warn!("Failed to parse variant line '{line}': {e}");
                    }
                }
            }
        }

        Ok(VcfFile {
            header_lines,
            column_header,
            variants,
        })
    }

    /// Parse individual variant line
    fn parse_variant_line(&self, line: &str, column_header: &str) -> Result<VcfVariant> {
        let fields: Vec<&str> = line.split('\t').collect();
        let header_fields: Vec<&str> = column_header.split('\t').collect();

        if fields.len() < 8 {
            return Err(SteganoError::IntegrityError(
                "Invalid variant line - insufficient fields".to_string(),
            ));
        }

        // Parse basic fields
        let chrom = fields[0].to_string();
        let pos = fields[1]
            .parse::<u64>()
            .map_err(|_| SteganoError::IntegrityError("Invalid position field".to_string()))?;
        let id = fields[2].to_string();
        let ref_allele = fields[3].to_string();
        let alt_alleles: Vec<String> = fields[4].split(',').map(|s| s.to_string()).collect();

        let qual = if fields[5] == "." {
            None
        } else {
            fields[5].parse::<f32>().ok()
        };

        let filter = fields[6].to_string();

        // Parse INFO field
        let info = self.parse_info_field(fields[7])?;

        // Parse FORMAT and sample data if present
        let (format, samples) = if fields.len() > 9 && !fields[8].is_empty() {
            let format_str = fields[8].to_string();
            let mut samples = Vec::new();

            // Parse sample columns
            for (sample_field, header_field) in
                fields[9..].iter().zip(header_fields[9..].iter())
            {
                let sample_data = self.parse_sample_data(&format_str, sample_field)?;
                let _ = header_field; // zip ensures we stay within header bounds
                samples.push(sample_data);
            }

            (Some(format_str), samples)
        } else {
            (None, Vec::new())
        };

        Ok(VcfVariant {
            chrom,
            pos,
            id,
            ref_allele,
            alt_alleles,
            qual,
            filter,
            info,
            format,
            samples,
        })
    }

    /// Parse INFO field into key-value pairs
    fn parse_info_field(&self, info_str: &str) -> Result<HashMap<String, String>> {
        let mut info = HashMap::new();

        if info_str == "." {
            return Ok(info);
        }

        for pair in info_str.split(';') {
            if let Some(eq_pos) = pair.find('=') {
                let key = pair[..eq_pos].to_string();
                let value = pair[eq_pos + 1..].to_string();
                info.insert(key, value);
            } else {
                // Flag field (no value)
                info.insert(pair.to_string(), "".to_string());
            }
        }

        Ok(info)
    }

    /// Parse sample data based on FORMAT field
    fn parse_sample_data(
        &self,
        format_str: &str,
        sample_str: &str,
    ) -> Result<HashMap<String, String>> {
        let mut sample_data = HashMap::new();
        let format_fields: Vec<&str> = format_str.split(':').collect();
        let sample_values: Vec<&str> = sample_str.split(':').collect();

        for (i, &field) in format_fields.iter().enumerate() {
            let value = sample_values.get(i).unwrap_or(&".").to_string();
            sample_data.insert(field.to_string(), value);
        }

        Ok(sample_data)
    }

    /// Embed payload using INFO field steganography
    fn embed_info_field_strategy(
        &self,
        vcf: &mut VcfFile,
        payload: &[u8],
    ) -> Result<VcfEmbedResult> {
        // Create a payload with length header
        let mut full_payload = Vec::new();
        let payload_len = payload.len() as u32;
        full_payload.extend_from_slice(&payload_len.to_le_bytes());
        full_payload.extend_from_slice(payload);

        // Convert to hex for embedding in INFO fields
        let hex_payload = hex::encode(full_payload);

        // Calculate chunks to distribute across variants
        let chunk_size = 32; // 32 hex characters per variant
        let chunks: Vec<&str> = hex_payload
            .as_bytes()
            .chunks(chunk_size)
            .map(|chunk| std::str::from_utf8(chunk).unwrap_or(""))
            .collect();

        if chunks.len() > vcf.variants.len() {
            return Err(SteganoError::InsufficientCapacity {
                needed: chunks.len(),
                available: vcf.variants.len(),
            });
        }

        let mut variants_modified = 0;
        let mut info_fields_added = 0;

        // Embed chunks in INFO fields
        for (i, chunk) in chunks.iter().enumerate() {
            if i < vcf.variants.len() && !chunk.is_empty() {
                let info_key = format!("{STEGANO_INFO_PREFIX}_{i:04}");
                vcf.variants[i].info.insert(info_key, chunk.to_string());
                variants_modified += 1;
                info_fields_added += 1;
            }
        }

        Ok(VcfEmbedResult {
            success: true,
            bytes_embedded: payload.len(),
            strategy_used: VcfEmbedStrategy::InfoFieldSteganography,
            variants_modified,
            info_fields_added,
            biological_integrity_preserved: true, // INFO fields don't affect biology
        })
    }

    /// Extract payload from INFO field steganography
    fn extract_info_field_strategy(&self, vcf: &VcfFile) -> Result<Vec<u8>> {
        let mut hex_chunks = Vec::new();

        // Collect steganographic INFO fields in order
        for variant in &vcf.variants {
            for i in 0..10000 {
                // Reasonable limit
                let info_key = format!("{STEGANO_INFO_PREFIX}_{i:04}");
                if let Some(chunk) = variant.info.get(&info_key) {
                    hex_chunks.push((i, chunk.clone()));
                }
            }
        }

        if hex_chunks.is_empty() {
            return Err(SteganoError::IntegrityError(
                "No steganographic data found in INFO fields".to_string(),
            ));
        }

        // Sort by index to reconstruct original order
        hex_chunks.sort_by_key(|(index, _)| *index);

        // Concatenate hex chunks
        let hex_payload: String = hex_chunks.into_iter().map(|(_, chunk)| chunk).collect();

        // Decode hex to bytes
        let full_payload = hex::decode(hex_payload).map_err(|_| {
            SteganoError::IntegrityError("Invalid hex data in INFO fields".to_string())
        })?;

        // Extract payload length from first 4 bytes
        if full_payload.len() < 4 {
            return Err(SteganoError::IntegrityError(
                "Insufficient data to extract payload length".to_string(),
            ));
        }

        let payload_len = u32::from_le_bytes([
            full_payload[0],
            full_payload[1],
            full_payload[2],
            full_payload[3],
        ]) as usize;

        // Validate payload length
        if payload_len + 4 > full_payload.len() {
            return Err(SteganoError::IntegrityError(
                "Invalid payload length in extracted data".to_string(),
            ));
        }

        // Extract the actual payload
        let payload = full_payload[4..4 + payload_len].to_vec();

        Ok(payload)
    }

    /// Embed payload using quality score steganography (self-framing: 4-byte length header)
    fn embed_quality_strategy(&self, vcf: &mut VcfFile, payload: &[u8]) -> Result<VcfEmbedResult> {
        // Prepend 4-byte LE length header so extraction is self-framing
        let len_bytes = (payload.len() as u32).to_le_bytes();
        let framed: Vec<u8> = len_bytes.iter().chain(payload.iter()).copied().collect();

        // Convert framed payload to bits (LSB-first per byte)
        let mut payload_bits = Vec::new();
        for byte in &framed {
            for i in 0..8 {
                payload_bits.push((byte >> i) & 1);
            }
        }

        // Find variants with quality scores
        let available_variants: Vec<usize> = vcf
            .variants
            .iter()
            .enumerate()
            .filter_map(|(i, var)| if var.qual.is_some() { Some(i) } else { None })
            .collect();

        if available_variants.len() * 3 < payload_bits.len() {
            return Err(SteganoError::InsufficientCapacity {
                needed: payload_bits.len(),
                available: available_variants.len() * 3,
            });
        }

        let mut bit_index = 0;
        let mut variants_modified = 0;

        // Embed 3 bits per variant by storing them in the low 3 bits of the integer quality score.
        // This avoids floating-point decimal-precision issues: qual is stored as floor(qual/8)*8 + 3-bit_value.
        for &var_idx in &available_variants {
            if bit_index >= payload_bits.len() {
                break;
            }

            if let Some(qual) = vcf.variants[var_idx].qual {
                let integer_qual = qual.round() as u32;
                // Pack up to 3 bits into the low 3 bits
                let b0 = if bit_index < payload_bits.len() { let b = payload_bits[bit_index]; bit_index += 1; b } else { 0 };
                let b1 = if bit_index < payload_bits.len() { let b = payload_bits[bit_index]; bit_index += 1; b } else { 0 };
                let b2 = if bit_index < payload_bits.len() { let b = payload_bits[bit_index]; bit_index += 1; b } else { 0 };
                let triplet = b0 as u32 | ((b1 as u32) << 1) | ((b2 as u32) << 2);
                // Store: clear low 3 bits, set to triplet
                let new_qual = (integer_qual & !7) | (triplet & 7);
                vcf.variants[var_idx].qual = Some(new_qual as f32);
                variants_modified += 1;
            }
        }

        Ok(VcfEmbedResult {
            success: bit_index >= payload_bits.len(),
            bytes_embedded: payload.len(),
            strategy_used: VcfEmbedStrategy::QualityScoreSteganography,
            variants_modified,
            info_fields_added: 0,
            biological_integrity_preserved: true, // Quality score precision doesn't affect biology
        })
    }

    /// Extract payload from quality score steganography (self-framing: reads 4-byte length header)
    fn extract_quality_strategy(&self, vcf: &VcfFile) -> Result<Vec<u8>> {
        // Collect all bits from low 3 bits of integer quality scores
        let all_bits: Vec<u8> = vcf
            .variants
            .iter()
            .filter_map(|v| v.qual)
            .flat_map(|qual| {
                let t = qual.round() as u32 & 7;
                [(t & 1) as u8, ((t >> 1) & 1) as u8, ((t >> 2) & 1) as u8]
            })
            .collect();

        // Need at least 32 bits for the 4-byte length header
        if all_bits.len() < 32 {
            return Err(SteganoError::IntegrityError(
                "Insufficient quality data for length header".to_string(),
            ));
        }

        // Decode 4-byte LE length from first 32 bits
        let mut len_bytes = [0u8; 4];
        for (byte_idx, chunk) in all_bits[..32].chunks(8).enumerate() {
            let mut byte = 0u8;
            for (i, &bit) in chunk.iter().enumerate() {
                byte |= bit << i;
            }
            len_bytes[byte_idx] = byte;
        }
        let payload_length = u32::from_le_bytes(len_bytes) as usize;

        let required_bits = 32 + payload_length * 8;
        if all_bits.len() < required_bits {
            return Err(SteganoError::IntegrityError(
                "Insufficient quality variants for payload extraction".to_string(),
            ));
        }

        // Convert payload bits (skip first 32 header bits) back to bytes
        let mut payload = Vec::with_capacity(payload_length);
        for chunk in all_bits[32..required_bits].chunks(8) {
            if chunk.len() == 8 {
                let mut byte = 0u8;
                for (i, &bit) in chunk.iter().enumerate() {
                    byte |= bit << i;
                }
                payload.push(byte);
            }
        }

        if payload.is_empty() && payload_length > 0 {
            return Err(SteganoError::IntegrityError(
                "No steganographic data found in quality scores".to_string(),
            ));
        }

        Ok(payload)
    }

    /// Embed payload using sample FORMAT field steganography
    fn embed_sample_format_strategy(
        &self,
        vcf: &mut VcfFile,
        payload: &[u8],
    ) -> Result<VcfEmbedResult> {
        let hex_payload = hex::encode(payload);
        let mut variants_modified = 0;
        let mut info_fields_added = 0;

        let chunk_size = 16; // 16 hex characters per variant

        let chunks: Vec<&str> = hex_payload
            .as_bytes()
            .chunks(chunk_size)
            .map(|chunk| std::str::from_utf8(chunk).unwrap_or(""))
            .collect();

        if chunks.len() > vcf.variants.len() {
            return Err(SteganoError::InsufficientCapacity {
                needed: chunks.len(),
                available: vcf.variants.len(),
            });
        }

        // Each chunk gets its own indexed FORMAT tag (S00, S01, …) so extraction can
        // reconstruct order even if variants are reordered between embed and extract.
        for (i, chunk) in chunks.iter().enumerate() {
            if i < vcf.variants.len() && !chunk.is_empty() {
                let variant = &mut vcf.variants[i];
                let field_tag = format!("S{i:02}"); // S00 … S99

                // Update FORMAT field
                if let Some(ref mut format) = variant.format {
                    if !format.contains(&field_tag) {
                        format.push_str(&format!(":{field_tag}"));
                    }
                } else {
                    variant.format = Some(format!("GT:{field_tag}"));
                }

                // Add steganographic data to each sample
                for sample in &mut variant.samples {
                    sample.insert(field_tag.clone(), chunk.to_string());
                }

                variants_modified += 1;
                info_fields_added += variant.samples.len();
            }
        }

        Ok(VcfEmbedResult {
            success: true,
            bytes_embedded: payload.len(),
            strategy_used: VcfEmbedStrategy::SampleFormatSteganography,
            variants_modified,
            info_fields_added,
            biological_integrity_preserved: true, // Custom FORMAT fields don't affect biology
        })
    }

    /// Extract payload from sample FORMAT field steganography
    fn extract_sample_format_strategy(&self, vcf: &VcfFile) -> Result<Vec<u8>> {
        // Collect (index, hex_chunk) pairs from indexed tags S00, S01, …
        let mut indexed_chunks: Vec<(usize, String)> = Vec::new();

        for variant in &vcf.variants {
            if let Some(ref format) = variant.format {
                for field_name in format.split(':') {
                    // Match S\d\d pattern (e.g. S00, S01, … S99)
                    if field_name.len() == 3
                        && field_name.starts_with('S')
                        && field_name[1..].chars().all(|c| c.is_ascii_digit())
                    {
                        if let Ok(idx) = field_name[1..].parse::<usize>() {
                            if let Some(sample) = variant.samples.first() {
                                if let Some(chunk) = sample.get(field_name) {
                                    if !chunk.is_empty() && chunk != "." {
                                        indexed_chunks.push((idx, chunk.clone()));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if indexed_chunks.is_empty() {
            return Err(SteganoError::IntegrityError(
                "No steganographic data found in FORMAT fields".to_string(),
            ));
        }

        // Sort by index to restore original chunk order
        indexed_chunks.sort_by_key(|(idx, _)| *idx);
        let hex_payload: String = indexed_chunks.into_iter().map(|(_, c)| c).collect();

        hex::decode(hex_payload).map_err(|_| {
            SteganoError::IntegrityError("Invalid hex data in FORMAT fields".to_string())
        })
    }

    /// Embed payload using variant ID steganography
    fn embed_variant_id_strategy(
        &self,
        vcf: &mut VcfFile,
        payload: &[u8],
    ) -> Result<VcfEmbedResult> {
        let hex_payload = hex::encode(payload);
        let chunk_size = 8; // 8 hex characters per variant ID suffix

        let chunks: Vec<&str> = hex_payload
            .as_bytes()
            .chunks(chunk_size)
            .map(|chunk| std::str::from_utf8(chunk).unwrap_or(""))
            .collect();

        if chunks.len() > vcf.variants.len() {
            return Err(SteganoError::InsufficientCapacity {
                needed: chunks.len(),
                available: vcf.variants.len(),
            });
        }

        let mut variants_modified = 0;

        // Embed chunks in variant IDs — always include a 4-digit index so extraction
        // can sort chunks back into the correct order regardless of variant reordering.
        for (i, chunk) in chunks.iter().enumerate() {
            if i < vcf.variants.len() && !chunk.is_empty() {
                let variant = &mut vcf.variants[i];

                // Format: STEG{i:04}_{hex_chunk} appended after the existing ID
                let steg_suffix = format!("STEG{i:04}_{chunk}");
                let new_id = if variant.id == "." {
                    steg_suffix
                } else {
                    format!("{}_{}",  variant.id, steg_suffix)
                };

                variant.id = new_id;
                variants_modified += 1;
            }
        }

        Ok(VcfEmbedResult {
            success: true,
            bytes_embedded: payload.len(),
            strategy_used: VcfEmbedStrategy::VariantIdSteganography,
            variants_modified,
            info_fields_added: 0,
            biological_integrity_preserved: true, // Variant IDs are primarily for reference
        })
    }

    /// Extract payload from variant ID steganography
    fn extract_variant_id_strategy(&self, vcf: &VcfFile) -> Result<Vec<u8>> {
        // Collect (index, hex_chunk) pairs by finding "STEG{4digits}_{hex}" pattern
        let marker = "STEG";
        let mut indexed_chunks: Vec<(usize, String)> = Vec::new();

        for variant in &vcf.variants {
            if let Some(steg_pos) = variant.id.find(marker) {
                let after = &variant.id[steg_pos + marker.len()..];
                // after should start with 4 decimal digits, then '_', then hex chunk
                if after.len() >= 6
                    && after[..4].chars().all(|c| c.is_ascii_digit())
                    && after.as_bytes().get(4) == Some(&b'_')
                {
                    if let Ok(idx) = after[..4].parse::<usize>() {
                        let rest = &after[5..];
                        let hex_end = rest
                            .find(|c: char| !c.is_ascii_hexdigit())
                            .unwrap_or(rest.len());
                        let hex_chunk = &rest[..hex_end];
                        if !hex_chunk.is_empty() {
                            indexed_chunks.push((idx, hex_chunk.to_string()));
                        }
                    }
                }
            }
        }

        if indexed_chunks.is_empty() {
            return Err(SteganoError::IntegrityError(
                "No steganographic data found in variant IDs".to_string(),
            ));
        }

        // Sort by index to restore original chunk order
        indexed_chunks.sort_by_key(|(idx, _)| *idx);
        let hex_payload: String = indexed_chunks.into_iter().map(|(_, c)| c).collect();

        hex::decode(hex_payload).map_err(|_| {
            SteganoError::IntegrityError("Invalid hex data in variant IDs".to_string())
        })
    }

    /// Estimate capacity for VCF file
    fn estimate_vcf_capacity(
        &self,
        data: &[u8],
        strategy: VcfEmbedStrategy,
    ) -> Result<CapacityInfo> {
        let vcf = self.parse_vcf(data)?;
        let variant_count = vcf.variants.len();

        let estimated_bytes = match strategy {
            VcfEmbedStrategy::InfoFieldSteganography => {
                // 32 hex chars (16 bytes) per variant
                variant_count * 16
            }
            VcfEmbedStrategy::QualityScoreSteganography => {
                // 3 bits per variant with quality score
                let quality_variants = vcf.variants.iter().filter(|v| v.qual.is_some()).count();
                quality_variants * 3 / 8 // Convert bits to bytes
            }
            VcfEmbedStrategy::SampleFormatSteganography => {
                // 16 hex chars (8 bytes) per variant
                variant_count * 8
            }
            VcfEmbedStrategy::VariantIdSteganography => {
                // 8 hex chars (4 bytes) per variant
                variant_count * 4
            }
            VcfEmbedStrategy::MultiStrategy => {
                // Combined capacity of all strategies
                let info_capacity = variant_count * 16;
                let quality_variants = vcf.variants.iter().filter(|v| v.qual.is_some()).count();
                let quality_capacity = quality_variants * 3 / 8;
                let format_capacity = variant_count * 8;
                let id_capacity = variant_count * 4;

                info_capacity + quality_capacity + format_capacity + id_capacity
            }
        };

        let total_bits = data.len() * 8;
        let recommended_bits = estimated_bytes * 8;

        let mut format_info = HashMap::new();
        format_info.insert(
            "method".to_string(),
            serde_json::Value::String(format!("{strategy:?}")),
        );
        format_info.insert(
            "variants".to_string(),
            serde_json::Value::Number(variant_count.into()),
        );
        format_info.insert(
            "estimated_capacity_bytes".to_string(),
            serde_json::Value::Number(estimated_bytes.into()),
        );
        format_info.insert(
            "samples".to_string(),
            serde_json::Value::Number(
                vcf.variants
                    .first()
                    .map(|v| v.samples.len())
                    .unwrap_or(0)
                    .into(),
            ),
        );

        Ok(CapacityInfo {
            total_bits,
            recommended_bits,
            steganographic_ratio: recommended_bits as f64 / total_bits as f64,
            format_specific_info: format_info,
        })
    }

    /// Serialize VCF file back to string format
    fn serialize_vcf(&self, vcf: &VcfFile) -> String {
        let mut result = String::new();

        // Add header lines
        for header in &vcf.header_lines {
            result.push_str(header);
            result.push('\n');
        }

        // Add column header
        result.push_str(&vcf.column_header);
        result.push('\n');

        // Add variant lines
        for variant in &vcf.variants {
            result.push_str(&self.serialize_variant(variant));
            result.push('\n');
        }

        result
    }

    /// Serialize individual variant to string
    fn serialize_variant(&self, variant: &VcfVariant) -> String {
        let mut fields = vec![
            variant.chrom.clone(),
            variant.pos.to_string(),
            variant.id.clone(),
            variant.ref_allele.clone(),
            variant.alt_alleles.join(","),
        ];

        // Quality field — use integer formatting since we embed in integer bits
        if let Some(qual) = variant.qual {
            fields.push(format!("{}", qual.round() as u32));
        } else {
            fields.push(".".to_string());
        }

        // Filter field
        fields.push(variant.filter.clone());

        // INFO field
        let info_str = if variant.info.is_empty() {
            ".".to_string()
        } else {
            variant
                .info
                .iter()
                .map(|(k, v)| {
                    if v.is_empty() {
                        k.clone()
                    } else {
                        format!("{k}={v}")
                    }
                })
                .collect::<Vec<_>>()
                .join(";")
        };
        fields.push(info_str);

        // FORMAT and sample fields
        if let Some(ref format) = variant.format {
            fields.push(format.clone());

            // Sample data
            for sample in &variant.samples {
                let format_fields: Vec<&str> = format.split(':').collect();
                let sample_values: Vec<String> = format_fields
                    .iter()
                    .map(|field| {
                        sample
                            .get(*field)
                            .cloned()
                            .unwrap_or_else(|| ".".to_string())
                    })
                    .collect();
                fields.push(sample_values.join(":"));
            }
        }

        fields.join("\t")
    }

    /// Verify VCF biological integrity
    fn verify_vcf_integrity(&self, data: &[u8]) -> Result<bool> {
        match self.parse_vcf(data) {
            Ok(vcf) => {
                // Check that all variants have valid chromosomes and positions
                for variant in &vcf.variants {
                    if variant.chrom.is_empty() || variant.pos == 0 {
                        return Ok(false);
                    }

                    // Check that reference and alternative alleles are valid
                    if variant.ref_allele.is_empty() || variant.alt_alleles.is_empty() {
                        return Ok(false);
                    }

                    // Validate nucleotide sequences
                    for allele in std::iter::once(&variant.ref_allele).chain(&variant.alt_alleles) {
                        if !allele
                            .chars()
                            .all(|c| "ATCGN.-".contains(c.to_ascii_uppercase()))
                        {
                            return Ok(false);
                        }
                    }
                }

                Ok(true)
            }
            Err(_) => Ok(false),
        }
    }

    /// Check if data is VCF format
    fn is_vcf(&self, data: &[u8]) -> bool {
        if data.is_empty() {
            return false;
        }

        let text = String::from_utf8_lossy(data);
        text.starts_with(VCF_MAGIC_HEADER)
    }
}

#[async_trait]
impl DataHandler for VcfHandler {
    fn detect_format(&self, data: &[u8]) -> Option<DataFormat> {
        if self.is_vcf(data) {
            Some(DataFormat::VCF)
        } else {
            None
        }
    }

    fn estimate_capacity(&self, carrier: &CarrierData) -> Result<CapacityInfo> {
        match carrier.format {
            DataFormat::VCF => {
                // Use INFO field steganography as default for capacity estimation
                self.estimate_vcf_capacity(
                    &carrier.raw_data,
                    VcfEmbedStrategy::InfoFieldSteganography,
                )
            }
            _ => Err(SteganoError::UnsupportedFormat(carrier.format.clone())),
        }
    }

    async fn embed(
        &self,
        carrier: &mut CarrierData,
        payload: &[u8],
        params: &EmbedParams,
    ) -> Result<EmbedResult> {
        let start_time = Instant::now();

        match carrier.format {
            DataFormat::VCF => {
                let mut vcf = self.parse_vcf(&carrier.raw_data)?;

                // Determine strategy from parameters or use default
                let strategy = params
                    .format_specific
                    .get("vcf_strategy")
                    .and_then(|v| v.as_str())
                    .map(|s| match s {
                        "info" => VcfEmbedStrategy::InfoFieldSteganography,
                        "quality" => VcfEmbedStrategy::QualityScoreSteganography,
                        "format" => VcfEmbedStrategy::SampleFormatSteganography,
                        "id" => VcfEmbedStrategy::VariantIdSteganography,
                        "multi" => VcfEmbedStrategy::MultiStrategy,
                        _ => VcfEmbedStrategy::InfoFieldSteganography,
                    })
                    .unwrap_or(VcfEmbedStrategy::InfoFieldSteganography);

                let embed_result = match strategy {
                    VcfEmbedStrategy::InfoFieldSteganography => {
                        self.embed_info_field_strategy(&mut vcf, payload)?
                    }
                    VcfEmbedStrategy::QualityScoreSteganography => {
                        self.embed_quality_strategy(&mut vcf, payload)?
                    }
                    VcfEmbedStrategy::SampleFormatSteganography => {
                        self.embed_sample_format_strategy(&mut vcf, payload)?
                    }
                    VcfEmbedStrategy::VariantIdSteganography => {
                        self.embed_variant_id_strategy(&mut vcf, payload)?
                    }
                    VcfEmbedStrategy::MultiStrategy => {
                        // Try multiple strategies in order of preference
                        self.embed_info_field_strategy(&mut vcf, payload)
                            .or_else(|_| self.embed_sample_format_strategy(&mut vcf, payload))
                            .or_else(|_| self.embed_variant_id_strategy(&mut vcf, payload))?
                    }
                };

                // Serialize modified VCF back to data
                let serialized_vcf = self.serialize_vcf(&vcf);
                carrier.raw_data = serialized_vcf.into_bytes();

                let embedding_time = start_time.elapsed().as_millis() as u64;

                Ok(EmbedResult {
                    success: embed_result.success,
                    bytes_embedded: embed_result.bytes_embedded,
                    carrier_modified: true,
                    performance_stats: PerformanceStats {
                        embedding_time_ms: embedding_time,
                        encryption_time_ms: 0,
                        gpu_acceleration_used: false,
                        memory_usage_bytes: carrier.raw_data.len() + payload.len(),
                    },
                    validation_report: None,
                })
            }
            _ => Err(SteganoError::UnsupportedFormat(carrier.format.clone())),
        }
    }

    async fn extract(&self, carrier: &CarrierData, params: &EmbedParams) -> Result<Vec<u8>> {
        match carrier.format {
            DataFormat::VCF => {
                let vcf = self.parse_vcf(&carrier.raw_data)?;

                // Determine strategy from parameters or try all
                let strategy = params
                    .format_specific
                    .get("vcf_strategy")
                    .and_then(|v| v.as_str())
                    .map(|s| match s {
                        "info" => VcfEmbedStrategy::InfoFieldSteganography,
                        "quality" => VcfEmbedStrategy::QualityScoreSteganography,
                        "format" => VcfEmbedStrategy::SampleFormatSteganography,
                        "id" => VcfEmbedStrategy::VariantIdSteganography,
                        "multi" => VcfEmbedStrategy::MultiStrategy,
                        _ => VcfEmbedStrategy::InfoFieldSteganography,
                    })
                    .unwrap_or(VcfEmbedStrategy::InfoFieldSteganography);

                match strategy {
                    VcfEmbedStrategy::InfoFieldSteganography => {
                        self.extract_info_field_strategy(&vcf)
                    }
                    VcfEmbedStrategy::QualityScoreSteganography => {
                        self.extract_quality_strategy(&vcf)
                    }
                    VcfEmbedStrategy::SampleFormatSteganography => {
                        self.extract_sample_format_strategy(&vcf)
                    }
                    VcfEmbedStrategy::VariantIdSteganography => {
                        self.extract_variant_id_strategy(&vcf)
                    }
                    VcfEmbedStrategy::MultiStrategy => {
                        // Try strategies in order until one succeeds
                        self.extract_info_field_strategy(&vcf)
                            .or_else(|_| self.extract_sample_format_strategy(&vcf))
                            .or_else(|_| self.extract_variant_id_strategy(&vcf))
                            .or_else(|_| self.extract_quality_strategy(&vcf))
                    }
                }
            }
            _ => Err(SteganoError::UnsupportedFormat(carrier.format.clone())),
        }
    }

    fn verify_integrity(&self, carrier: &CarrierData) -> Result<bool> {
        match carrier.format {
            DataFormat::VCF => self.verify_vcf_integrity(&carrier.raw_data),
            _ => Err(SteganoError::UnsupportedFormat(carrier.format.clone())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_vcf_detection() {
        let handler = VcfHandler::new();
        let vcf_data = b"##fileformat=VCFv4.2\n##source=test\n#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\nchr1\t100\t.\tA\tT\t30.0\tPASS\t.\n";
        assert_eq!(handler.detect_format(vcf_data), Some(DataFormat::VCF));
    }

    #[test]
    fn test_vcf_parsing() {
        let handler = VcfHandler::new();
        let vcf_data = b"##fileformat=VCFv4.2\n##source=test\n#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\nchr1\t100\trs123\tA\tT\t30.5\tPASS\tDP=50;AF=0.5\n";

        let vcf = handler.parse_vcf(vcf_data).unwrap();
        assert_eq!(vcf.variants.len(), 1);

        let variant = &vcf.variants[0];
        assert_eq!(variant.chrom, "chr1");
        assert_eq!(variant.pos, 100);
        assert_eq!(variant.id, "rs123");
        assert_eq!(variant.ref_allele, "A");
        assert_eq!(variant.alt_alleles, vec!["T"]);
        assert_eq!(variant.qual, Some(30.5));
        assert_eq!(variant.filter, "PASS");
        assert_eq!(variant.info.get("DP"), Some(&"50".to_string()));
        assert_eq!(variant.info.get("AF"), Some(&"0.5".to_string()));
    }

    #[test]
    fn test_vcf_capacity_estimation() {
        let handler = VcfHandler::new();
        let vcf_data = b"##fileformat=VCFv4.2\n#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\nchr1\t100\t.\tA\tT\t30.0\tPASS\t.\nchr1\t200\t.\tG\tC\t45.0\tPASS\t.\n";

        let carrier = CarrierData {
            format: DataFormat::VCF,
            raw_data: vcf_data.to_vec(),
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };

        let capacity = handler.estimate_capacity(&carrier).unwrap();
        assert!(capacity.recommended_bits > 0);
        assert!(capacity.steganographic_ratio > 0.0);
    }

    #[tokio::test]
    async fn test_vcf_info_field_embedding_roundtrip() {
        let handler = VcfHandler::new();
        let vcf_data = b"##fileformat=VCFv4.2\n#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\nchr1\t100\t.\tA\tT\t30.0\tPASS\tDP=50\nchr1\t200\t.\tG\tC\t45.0\tPASS\tAF=0.5\nchr1\t300\t.\tC\tA\t60.0\tPASS\t.\n";

        let mut carrier = CarrierData {
            format: DataFormat::VCF,
            raw_data: vcf_data.to_vec(),
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };

        let payload = b"secret genomics variant data";
        let mut params = EmbedParams {
            encryption_key: vec![],
            error_correction_level: ECLevel::None,
            stealth_level: StealthLevel::Low,
            use_gpu: false,
            format_specific: HashMap::new(),
            enable_validation: false,
        };
        params.format_specific.insert(
            "vcf_strategy".to_string(),
            serde_json::Value::String("info".to_string()),
        );

        // Embed
        let result = handler.embed(&mut carrier, payload, &params).await.unwrap();
        assert!(result.success);
        assert_eq!(result.bytes_embedded, payload.len());

        // Extract
        let extracted = handler.extract(&carrier, &params).await.unwrap();
        assert_eq!(extracted, payload);

        // Verify integrity
        assert!(handler.verify_integrity(&carrier).unwrap());
    }

    #[tokio::test]
    async fn test_vcf_quality_score_embedding_roundtrip() {
        let handler = VcfHandler::new();
        // Need ≥14 variants: 4-byte length header + 1-byte payload = 40 bits at 3 bits/variant
        let vcf_data = b"##fileformat=VCFv4.2\n#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\nchr1\t100\t.\tA\tT\t30\tPASS\t.\nchr1\t200\t.\tG\tC\t45\tPASS\t.\nchr1\t300\t.\tC\tA\t60\tPASS\t.\nchr1\t400\t.\tT\tG\t75\tPASS\t.\nchr1\t500\t.\tA\tC\t90\tPASS\t.\nchr1\t600\t.\tG\tT\t50\tPASS\t.\nchr1\t700\t.\tC\tG\t35\tPASS\t.\nchr1\t800\t.\tT\tA\t80\tPASS\t.\nchr1\t900\t.\tA\tG\t40\tPASS\t.\nchr1\t1000\t.\tG\tA\t55\tPASS\t.\nchr1\t1100\t.\tC\tT\t70\tPASS\t.\nchr1\t1200\t.\tT\tC\t85\tPASS\t.\nchr1\t1300\t.\tA\tT\t25\tPASS\t.\nchr1\t1400\t.\tG\tC\t95\tPASS\t.\n";

        let mut carrier = CarrierData {
            format: DataFormat::VCF,
            raw_data: vcf_data.to_vec(),
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };

        let payload = b"x"; // 1 byte payload for quality score strategy
        let mut params = EmbedParams {
            encryption_key: vec![],
            error_correction_level: ECLevel::None,
            stealth_level: StealthLevel::Low,
            use_gpu: false,
            format_specific: HashMap::new(),
            enable_validation: false,
        };
        params.format_specific.insert(
            "vcf_strategy".to_string(),
            serde_json::Value::String("quality".to_string()),
        );

        // Embed
        let result = handler.embed(&mut carrier, payload, &params).await.unwrap();
        assert!(result.success);

        // Extract
        let extracted = handler.extract(&carrier, &params).await.unwrap();
        assert_eq!(extracted, payload);

        // Verify integrity
        assert!(handler.verify_integrity(&carrier).unwrap());
    }

    #[test]
    fn test_vcf_serialization() {
        let handler = VcfHandler::new();
        let vcf_data = b"##fileformat=VCFv4.2\n##source=test\n#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\nchr1\t100\trs123\tA\tT\t30.5\tPASS\tDP=50;AF=0.5\n";

        let vcf = handler.parse_vcf(vcf_data).unwrap();
        let serialized = handler.serialize_vcf(&vcf);

        // Should be able to parse the serialized version
        let reparsed = handler.parse_vcf(serialized.as_bytes()).unwrap();
        assert_eq!(reparsed.variants.len(), vcf.variants.len());
        assert_eq!(reparsed.variants[0].chrom, vcf.variants[0].chrom);
        assert_eq!(reparsed.variants[0].pos, vcf.variants[0].pos);
    }

    #[test]
    fn test_vcf_biological_integrity() {
        let handler = VcfHandler::new();

        // Valid VCF
        let valid_vcf = b"##fileformat=VCFv4.2\n#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\nchr1\t100\t.\tA\tT\t30.0\tPASS\t.\n";
        assert!(handler.verify_vcf_integrity(valid_vcf).unwrap());

        // Invalid VCF with bad nucleotides
        let invalid_vcf = b"##fileformat=VCFv4.2\n#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\nchr1\t100\t.\tX\tY\t30.0\tPASS\t.\n";
        assert!(!handler.verify_vcf_integrity(invalid_vcf).unwrap());
    }

    #[tokio::test]
    async fn test_vcf_sample_format_embedding_roundtrip() {
        let handler = VcfHandler::new();
        // VCF with FORMAT and sample columns so SampleFormat strategy can embed.
        // payload "Hello!" = 6 bytes = 12 hex chars (fits in a single 16-char chunk, 1 variant needed).
        let vcf_data = b"##fileformat=VCFv4.2\n#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\tSAMPLE1\nchr1\t100\t.\tA\tT\t30\tPASS\t.\tGT\t0/1\nchr1\t200\t.\tG\tC\t45\tPASS\t.\tGT\t0/0\n";

        let mut carrier = CarrierData {
            format: DataFormat::VCF,
            raw_data: vcf_data.to_vec(),
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };

        let payload = b"Hello!";
        let mut params = EmbedParams {
            encryption_key: vec![],
            error_correction_level: ECLevel::None,
            stealth_level: StealthLevel::Low,
            use_gpu: false,
            enable_validation: false,
            format_specific: HashMap::new(),
        };
        params.format_specific.insert(
            "vcf_strategy".to_string(),
            serde_json::Value::String("format".to_string()),
        );

        handler.embed(&mut carrier, payload, &params).await.unwrap();
        let extracted = handler.extract(&carrier, &params).await.unwrap();
        assert_eq!(&extracted, payload);
    }
}
