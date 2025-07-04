//! SAM/BAM format handlers for bioinformatics alignment data steganography
//! 
//! Implements steganography techniques for SAM (Sequence Alignment/Map) and BAM (Binary Alignment/Map)
//! formats used in bioinformatics for storing DNA/RNA sequence alignments to reference genomes.

use crate::core::*;
use super::DataHandler;
use async_trait::async_trait;
use std::time::Instant;
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

pub struct SamBamHandler;

/// SAM file structure
#[derive(Debug, Clone)]
pub struct SamFile {
    /// Header lines starting with @
    pub header_lines: Vec<SamHeaderLine>,
    
    /// Alignment records
    pub alignments: Vec<SamAlignment>,
}

/// BAM file structure  
#[derive(Debug, Clone)]
pub struct BamFile {
    /// BAM header
    pub header: BamHeader,
    
    /// Reference sequences
    pub references: Vec<BamReference>,
    
    /// Alignment records
    pub alignments: Vec<BamAlignment>,
}

/// SAM header line
#[derive(Debug, Clone)]
pub struct SamHeaderLine {
    /// Header type (HD, SQ, RG, PG, CO)
    pub header_type: String,
    
    /// Header tags and values
    pub tags: HashMap<String, String>,
    
    /// Original line text
    pub raw_line: String,
}

/// SAM alignment record
#[derive(Debug, Clone)]
pub struct SamAlignment {
    /// Query template name
    pub qname: String,
    
    /// Bitwise flag
    pub flag: u16,
    
    /// Reference sequence name
    pub rname: String,
    
    /// 1-based leftmost mapping position
    pub pos: u32,
    
    /// Mapping quality
    pub mapq: u8,
    
    /// CIGAR string
    pub cigar: String,
    
    /// Reference name of mate/next read
    pub rnext: String,
    
    /// Position of mate/next read
    pub pnext: u32,
    
    /// Template length
    pub tlen: i32,
    
    /// Segment sequence
    pub seq: String,
    
    /// ASCII quality scores
    pub qual: String,
    
    /// Optional fields
    pub optional_fields: Vec<SamOptionalField>,
}

/// BAM header
#[derive(Debug, Clone)]
pub struct BamHeader {
    /// Magic bytes "BAM\1"
    pub magic: [u8; 4],
    
    /// Length of header text
    pub l_text: u32,
    
    /// Header text (SAM format)
    pub text: String,
    
    /// Number of reference sequences
    pub n_ref: u32,
}

/// BAM reference sequence
#[derive(Debug, Clone)]
pub struct BamReference {
    /// Length of reference name
    pub l_name: u32,
    
    /// Reference sequence name
    pub name: String,
    
    /// Length of reference sequence
    pub l_ref: u32,
}

/// BAM alignment record
#[derive(Debug, Clone)]
pub struct BamAlignment {
    /// Block size
    pub block_size: u32,
    
    /// Reference ID
    pub refid: i32,
    
    /// 0-based leftmost position
    pub pos: i32,
    
    /// Length of read name
    pub l_read_name: u8,
    
    /// Mapping quality
    pub mapq: u8,
    
    /// BAM file index
    pub bin: u16,
    
    /// Number of CIGAR operations
    pub n_cigar_op: u16,
    
    /// Bitwise flag
    pub flag: u16,
    
    /// Length of sequence
    pub l_seq: u32,
    
    /// Reference ID of next read
    pub next_refid: i32,
    
    /// Position of next read
    pub next_pos: i32,
    
    /// Template length
    pub tlen: i32,
    
    /// Read name (null-terminated)
    pub read_name: String,
    
    /// CIGAR operations
    pub cigar: Vec<u32>,
    
    /// 4-bit encoded sequence
    pub seq: Vec<u8>,
    
    /// Quality scores
    pub qual: Vec<u8>,
    
    /// Auxiliary data
    pub aux_data: Vec<u8>,
}

/// SAM optional field
#[derive(Debug, Clone)]
pub struct SamOptionalField {
    /// Field tag (2 characters)
    pub tag: String,
    
    /// Field type (A, i, f, Z, H, B)
    pub field_type: char,
    
    /// Field value
    pub value: String,
}

/// SAM/BAM embedding strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SamBamEmbedStrategy {
    /// Embed in optional fields
    OptionalFields,
    
    /// Embed in quality scores
    QualityScores,
    
    /// Embed in comment lines
    CommentLines,
    
    /// Embed in auxiliary data (BAM only)
    AuxiliaryData,
    
    /// Multi-strategy embedding
    MultiStrategy,
}

/// SAM/BAM embedding result
#[derive(Debug, Clone)]
pub struct SamBamEmbedResult {
    pub success: bool,
    pub bytes_embedded: usize,
    pub strategy_used: SamBamEmbedStrategy,
    pub records_modified: usize,
    pub fields_added: usize,
    pub biological_integrity_preserved: bool,
}

/// Constants for SAM/BAM steganography
const BAM_MAGIC: [u8; 4] = [b'B', b'A', b'M', 1];
const SAM_HEADER_PREFIX: u8 = b'@';
const STEGANO_TAG_PREFIX: &str = "ZS"; // Custom tag prefix for steganographic data
const MAX_OPTIONAL_FIELD_SIZE: usize = 255;

impl SamBamHandler {
    pub fn new() -> Self {
        Self
    }

    /// Parse SAM file
    fn parse_sam(&self, data: &[u8]) -> Result<SamFile> {
        let content = String::from_utf8_lossy(data);
        let lines: Vec<&str> = content.lines().collect();
        
        if lines.is_empty() {
            return Err(SteganoError::IntegrityError("Empty SAM file".to_string()));
        }
        
        let mut header_lines = Vec::new();
        let mut alignments = Vec::new();
        let mut in_header = true;
        
        for line in lines {
            if line.is_empty() {
                continue;
            }
            
            if line.starts_with('@') && in_header {
                header_lines.push(self.parse_sam_header_line(line)?);
            } else {
                in_header = false;
                if !line.starts_with('@') {
                    alignments.push(self.parse_sam_alignment(line)?);
                }
            }
        }
        
        Ok(SamFile {
            header_lines,
            alignments,
        })
    }
    
    /// Parse BAM file
    fn parse_bam(&self, data: &[u8]) -> Result<BamFile> {
        if data.len() < 4 {
            return Err(SteganoError::IntegrityError("File too small to be BAM".to_string()));
        }
        
        let mut offset = 0;
        
        // Check magic bytes
        let magic = [data[0], data[1], data[2], data[3]];
        if magic != BAM_MAGIC {
            return Err(SteganoError::IntegrityError("Invalid BAM magic bytes".to_string()));
        }
        offset += 4;
        
        // Read header
        if offset + 4 > data.len() {
            return Err(SteganoError::IntegrityError("Insufficient data for header length".to_string()));
        }
        
        let l_text = u32::from_le_bytes([
            data[offset], data[offset + 1], data[offset + 2], data[offset + 3]
        ]);
        offset += 4;
        
        if offset + l_text as usize > data.len() {
            return Err(SteganoError::IntegrityError("Insufficient data for header text".to_string()));
        }
        
        let text = String::from_utf8_lossy(&data[offset..offset + l_text as usize]).to_string();
        offset += l_text as usize;
        
        // Read number of references
        if offset + 4 > data.len() {
            return Err(SteganoError::IntegrityError("Insufficient data for reference count".to_string()));
        }
        
        let n_ref = u32::from_le_bytes([
            data[offset], data[offset + 1], data[offset + 2], data[offset + 3]
        ]);
        offset += 4;
        
        // Read reference sequences
        let mut references = Vec::new();
        for _ in 0..n_ref {
            let reference = self.parse_bam_reference(data, &mut offset)?;
            references.push(reference);
        }
        
        // Read alignment records (simplified - just read a few for demonstration)
        let mut alignments = Vec::new();
        let max_alignments = 100; // Limit for performance
        
        while offset < data.len() && alignments.len() < max_alignments {
            match self.parse_bam_alignment(data, &mut offset) {
                Ok(alignment) => alignments.push(alignment),
                Err(_) => break, // Stop on parsing error
            }
        }
        
        let header = BamHeader {
            magic,
            l_text,
            text,
            n_ref,
        };
        
        Ok(BamFile {
            header,
            references,
            alignments,
        })
    }
    
    /// Parse SAM header line
    fn parse_sam_header_line(&self, line: &str) -> Result<SamHeaderLine> {
        let parts: Vec<&str> = line[1..].split('\t').collect(); // Skip @ prefix
        
        if parts.is_empty() {
            return Err(SteganoError::IntegrityError("Invalid SAM header line".to_string()));
        }
        
        let header_type = parts[0].to_string();
        let mut tags = HashMap::new();
        
        for part in &parts[1..] {
            if let Some(colon_pos) = part.find(':') {
                let tag = part[..colon_pos].to_string();
                let value = part[colon_pos + 1..].to_string();
                tags.insert(tag, value);
            }
        }
        
        Ok(SamHeaderLine {
            header_type,
            tags,
            raw_line: line.to_string(),
        })
    }
    
    /// Parse SAM alignment record
    fn parse_sam_alignment(&self, line: &str) -> Result<SamAlignment> {
        let fields: Vec<&str> = line.split('\t').collect();
        
        if fields.len() < 11 {
            return Err(SteganoError::IntegrityError("Invalid SAM alignment record".to_string()));
        }
        
        let qname = fields[0].to_string();
        let flag = fields[1].parse::<u16>()
            .map_err(|_| SteganoError::IntegrityError("Invalid flag field".to_string()))?;
        let rname = fields[2].to_string();
        let pos = fields[3].parse::<u32>()
            .map_err(|_| SteganoError::IntegrityError("Invalid position field".to_string()))?;
        let mapq = fields[4].parse::<u8>()
            .map_err(|_| SteganoError::IntegrityError("Invalid mapping quality".to_string()))?;
        let cigar = fields[5].to_string();
        let rnext = fields[6].to_string();
        let pnext = fields[7].parse::<u32>()
            .map_err(|_| SteganoError::IntegrityError("Invalid next position".to_string()))?;
        let tlen = fields[8].parse::<i32>()
            .map_err(|_| SteganoError::IntegrityError("Invalid template length".to_string()))?;
        let seq = fields[9].to_string();
        let qual = fields[10].to_string();
        
        // Parse optional fields
        let mut optional_fields = Vec::new();
        for field_str in &fields[11..] {
            if let Ok(field) = self.parse_optional_field(field_str) {
                optional_fields.push(field);
            }
        }
        
        Ok(SamAlignment {
            qname,
            flag,
            rname,
            pos,
            mapq,
            cigar,
            rnext,
            pnext,
            tlen,
            seq,
            qual,
            optional_fields,
        })
    }
    
    /// Parse SAM optional field
    fn parse_optional_field(&self, field_str: &str) -> Result<SamOptionalField> {
        let parts: Vec<&str> = field_str.split(':').collect();
        
        if parts.len() != 3 {
            return Err(SteganoError::IntegrityError("Invalid optional field format".to_string()));
        }
        
        let tag = parts[0].to_string();
        let field_type = parts[1].chars().next()
            .ok_or_else(|| SteganoError::IntegrityError("Missing field type".to_string()))?;
        let value = parts[2].to_string();
        
        Ok(SamOptionalField {
            tag,
            field_type,
            value,
        })
    }
    
    /// Parse BAM reference sequence
    fn parse_bam_reference(&self, data: &[u8], offset: &mut usize) -> Result<BamReference> {
        if *offset + 4 > data.len() {
            return Err(SteganoError::IntegrityError("Insufficient data for reference name length".to_string()));
        }
        
        let l_name = u32::from_le_bytes([
            data[*offset], data[*offset + 1], data[*offset + 2], data[*offset + 3]
        ]);
        *offset += 4;
        
        if *offset + l_name as usize > data.len() {
            return Err(SteganoError::IntegrityError("Insufficient data for reference name".to_string()));
        }
        
        let name_bytes = &data[*offset..*offset + l_name as usize];
        let name = String::from_utf8_lossy(name_bytes).trim_end_matches('\0').to_string();
        *offset += l_name as usize;
        
        if *offset + 4 > data.len() {
            return Err(SteganoError::IntegrityError("Insufficient data for reference length".to_string()));
        }
        
        let l_ref = u32::from_le_bytes([
            data[*offset], data[*offset + 1], data[*offset + 2], data[*offset + 3]
        ]);
        *offset += 4;
        
        Ok(BamReference {
            l_name,
            name,
            l_ref,
        })
    }
    
    /// Parse BAM alignment record
    fn parse_bam_alignment(&self, data: &[u8], offset: &mut usize) -> Result<BamAlignment> {
        if *offset + 4 > data.len() {
            return Err(SteganoError::IntegrityError("Insufficient data for block size".to_string()));
        }
        
        let block_size = u32::from_le_bytes([
            data[*offset], data[*offset + 1], data[*offset + 2], data[*offset + 3]
        ]);
        *offset += 4;
        
        let record_start = *offset;
        
        if *offset + block_size as usize > data.len() {
            return Err(SteganoError::IntegrityError("Insufficient data for alignment record".to_string()));
        }
        
        // Parse fixed-length fields
        let refid = i32::from_le_bytes([
            data[*offset], data[*offset + 1], data[*offset + 2], data[*offset + 3]
        ]);
        *offset += 4;
        
        let pos = i32::from_le_bytes([
            data[*offset], data[*offset + 1], data[*offset + 2], data[*offset + 3]
        ]);
        *offset += 4;
        
        let l_read_name = data[*offset];
        *offset += 1;
        
        let mapq = data[*offset];
        *offset += 1;
        
        let bin = u16::from_le_bytes([data[*offset], data[*offset + 1]]);
        *offset += 2;
        
        let n_cigar_op = u16::from_le_bytes([data[*offset], data[*offset + 1]]);
        *offset += 2;
        
        let flag = u16::from_le_bytes([data[*offset], data[*offset + 1]]);
        *offset += 2;
        
        let l_seq = u32::from_le_bytes([
            data[*offset], data[*offset + 1], data[*offset + 2], data[*offset + 3]
        ]);
        *offset += 4;
        
        let next_refid = i32::from_le_bytes([
            data[*offset], data[*offset + 1], data[*offset + 2], data[*offset + 3]
        ]);
        *offset += 4;
        
        let next_pos = i32::from_le_bytes([
            data[*offset], data[*offset + 1], data[*offset + 2], data[*offset + 3]
        ]);
        *offset += 4;
        
        let tlen = i32::from_le_bytes([
            data[*offset], data[*offset + 1], data[*offset + 2], data[*offset + 3]
        ]);
        *offset += 4;
        
        // Read variable-length fields
        let read_name_bytes = &data[*offset..*offset + l_read_name as usize];
        let read_name = String::from_utf8_lossy(read_name_bytes).trim_end_matches('\0').to_string();
        *offset += l_read_name as usize;
        
        // Read CIGAR
        let mut cigar = Vec::new();
        for _ in 0..n_cigar_op {
            if *offset + 4 > data.len() {
                break;
            }
            let cigar_op = u32::from_le_bytes([
                data[*offset], data[*offset + 1], data[*offset + 2], data[*offset + 3]
            ]);
            cigar.push(cigar_op);
            *offset += 4;
        }
        
        // Read sequence (4-bit encoded)
        let seq_bytes = ((l_seq + 1) / 2) as usize;
        let seq = if *offset + seq_bytes <= data.len() {
            data[*offset..*offset + seq_bytes].to_vec()
        } else {
            Vec::new()
        };
        *offset += seq_bytes;
        
        // Read quality scores
        let qual = if *offset + l_seq as usize <= data.len() {
            data[*offset..*offset + l_seq as usize].to_vec()
        } else {
            Vec::new()
        };
        *offset += l_seq as usize;
        
        // Read auxiliary data
        let aux_data_end = record_start + block_size as usize;
        let aux_data = if *offset < aux_data_end && aux_data_end <= data.len() {
            data[*offset..aux_data_end].to_vec()
        } else {
            Vec::new()
        };
        *offset = aux_data_end;
        
        Ok(BamAlignment {
            block_size,
            refid,
            pos,
            l_read_name,
            mapq,
            bin,
            n_cigar_op,
            flag,
            l_seq,
            next_refid,
            next_pos,
            tlen,
            read_name,
            cigar,
            seq,
            qual,
            aux_data,
        })
    }
    
    /// Embed payload in SAM optional fields
    fn embed_sam_optional_fields(&self, sam: &mut SamFile, payload: &[u8]) -> Result<SamBamEmbedResult> {
        // Create payload with length header
        let mut full_payload = Vec::new();
        let payload_len = payload.len() as u32;
        full_payload.extend_from_slice(&payload_len.to_le_bytes());
        full_payload.extend_from_slice(payload);
        
        // Convert to hex for storage in string fields
        let hex_payload = hex::encode(full_payload);
        
        // Split payload across alignment records
        let chunk_size = 64; // Hex characters per record
        let chunks: Vec<&str> = hex_payload.as_bytes()
            .chunks(chunk_size)
            .map(|chunk| std::str::from_utf8(chunk).unwrap_or(""))
            .collect();
        
        if chunks.len() > sam.alignments.len() {
            return Err(SteganoError::InsufficientCapacity {
                needed: chunks.len(),
                available: sam.alignments.len(),
            });
        }
        
        let mut records_modified = 0;
        let mut fields_added = 0;
        
        // Add steganographic optional fields
        for (i, chunk) in chunks.iter().enumerate() {
            if i < sam.alignments.len() && !chunk.is_empty() {
                let field_tag = format!("{}{:02}", STEGANO_TAG_PREFIX, i);
                let optional_field = SamOptionalField {
                    tag: field_tag,
                    field_type: 'Z', // String type
                    value: chunk.to_string(),
                };
                
                sam.alignments[i].optional_fields.push(optional_field);
                records_modified += 1;
                fields_added += 1;
            }
        }
        
        Ok(SamBamEmbedResult {
            success: true,
            bytes_embedded: payload.len(),
            strategy_used: SamBamEmbedStrategy::OptionalFields,
            records_modified,
            fields_added,
            biological_integrity_preserved: true, // Optional fields don't affect alignment
        })
    }
    
    /// Extract payload from SAM optional fields
    fn extract_sam_optional_fields(&self, sam: &SamFile) -> Result<Vec<u8>> {
        let mut hex_chunks = Vec::new();
        
        // Collect steganographic optional fields
        for alignment in &sam.alignments {
            for field in &alignment.optional_fields {
                if field.tag.starts_with(STEGANO_TAG_PREFIX) {
                    let index_str = &field.tag[STEGANO_TAG_PREFIX.len()..];
                    if let Ok(index) = index_str.parse::<usize>() {
                        hex_chunks.push((index, field.value.clone()));
                    }
                }
            }
        }
        
        if hex_chunks.is_empty() {
            return Err(SteganoError::IntegrityError("No steganographic data found in optional fields".to_string()));
        }
        
        // Sort by index and reconstruct payload
        hex_chunks.sort_by_key(|(index, _)| *index);
        let hex_payload: String = hex_chunks.into_iter().map(|(_, chunk)| chunk).collect();
        
        // Decode hex to bytes
        let full_payload = hex::decode(hex_payload)
            .map_err(|_| SteganoError::IntegrityError("Invalid hex data in optional fields".to_string()))?;
        
        // Extract payload length and validate
        if full_payload.len() < 4 {
            return Err(SteganoError::IntegrityError("Insufficient data to extract payload length".to_string()));
        }
        
        let payload_len = u32::from_le_bytes([
            full_payload[0], full_payload[1], full_payload[2], full_payload[3]
        ]) as usize;
        
        if payload_len + 4 > full_payload.len() {
            return Err(SteganoError::IntegrityError("Invalid payload length in extracted data".to_string()));
        }
        
        let payload = full_payload[4..4 + payload_len].to_vec();
        Ok(payload)
    }
    
    /// Embed payload in quality scores
    fn embed_quality_scores(&self, sam: &mut SamFile, payload: &[u8]) -> Result<SamBamEmbedResult> {
        // Convert payload to bits
        let mut payload_bits = Vec::new();
        for byte in payload {
            for i in 0..8 {
                payload_bits.push((byte >> i) & 1);
            }
        }
        
        // Count available quality score positions
        let mut total_quals = 0;
        for alignment in &sam.alignments {
            total_quals += alignment.qual.len();
        }
        
        if payload_bits.len() > total_quals {
            return Err(SteganoError::InsufficientCapacity {
                needed: payload_bits.len(),
                available: total_quals,
            });
        }
        
        let mut bit_index = 0;
        let mut records_modified = 0;
        
        // Embed bits in quality scores (LSB)
        for alignment in &mut sam.alignments {
            if bit_index >= payload_bits.len() {
                break;
            }
            
            let mut modified_qual = String::new();
            let mut alignment_modified = false;
            
            for c in alignment.qual.chars() {
                if bit_index < payload_bits.len() {
                    let ascii_val = c as u8;
                    
                    // Ensure quality score is in valid range (33-126)
                    if ascii_val >= 33 && ascii_val <= 126 {
                        // Modify LSB
                        let new_val = (ascii_val & 0xFE) | payload_bits[bit_index];
                        modified_qual.push(new_val as char);
                        bit_index += 1;
                        alignment_modified = true;
                    } else {
                        modified_qual.push(c);
                    }
                } else {
                    modified_qual.push(c);
                }
            }
            
            alignment.qual = modified_qual;
            if alignment_modified {
                records_modified += 1;
            }
        }
        
        Ok(SamBamEmbedResult {
            success: bit_index >= payload_bits.len(),
            bytes_embedded: payload.len(),
            strategy_used: SamBamEmbedStrategy::QualityScores,
            records_modified,
            fields_added: 0,
            biological_integrity_preserved: true, // LSB changes are minimal
        })
    }
    
    /// Extract payload from quality scores
    fn extract_quality_scores(&self, sam: &SamFile, payload_length: usize) -> Result<Vec<u8>> {
        let required_bits = payload_length * 8;
        let mut extracted_bits = Vec::new();
        
        // Extract bits from quality scores
        for alignment in &sam.alignments {
            if extracted_bits.len() >= required_bits {
                break;
            }
            
            for c in alignment.qual.chars() {
                if extracted_bits.len() >= required_bits {
                    break;
                }
                
                let ascii_val = c as u8;
                if ascii_val >= 33 && ascii_val <= 126 {
                    extracted_bits.push(ascii_val & 1);
                }
            }
        }
        
        if extracted_bits.len() < required_bits {
            return Err(SteganoError::IntegrityError("Insufficient quality data for extraction".to_string()));
        }
        
        // Convert bits back to bytes
        let mut payload = Vec::new();
        for chunk in extracted_bits.chunks(8) {
            if chunk.len() == 8 {
                let mut byte = 0u8;
                for (i, &bit) in chunk.iter().enumerate() {
                    byte |= bit << i;
                }
                payload.push(byte);
            }
        }
        
        Ok(payload)
    }
    
    /// Embed payload in comment lines
    fn embed_comment_lines(&self, sam: &mut SamFile, payload: &[u8]) -> Result<SamBamEmbedResult> {
        let hex_payload = hex::encode(payload);
        
        // Create comment header line with embedded data
        let comment_line = SamHeaderLine {
            header_type: "CO".to_string(),
            tags: HashMap::new(),
            raw_line: format!("@CO\tSTEGANO_DATA:{}", hex_payload),
        };
        
        sam.header_lines.push(comment_line);
        
        Ok(SamBamEmbedResult {
            success: true,
            bytes_embedded: payload.len(),
            strategy_used: SamBamEmbedStrategy::CommentLines,
            records_modified: 0,
            fields_added: 1,
            biological_integrity_preserved: true, // Comments don't affect alignment
        })
    }
    
    /// Extract payload from comment lines
    fn extract_comment_lines(&self, sam: &SamFile) -> Result<Vec<u8>> {
        for header_line in &sam.header_lines {
            if header_line.header_type == "CO" {
                if let Some(data_start) = header_line.raw_line.find("STEGANO_DATA:") {
                    let hex_start = data_start + "STEGANO_DATA:".len();
                    let hex_data = &header_line.raw_line[hex_start..];
                    
                    return hex::decode(hex_data.trim())
                        .map_err(|_| SteganoError::IntegrityError("Invalid hex data in comment".to_string()));
                }
            }
        }
        
        Err(SteganoError::IntegrityError("No steganographic data found in comments".to_string()))
    }
    
    /// Estimate capacity for SAM/BAM file
    fn estimate_capacity(&self, data: &[u8], strategy: SamBamEmbedStrategy, is_bam: bool) -> Result<CapacityInfo> {
        let (alignment_count, quality_chars) = if is_bam {
            let bam = self.parse_bam(data)?;
            let quality_chars: usize = bam.alignments.iter().map(|a| a.qual.len()).sum();
            (bam.alignments.len(), quality_chars)
        } else {
            let sam = self.parse_sam(data)?;
            let quality_chars: usize = sam.alignments.iter().map(|a| a.qual.len()).sum();
            (sam.alignments.len(), quality_chars)
        };
        
        let estimated_bytes = match strategy {
            SamBamEmbedStrategy::OptionalFields => {
                // 64 hex chars (32 bytes) per alignment record
                alignment_count * 32
            },
            SamBamEmbedStrategy::QualityScores => {
                // 1 bit per quality score character
                quality_chars / 8
            },
            SamBamEmbedStrategy::CommentLines => {
                // Conservative estimate for comment capacity
                data.len() / 100
            },
            SamBamEmbedStrategy::AuxiliaryData => {
                if is_bam {
                    // BAM auxiliary data capacity
                    alignment_count * 50 // Conservative estimate
                } else {
                    0 // Not applicable to SAM
                }
            },
            SamBamEmbedStrategy::MultiStrategy => {
                // Combined capacity
                let optional_capacity = alignment_count * 32;
                let quality_capacity = quality_chars / 8;
                let comment_capacity = data.len() / 100;
                
                optional_capacity + quality_capacity + comment_capacity
            },
        };
        
        let total_bits = data.len() * 8;
        let recommended_bits = estimated_bytes * 8;
        
        let mut format_info = HashMap::new();
        format_info.insert("method".to_string(), serde_json::Value::String(format!("{:?}", strategy)));
        format_info.insert("alignment_records".to_string(), serde_json::Value::Number(alignment_count.into()));
        format_info.insert("quality_characters".to_string(), serde_json::Value::Number(quality_chars.into()));
        format_info.insert("estimated_capacity_bytes".to_string(), serde_json::Value::Number(estimated_bytes.into()));
        format_info.insert("is_bam".to_string(), serde_json::Value::Bool(is_bam));
        
        Ok(CapacityInfo {
            total_bits,
            recommended_bits,
            steganographic_ratio: recommended_bits as f64 / total_bits as f64,
            format_specific_info: format_info,
        })
    }
    
    /// Serialize SAM file back to string format
    fn serialize_sam(&self, sam: &SamFile) -> String {
        let mut result = String::new();
        
        // Add header lines
        for header_line in &sam.header_lines {
            result.push_str(&header_line.raw_line);
            result.push('\n');
        }
        
        // Add alignment records
        for alignment in &sam.alignments {
            result.push_str(&self.serialize_sam_alignment(alignment));
            result.push('\n');
        }
        
        result
    }
    
    /// Serialize SAM alignment record
    fn serialize_sam_alignment(&self, alignment: &SamAlignment) -> String {
        let mut fields = vec![
            alignment.qname.clone(),
            alignment.flag.to_string(),
            alignment.rname.clone(),
            alignment.pos.to_string(),
            alignment.mapq.to_string(),
            alignment.cigar.clone(),
            alignment.rnext.clone(),
            alignment.pnext.to_string(),
            alignment.tlen.to_string(),
            alignment.seq.clone(),
            alignment.qual.clone(),
        ];
        
        // Add optional fields
        for field in &alignment.optional_fields {
            fields.push(format!("{}:{}:{}", field.tag, field.field_type, field.value));
        }
        
        fields.join("\t")
    }
    
    /// Verify SAM/BAM biological integrity
    fn verify_integrity(&self, data: &[u8], is_bam: bool) -> Result<bool> {
        if is_bam {
            match self.parse_bam(data) {
                Ok(_) => Ok(true),
                Err(_) => Ok(false),
            }
        } else {
            match self.parse_sam(data) {
                Ok(sam) => {
                    // Basic validation
                    for alignment in &sam.alignments {
                        // Check sequence and quality lengths match
                        if alignment.seq != "*" && alignment.qual != "*" {
                            if alignment.seq.len() != alignment.qual.len() {
                                return Ok(false);
                            }
                        }
                        
                        // Validate sequence characters
                        for c in alignment.seq.chars() {
                            if !"ATCGN*".contains(c.to_ascii_uppercase()) {
                                return Ok(false);
                            }
                        }
                    }
                    Ok(true)
                },
                Err(_) => Ok(false),
            }
        }
    }
    
    /// Check if data is BAM format
    fn is_bam(&self, data: &[u8]) -> bool {
        data.len() >= 4 && data[..4] == BAM_MAGIC
    }
    
    /// Check if data is SAM format
    fn is_sam(&self, data: &[u8]) -> bool {
        if data.is_empty() {
            return false;
        }
        
        let text = String::from_utf8_lossy(data);
        let lines: Vec<&str> = text.lines().take(10).collect(); // Check first 10 lines
        
        // Should have header lines starting with @
        let has_headers = lines.iter().any(|line| line.starts_with('@'));
        
        // Should have alignment records with tab-separated fields
        let has_alignments = lines.iter().any(|line| {
            !line.starts_with('@') && line.split('\t').count() >= 11
        });
        
        has_headers || has_alignments
    }
}

#[async_trait]
impl DataHandler for SamBamHandler {
    fn detect_format(&self, data: &[u8]) -> Option<DataFormat> {
        if self.is_bam(data) {
            Some(DataFormat::BAM)
        } else if self.is_sam(data) {
            Some(DataFormat::SAM)
        } else {
            None
        }
    }
    
    fn estimate_capacity(&self, carrier: &CarrierData) -> Result<CapacityInfo> {
        match carrier.format {
            DataFormat::SAM => {
                self.estimate_capacity(&carrier.raw_data, SamBamEmbedStrategy::OptionalFields, false)
            },
            DataFormat::BAM => {
                self.estimate_capacity(&carrier.raw_data, SamBamEmbedStrategy::OptionalFields, true)
            },
            _ => Err(SteganoError::UnsupportedFormat(carrier.format.clone())),
        }
    }
    
    async fn embed(&self, carrier: &mut CarrierData, payload: &[u8], params: &EmbedParams) -> Result<EmbedResult> {
        let start_time = Instant::now();
        
        match carrier.format {
            DataFormat::SAM => {
                let mut sam = self.parse_sam(&carrier.raw_data)?;
                
                // Determine strategy from parameters
                let strategy = params.format_specific
                    .get("sam_strategy")
                    .and_then(|v| v.as_str())
                    .map(|s| match s {
                        "optional" => SamBamEmbedStrategy::OptionalFields,
                        "quality" => SamBamEmbedStrategy::QualityScores,
                        "comment" => SamBamEmbedStrategy::CommentLines,
                        "multi" => SamBamEmbedStrategy::MultiStrategy,
                        _ => SamBamEmbedStrategy::OptionalFields,
                    })
                    .unwrap_or(SamBamEmbedStrategy::OptionalFields);
                
                let embed_result = match strategy {
                    SamBamEmbedStrategy::OptionalFields => {
                        self.embed_sam_optional_fields(&mut sam, payload)?
                    },
                    SamBamEmbedStrategy::QualityScores => {
                        self.embed_quality_scores(&mut sam, payload)?
                    },
                    SamBamEmbedStrategy::CommentLines => {
                        self.embed_comment_lines(&mut sam, payload)?
                    },
                    SamBamEmbedStrategy::MultiStrategy => {
                        // Try strategies in order of preference
                        self.embed_sam_optional_fields(&mut sam, payload)
                            .or_else(|_| self.embed_comment_lines(&mut sam, payload))?
                    },
                    _ => {
                        self.embed_sam_optional_fields(&mut sam, payload)?
                    }
                };
                
                // Serialize modified SAM back to data
                carrier.raw_data = self.serialize_sam(&sam).into_bytes();
                
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
                })
            },
            DataFormat::BAM => {
                // For BAM, we'll implement a simplified approach
                // In practice, would need full BAM serialization
                return Err(SteganoError::UnsupportedFormat(DataFormat::BAM));
            },
            _ => Err(SteganoError::UnsupportedFormat(carrier.format.clone())),
        }
    }
    
    async fn extract(&self, carrier: &CarrierData, params: &EmbedParams) -> Result<Vec<u8>> {
        match carrier.format {
            DataFormat::SAM => {
                let sam = self.parse_sam(&carrier.raw_data)?;
                
                // Determine strategy from parameters
                let strategy = params.format_specific
                    .get("sam_strategy")
                    .and_then(|v| v.as_str())
                    .map(|s| match s {
                        "optional" => SamBamEmbedStrategy::OptionalFields,
                        "quality" => SamBamEmbedStrategy::QualityScores,
                        "comment" => SamBamEmbedStrategy::CommentLines,
                        "multi" => SamBamEmbedStrategy::MultiStrategy,
                        _ => SamBamEmbedStrategy::OptionalFields,
                    })
                    .unwrap_or(SamBamEmbedStrategy::OptionalFields);
                
                match strategy {
                    SamBamEmbedStrategy::OptionalFields => {
                        self.extract_sam_optional_fields(&sam)
                    },
                    SamBamEmbedStrategy::QualityScores => {
                        // For quality scores, we need to know the payload length
                        let estimated_length = 100; // Default estimate
                        self.extract_quality_scores(&sam, estimated_length)
                    },
                    SamBamEmbedStrategy::CommentLines => {
                        self.extract_comment_lines(&sam)
                    },
                    SamBamEmbedStrategy::MultiStrategy => {
                        // Try strategies in order
                        self.extract_sam_optional_fields(&sam)
                            .or_else(|_| self.extract_comment_lines(&sam))
                    },
                    _ => {
                        self.extract_sam_optional_fields(&sam)
                    }
                }
            },
            DataFormat::BAM => {
                Err(SteganoError::UnsupportedFormat(DataFormat::BAM))
            },
            _ => Err(SteganoError::UnsupportedFormat(carrier.format.clone())),
        }
    }
    
    fn verify_integrity(&self, carrier: &CarrierData) -> Result<bool> {
        match carrier.format {
            DataFormat::SAM => self.verify_integrity(&carrier.raw_data, false),
            DataFormat::BAM => self.verify_integrity(&carrier.raw_data, true),
            _ => Err(SteganoError::UnsupportedFormat(carrier.format.clone())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn create_minimal_sam() -> Vec<u8> {
        let sam_content = r#"@HD	VN:1.0	SO:coordinate
@SQ	SN:chr1	LN:248956422
@PG	ID:test	PN:test_program	VN:1.0
read1	0	chr1	100	60	100M	*	0	0	ATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCG	IIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIII
read2	0	chr1	200	60	100M	*	0	0	GCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTA	JJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJJ
"#;
        sam_content.as_bytes().to_vec()
    }

    #[test]
    fn test_sam_detection() {
        let handler = SamBamHandler::new();
        let sam_data = create_minimal_sam();
        assert_eq!(handler.detect_format(&sam_data), Some(DataFormat::SAM));
        
        // Test BAM detection
        let bam_magic = [b'B', b'A', b'M', 1, 0, 0, 0, 0];
        assert_eq!(handler.detect_format(&bam_magic), Some(DataFormat::BAM));
    }

    #[test]
    fn test_sam_parsing() {
        let handler = SamBamHandler::new();
        let sam_data = create_minimal_sam();
        
        let sam = handler.parse_sam(&sam_data).unwrap();
        assert_eq!(sam.header_lines.len(), 3);
        assert_eq!(sam.alignments.len(), 2);
        
        assert_eq!(sam.alignments[0].qname, "read1");
        assert_eq!(sam.alignments[0].pos, 100);
        assert_eq!(sam.alignments[1].qname, "read2");
        assert_eq!(sam.alignments[1].pos, 200);
    }

    #[test]
    fn test_sam_capacity_estimation() {
        let handler = SamBamHandler::new();
        let sam_data = create_minimal_sam();
        
        let carrier = CarrierData {
            format: DataFormat::SAM,
            raw_data: sam_data,
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };
        
        let capacity = handler.estimate_capacity(&carrier).unwrap();
        assert!(capacity.recommended_bits > 0);
        assert!(capacity.steganographic_ratio > 0.0);
    }

    #[tokio::test]
    async fn test_sam_optional_fields_roundtrip() {
        let handler = SamBamHandler::new();
        let sam_data = create_minimal_sam();
        
        let mut carrier = CarrierData {
            format: DataFormat::SAM,
            raw_data: sam_data,
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };
        
        let payload = b"secret alignment data";
        let mut params = EmbedParams {
            encryption_key: vec![],
            error_correction_level: ECLevel::None,
            stealth_level: StealthLevel::Low,
            use_gpu: false,
            format_specific: HashMap::new(),
        };
        params.format_specific.insert("sam_strategy".to_string(), 
            serde_json::Value::String("optional".to_string()));
        
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
    async fn test_sam_comment_lines_roundtrip() {
        let handler = SamBamHandler::new();
        let sam_data = create_minimal_sam();
        
        let mut carrier = CarrierData {
            format: DataFormat::SAM,
            raw_data: sam_data,
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };
        
        let payload = b"short secret";
        let mut params = EmbedParams {
            encryption_key: vec![],
            error_correction_level: ECLevel::None,
            stealth_level: StealthLevel::Low,
            use_gpu: false,
            format_specific: HashMap::new(),
        };
        params.format_specific.insert("sam_strategy".to_string(), 
            serde_json::Value::String("comment".to_string()));
        
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
    fn test_sam_optional_field_parsing() {
        let handler = SamBamHandler::new();
        
        let field_str = "NM:i:2";
        let field = handler.parse_optional_field(field_str).unwrap();
        assert_eq!(field.tag, "NM");
        assert_eq!(field.field_type, 'i');
        assert_eq!(field.value, "2");
    }

    #[test]
    fn test_sam_integrity_verification() {
        let handler = SamBamHandler::new();
        
        // Valid SAM
        let valid_sam = create_minimal_sam();
        assert!(handler.verify_integrity(&valid_sam, false).unwrap());
        
        // Invalid SAM (sequence/quality length mismatch)
        let invalid_sam = b"@HD\tVN:1.0\nread1\t0\tchr1\t100\t60\t10M\t*\t0\t0\tATCG\tIIIIII\n";
        assert!(!handler.verify_integrity(invalid_sam, false).unwrap());
    }
}