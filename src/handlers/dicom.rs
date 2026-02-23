//! DICOM (Digital Imaging and Communications in Medicine) handler
//!
//! Implements steganography techniques specifically designed for DICOM medical imaging files,
//! supporting both metadata embedding and pixel data steganography while preserving
//! medical image integrity and compliance with healthcare standards.

use super::DataHandler;
use crate::core::*;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;

/// Handler for DICOM medical imaging files.
///
/// Embeds payload data in private DICOM tags, preserving all mandatory
/// tags and pixel data so the file remains valid for medical viewers.
pub struct DicomHandler;

/// DICOM file structure for parsing
#[derive(Debug, Clone)]
pub struct DicomFile {
    /// DICOM preamble (128 bytes)
    pub preamble: Vec<u8>,

    /// DICOM prefix "DICM"
    pub prefix: String,

    /// Data elements (tags with values)
    pub data_elements: Vec<DicomDataElement>,

    /// Pixel data (if present)
    pub pixel_data: Option<DicomPixelData>,

    /// File meta information
    pub file_meta: DicomFileMeta,
}

/// DICOM data element
#[derive(Debug, Clone)]
pub struct DicomDataElement {
    /// Group number
    pub group: u16,

    /// Element number
    pub element: u16,

    /// Value representation
    pub vr: String,

    /// Value length
    pub length: u32,

    /// Raw value data
    pub value: Vec<u8>,

    /// Parsed value (when applicable)
    pub parsed_value: Option<DicomValue>,
}

/// DICOM value types
#[derive(Debug, Clone)]
pub enum DicomValue {
    /// Application Entity
    AE(String),
    /// Age String
    AS(String),
    /// Attribute Tag
    AT(u32),
    /// Code String
    CS(String),
    /// Date
    DA(String),
    /// Decimal String
    DS(String),
    /// Date Time
    DT(String),
    /// Floating Point Single
    FL(f32),
    /// Floating Point Double
    FD(f64),
    /// Integer String
    IS(String),
    /// Long String
    LO(String),
    /// Long Text
    LT(String),
    /// Other Byte
    OB(Vec<u8>),
    /// Other Double
    OD(Vec<f64>),
    /// Other Float
    OF(Vec<f32>),
    /// Other Long
    OL(Vec<u32>),
    /// Other Very Long
    OV(Vec<u64>),
    /// Other Word
    OW(Vec<u16>),
    /// Person Name
    PN(String),
    /// Short String
    SH(String),
    /// Signed Long
    SL(i32),
    /// Sequence of Items
    SQ(Vec<DicomDataElement>),
    /// Signed Short
    SS(i16),
    /// Short Text
    ST(String),
    /// Time
    TM(String),
    /// Unique Identifier
    UI(String),
    /// Unsigned Long
    UL(u32),
    /// Unknown
    UN(Vec<u8>),
    /// Unsigned Short
    US(u16),
    /// Unlimited Text
    UT(String),
}

/// DICOM pixel data information
#[derive(Debug, Clone)]
pub struct DicomPixelData {
    /// Samples per pixel
    pub samples_per_pixel: u16,

    /// Photometric interpretation
    pub photometric_interpretation: String,

    /// Rows
    pub rows: u16,

    /// Columns
    pub columns: u16,

    /// Bits allocated
    pub bits_allocated: u16,

    /// Bits stored
    pub bits_stored: u16,

    /// High bit
    pub high_bit: u16,

    /// Pixel representation
    pub pixel_representation: u16,

    /// Raw pixel data
    pub pixel_data: Vec<u8>,

    /// Transfer syntax UID
    pub transfer_syntax: String,
}

/// DICOM file meta information
#[derive(Debug, Clone)]
pub struct DicomFileMeta {
    /// File Meta Information Group Length
    pub group_length: u32,

    /// File Meta Information Version
    pub version: Vec<u8>,

    /// Media Storage SOP Class UID
    pub media_storage_sop_class_uid: String,

    /// Media Storage SOP Instance UID
    pub media_storage_sop_instance_uid: String,

    /// Transfer Syntax UID
    pub transfer_syntax_uid: String,

    /// Implementation Class UID
    pub implementation_class_uid: String,

    /// Implementation Version Name
    pub implementation_version_name: Option<String>,
}

/// DICOM embedding strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DicomEmbedStrategy {
    /// Embed in private data elements
    PrivateDataElements,

    /// Embed in pixel data LSB
    PixelDataLSB,

    /// Embed in unused metadata fields
    UnusedMetadata,

    /// Embed in sequence item padding
    SequencePadding,

    /// Multi-strategy embedding
    MultiStrategy,
}

/// DICOM embedding result
#[derive(Debug, Clone)]
pub struct DicomEmbedResult {
    pub success: bool,
    pub bytes_embedded: usize,
    pub strategy_used: DicomEmbedStrategy,
    pub elements_modified: usize,
    pub pixels_modified: usize,
    pub medical_integrity_preserved: bool,
    pub dicom_compliance_maintained: bool,
}

/// DICOM tag constants
const DICOM_PREAMBLE_LENGTH: usize = 128;
const DICOM_PREFIX: &str = "DICM";
#[allow(dead_code)]
const PRIVATE_GROUP_START: u16 = 0x0009; // Private groups start from 0x0009
#[allow(dead_code)]
const PRIVATE_GROUP_END: u16 = 0xFFFE; // Before sequence delimiters
const PIXEL_DATA_TAG: (u16, u16) = (0x7FE0, 0x0010);

/// Standard DICOM tags
mod dicom_tags {
    pub const FILE_META_INFO_GROUP_LENGTH: (u16, u16) = (0x0002, 0x0000);
    pub const FILE_META_INFO_VERSION: (u16, u16) = (0x0002, 0x0001);
    pub const MEDIA_STORAGE_SOP_CLASS_UID: (u16, u16) = (0x0002, 0x0002);
    pub const MEDIA_STORAGE_SOP_INSTANCE_UID: (u16, u16) = (0x0002, 0x0003);
    pub const TRANSFER_SYNTAX_UID: (u16, u16) = (0x0002, 0x0010);
    pub const IMPLEMENTATION_CLASS_UID: (u16, u16) = (0x0002, 0x0012);
    pub const IMPLEMENTATION_VERSION_NAME: (u16, u16) = (0x0002, 0x0013);

    #[allow(dead_code)]
    pub const SPECIFIC_CHARACTER_SET: (u16, u16) = (0x0008, 0x0005);
    #[allow(dead_code)]
    pub const IMAGE_TYPE: (u16, u16) = (0x0008, 0x0008);
    #[allow(dead_code)]
    pub const SOP_CLASS_UID: (u16, u16) = (0x0008, 0x0016);
    #[allow(dead_code)]
    pub const SOP_INSTANCE_UID: (u16, u16) = (0x0008, 0x0018);
    pub const STUDY_DATE: (u16, u16) = (0x0008, 0x0020);
    #[allow(dead_code)]
    pub const SERIES_DATE: (u16, u16) = (0x0008, 0x0021);
    #[allow(dead_code)]
    pub const ACQUISITION_DATE: (u16, u16) = (0x0008, 0x0022);
    #[allow(dead_code)]
    pub const CONTENT_DATE: (u16, u16) = (0x0008, 0x0023);

    pub const PATIENT_NAME: (u16, u16) = (0x0010, 0x0010);
    pub const PATIENT_ID: (u16, u16) = (0x0010, 0x0020);
    pub const PATIENT_BIRTH_DATE: (u16, u16) = (0x0010, 0x0030);
    pub const PATIENT_SEX: (u16, u16) = (0x0010, 0x0040);

    pub const SAMPLES_PER_PIXEL: (u16, u16) = (0x0028, 0x0002);
    pub const PHOTOMETRIC_INTERPRETATION: (u16, u16) = (0x0028, 0x0004);
    pub const ROWS: (u16, u16) = (0x0028, 0x0010);
    pub const COLUMNS: (u16, u16) = (0x0028, 0x0011);
    pub const BITS_ALLOCATED: (u16, u16) = (0x0028, 0x0100);
    pub const BITS_STORED: (u16, u16) = (0x0028, 0x0101);
    pub const HIGH_BIT: (u16, u16) = (0x0028, 0x0102);
    pub const PIXEL_REPRESENTATION: (u16, u16) = (0x0028, 0x0103);
}

impl Default for DicomHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl DicomHandler {
    pub fn new() -> Self {
        Self
    }

    /// Parse DICOM file
    fn parse_dicom(&self, data: &[u8]) -> Result<DicomFile> {
        if data.len() < DICOM_PREAMBLE_LENGTH + 4 {
            return Err(SteganoError::IntegrityError(
                "File too small to be DICOM".to_string(),
            ));
        }

        // Read preamble
        let preamble = data[..DICOM_PREAMBLE_LENGTH].to_vec();

        // Read and verify DICOM prefix
        let prefix_bytes = &data[DICOM_PREAMBLE_LENGTH..DICOM_PREAMBLE_LENGTH + 4];
        let prefix = String::from_utf8_lossy(prefix_bytes).to_string();

        if prefix != DICOM_PREFIX {
            return Err(SteganoError::IntegrityError(
                "Invalid DICOM prefix".to_string(),
            ));
        }

        // Parse data elements starting after preamble and prefix
        let mut offset = DICOM_PREAMBLE_LENGTH + 4;
        let mut data_elements = Vec::new();
        let mut pixel_data = None;
        let mut file_meta = DicomFileMeta::default();

        // Parse file meta information first (group 0x0002)
        while offset < data.len() {
            match self.parse_data_element(data, &mut offset) {
                Ok(element) => {
                    // Check if this is file meta information
                    if element.group == 0x0002 {
                        self.update_file_meta(&mut file_meta, &element);
                    }

                    // Check if this is pixel data
                    if (element.group, element.element) == PIXEL_DATA_TAG {
                        pixel_data = self.parse_pixel_data(&element, &data_elements);
                    }

                    // Check if we've moved past file meta information before moving element
                    let should_break = element.group > 0x0002 && !data_elements.is_empty();

                    data_elements.push(element);

                    // Break if we've moved past file meta information and found some data
                    if should_break {
                        // Continue parsing but we can optimize by breaking early for large files
                        if data_elements.len() > 1000 {
                            break;
                        }
                    }
                }
                Err(e) => {
                    log::warn!("Failed to parse DICOM element at offset {offset}: {e}");
                    break;
                }
            }
        }

        Ok(DicomFile {
            preamble,
            prefix,
            data_elements,
            pixel_data,
            file_meta,
        })
    }

    /// Parse individual DICOM data element
    fn parse_data_element(&self, data: &[u8], offset: &mut usize) -> Result<DicomDataElement> {
        if *offset + 8 > data.len() {
            return Err(SteganoError::IntegrityError(
                "Insufficient data for DICOM element".to_string(),
            ));
        }

        // Read group and element
        let group = u16::from_le_bytes([data[*offset], data[*offset + 1]]);
        let element = u16::from_le_bytes([data[*offset + 2], data[*offset + 3]]);
        *offset += 4;

        // Read VR (Value Representation)
        let vr = if group == 0x0002 || self.is_explicit_vr() {
            // Explicit VR
            let vr_bytes = &data[*offset..*offset + 2];
            *offset += 2;
            String::from_utf8_lossy(vr_bytes)
                .trim_end_matches('\0')
                .to_string()
        } else {
            // Implicit VR - infer from tag
            self.infer_vr(group, element)
        };

        // Read length
        let length = if self.has_explicit_length(&vr) {
            // Explicit length (4 bytes)
            if *offset + 4 > data.len() {
                return Err(SteganoError::IntegrityError(
                    "Insufficient data for length".to_string(),
                ));
            }
            let len = u32::from_le_bytes([
                data[*offset],
                data[*offset + 1],
                data[*offset + 2],
                data[*offset + 3],
            ]);
            *offset += 4;
            len
        } else {
            // Short length (2 bytes) followed by 2 reserved bytes
            if *offset + 4 > data.len() {
                return Err(SteganoError::IntegrityError(
                    "Insufficient data for length".to_string(),
                ));
            }
            let len = u16::from_le_bytes([data[*offset], data[*offset + 1]]) as u32;
            *offset += 4; // Skip reserved bytes
            len
        };

        // Read value
        let value = if length == 0xFFFFFFFF {
            // Undefined length - read until sequence delimiter
            self.read_undefined_length_value(data, offset)?
        } else {
            if *offset + length as usize > data.len() {
                return Err(SteganoError::IntegrityError(
                    "Insufficient data for value".to_string(),
                ));
            }
            let val = data[*offset..*offset + length as usize].to_vec();
            *offset += length as usize;
            val
        };

        // Parse value based on VR
        let parsed_value = self.parse_value(&vr, &value);

        Ok(DicomDataElement {
            group,
            element,
            vr,
            length,
            value,
            parsed_value,
        })
    }

    /// Check if transfer syntax uses explicit VR
    fn is_explicit_vr(&self) -> bool {
        // Simplified - assume explicit VR for file meta information
        true
    }

    /// Infer VR from tag for implicit VR transfer syntaxes
    fn infer_vr(&self, group: u16, element: u16) -> String {
        match (group, element) {
            dicom_tags::PATIENT_NAME => "PN".to_string(),
            dicom_tags::PATIENT_ID => "LO".to_string(),
            dicom_tags::PATIENT_BIRTH_DATE => "DA".to_string(),
            dicom_tags::PATIENT_SEX => "CS".to_string(),
            dicom_tags::STUDY_DATE => "DA".to_string(),
            dicom_tags::ROWS | dicom_tags::COLUMNS => "US".to_string(),
            dicom_tags::BITS_ALLOCATED | dicom_tags::BITS_STORED => "US".to_string(),
            PIXEL_DATA_TAG => "OW".to_string(),
            _ => "UN".to_string(), // Unknown
        }
    }

    /// Check if VR has explicit length encoding
    fn has_explicit_length(&self, vr: &str) -> bool {
        matches!(
            vr,
            "OB" | "OD" | "OF" | "OL" | "OV" | "OW" | "SQ" | "UC" | "UR" | "UT" | "UN"
        )
    }

    /// Read value with undefined length
    fn read_undefined_length_value(&self, data: &[u8], offset: &mut usize) -> Result<Vec<u8>> {
        let start = *offset;
        let mut value = Vec::new();

        // Look for sequence delimiter (FFFE,E0DD)
        while *offset + 4 <= data.len() {
            if data[*offset] == 0xFE
                && data[*offset + 1] == 0xFF
                && data[*offset + 2] == 0xDD
                && data[*offset + 3] == 0xE0
            {
                // Found sequence delimiter
                *offset += 8; // Skip delimiter and its length
                break;
            }
            value.push(data[*offset]);
            *offset += 1;
        }

        if value.is_empty() && *offset == start {
            return Err(SteganoError::IntegrityError(
                "No sequence delimiter found".to_string(),
            ));
        }

        Ok(value)
    }

    /// Parse DICOM value based on VR
    fn parse_value(&self, vr: &str, value: &[u8]) -> Option<DicomValue> {
        if value.is_empty() {
            return None;
        }

        match vr {
            "AE" | "CS" | "LO" | "LT" | "SH" | "ST" | "UT" => {
                let s = String::from_utf8_lossy(value)
                    .trim_end_matches('\0')
                    .trim()
                    .to_string();
                Some(match vr {
                    "AE" => DicomValue::AE(s),
                    "CS" => DicomValue::CS(s),
                    "LO" => DicomValue::LO(s),
                    "LT" => DicomValue::LT(s),
                    "SH" => DicomValue::SH(s),
                    "ST" => DicomValue::ST(s),
                    "UT" => DicomValue::UT(s),
                    _ => DicomValue::UN(value.to_vec()),
                })
            }
            "PN" => {
                let s = String::from_utf8_lossy(value)
                    .trim_end_matches('\0')
                    .trim()
                    .to_string();
                Some(DicomValue::PN(s))
            }
            "DA" | "DT" | "TM" => {
                let s = String::from_utf8_lossy(value)
                    .trim_end_matches('\0')
                    .trim()
                    .to_string();
                Some(match vr {
                    "DA" => DicomValue::DA(s),
                    "DT" => DicomValue::DT(s),
                    "TM" => DicomValue::TM(s),
                    _ => DicomValue::UN(value.to_vec()),
                })
            }
            "UI" => {
                let s = String::from_utf8_lossy(value)
                    .trim_end_matches('\0')
                    .trim()
                    .to_string();
                Some(DicomValue::UI(s))
            }
            "US" => {
                if value.len() >= 2 {
                    let val = u16::from_le_bytes([value[0], value[1]]);
                    Some(DicomValue::US(val))
                } else {
                    None
                }
            }
            "UL" => {
                if value.len() >= 4 {
                    let val = u32::from_le_bytes([value[0], value[1], value[2], value[3]]);
                    Some(DicomValue::UL(val))
                } else {
                    None
                }
            }
            "SS" => {
                if value.len() >= 2 {
                    let val = i16::from_le_bytes([value[0], value[1]]);
                    Some(DicomValue::SS(val))
                } else {
                    None
                }
            }
            "SL" => {
                if value.len() >= 4 {
                    let val = i32::from_le_bytes([value[0], value[1], value[2], value[3]]);
                    Some(DicomValue::SL(val))
                } else {
                    None
                }
            }
            "FL" => {
                if value.len() >= 4 {
                    let val = f32::from_le_bytes([value[0], value[1], value[2], value[3]]);
                    Some(DicomValue::FL(val))
                } else {
                    None
                }
            }
            "FD" => {
                if value.len() >= 8 {
                    let val = f64::from_le_bytes([
                        value[0], value[1], value[2], value[3], value[4], value[5], value[6],
                        value[7],
                    ]);
                    Some(DicomValue::FD(val))
                } else {
                    None
                }
            }
            "OB" | "OW" | "OD" | "OF" | "UN" => Some(match vr {
                "OB" | "UN" => DicomValue::OB(value.to_vec()),
                "OW" => {
                    let words: Vec<u16> = value
                        .chunks(2)
                        .map(|chunk| {
                            if chunk.len() == 2 {
                                u16::from_le_bytes([chunk[0], chunk[1]])
                            } else {
                                chunk[0] as u16
                            }
                        })
                        .collect();
                    DicomValue::OW(words)
                }
                "OD" => {
                    let doubles: Vec<f64> = value
                        .chunks(8)
                        .filter_map(|chunk| {
                            if chunk.len() == 8 {
                                Some(f64::from_le_bytes([
                                    chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5],
                                    chunk[6], chunk[7],
                                ]))
                            } else {
                                None
                            }
                        })
                        .collect();
                    DicomValue::OD(doubles)
                }
                "OF" => {
                    let floats: Vec<f32> = value
                        .chunks(4)
                        .filter_map(|chunk| {
                            if chunk.len() == 4 {
                                Some(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                            } else {
                                None
                            }
                        })
                        .collect();
                    DicomValue::OF(floats)
                }
                _ => DicomValue::OB(value.to_vec()),
            }),
            _ => {
                // Unknown VR - store as bytes
                Some(DicomValue::UN(value.to_vec()))
            }
        }
    }

    /// Update file meta information from parsed element
    fn update_file_meta(&self, file_meta: &mut DicomFileMeta, element: &DicomDataElement) {
        match (element.group, element.element) {
            dicom_tags::FILE_META_INFO_GROUP_LENGTH => {
                if let Some(DicomValue::UL(val)) = &element.parsed_value {
                    file_meta.group_length = *val;
                }
            }
            dicom_tags::FILE_META_INFO_VERSION => {
                file_meta.version = element.value.clone();
            }
            dicom_tags::MEDIA_STORAGE_SOP_CLASS_UID => {
                if let Some(DicomValue::UI(val)) = &element.parsed_value {
                    file_meta.media_storage_sop_class_uid = val.clone();
                }
            }
            dicom_tags::MEDIA_STORAGE_SOP_INSTANCE_UID => {
                if let Some(DicomValue::UI(val)) = &element.parsed_value {
                    file_meta.media_storage_sop_instance_uid = val.clone();
                }
            }
            dicom_tags::TRANSFER_SYNTAX_UID => {
                if let Some(DicomValue::UI(val)) = &element.parsed_value {
                    file_meta.transfer_syntax_uid = val.clone();
                }
            }
            dicom_tags::IMPLEMENTATION_CLASS_UID => {
                if let Some(DicomValue::UI(val)) = &element.parsed_value {
                    file_meta.implementation_class_uid = val.clone();
                }
            }
            dicom_tags::IMPLEMENTATION_VERSION_NAME => {
                if let Some(DicomValue::SH(val)) = &element.parsed_value {
                    file_meta.implementation_version_name = Some(val.clone());
                }
            }
            _ => {}
        }
    }

    /// Parse pixel data from DICOM elements
    fn parse_pixel_data(
        &self,
        pixel_element: &DicomDataElement,
        elements: &[DicomDataElement],
    ) -> Option<DicomPixelData> {
        // Get image parameters from other elements
        let mut samples_per_pixel = 1;
        let mut photometric_interpretation = "MONOCHROME2".to_string();
        let mut rows = 0;
        let mut columns = 0;
        let mut bits_allocated = 16;
        let mut bits_stored = 16;
        let mut high_bit = 15;
        let mut pixel_representation = 0;

        for element in elements {
            match (element.group, element.element) {
                dicom_tags::SAMPLES_PER_PIXEL => {
                    if let Some(DicomValue::US(val)) = &element.parsed_value {
                        samples_per_pixel = *val;
                    }
                }
                dicom_tags::PHOTOMETRIC_INTERPRETATION => {
                    if let Some(DicomValue::CS(val)) = &element.parsed_value {
                        photometric_interpretation = val.clone();
                    }
                }
                dicom_tags::ROWS => {
                    if let Some(DicomValue::US(val)) = &element.parsed_value {
                        rows = *val;
                    }
                }
                dicom_tags::COLUMNS => {
                    if let Some(DicomValue::US(val)) = &element.parsed_value {
                        columns = *val;
                    }
                }
                dicom_tags::BITS_ALLOCATED => {
                    if let Some(DicomValue::US(val)) = &element.parsed_value {
                        bits_allocated = *val;
                    }
                }
                dicom_tags::BITS_STORED => {
                    if let Some(DicomValue::US(val)) = &element.parsed_value {
                        bits_stored = *val;
                    }
                }
                dicom_tags::HIGH_BIT => {
                    if let Some(DicomValue::US(val)) = &element.parsed_value {
                        high_bit = *val;
                    }
                }
                dicom_tags::PIXEL_REPRESENTATION => {
                    if let Some(DicomValue::US(val)) = &element.parsed_value {
                        pixel_representation = *val;
                    }
                }
                _ => {}
            }
        }

        // Validate essential parameters
        if rows == 0 || columns == 0 {
            return None;
        }

        Some(DicomPixelData {
            samples_per_pixel,
            photometric_interpretation,
            rows,
            columns,
            bits_allocated,
            bits_stored,
            high_bit,
            pixel_representation,
            pixel_data: pixel_element.value.clone(),
            transfer_syntax: "1.2.840.10008.1.2".to_string(), // Implicit VR Little Endian
        })
    }

    /// Embed payload using private data elements
    fn embed_private_data_elements(
        &self,
        dicom: &mut DicomFile,
        payload: &[u8],
    ) -> Result<DicomEmbedResult> {
        // Create payload with length header
        let mut full_payload = Vec::new();
        let payload_len = payload.len() as u32;
        full_payload.extend_from_slice(&payload_len.to_le_bytes());
        full_payload.extend_from_slice(payload);

        // Split payload into chunks
        let chunk_size = 64; // Maximum bytes per private element
        let chunks: Vec<&[u8]> = full_payload.chunks(chunk_size).collect();

        let mut elements_modified = 0;
        let group_number = 0x7777; // Use a specific private group

        // Add private data elements
        for (i, chunk) in chunks.iter().enumerate() {
            // Create private element tag
            let element_number = 0x0010 + (i as u16);

            let private_element = DicomDataElement {
                group: group_number,
                element: element_number,
                vr: "OB".to_string(),
                length: chunk.len() as u32,
                value: chunk.to_vec(),
                parsed_value: Some(DicomValue::OB(chunk.to_vec())),
            };

            dicom.data_elements.push(private_element);
            elements_modified += 1;
        }

        Ok(DicomEmbedResult {
            success: true,
            bytes_embedded: payload.len(),
            strategy_used: DicomEmbedStrategy::PrivateDataElements,
            elements_modified,
            pixels_modified: 0,
            medical_integrity_preserved: true, // Private elements don't affect medical data
            dicom_compliance_maintained: true, // Private elements are DICOM compliant
        })
    }

    /// Extract payload from private data elements
    fn extract_private_data_elements(&self, dicom: &DicomFile) -> Result<Vec<u8>> {
        let mut private_elements: Vec<&DicomDataElement> = dicom
            .data_elements
            .iter()
            .filter(|e| e.group == 0x7777 && e.element >= 0x0010)
            .collect();

        // Sort by element number to maintain order
        private_elements.sort_by_key(|e| e.element);

        if private_elements.is_empty() {
            return Err(SteganoError::IntegrityError(
                "No private steganographic data found".to_string(),
            ));
        }

        // Reconstruct payload
        let mut full_payload = Vec::new();
        for element in private_elements {
            full_payload.extend_from_slice(&element.value);
        }

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

        // Validate and extract payload
        if payload_len + 4 > full_payload.len() {
            return Err(SteganoError::IntegrityError(
                "Invalid payload length in extracted data".to_string(),
            ));
        }

        let payload = full_payload[4..4 + payload_len].to_vec();
        Ok(payload)
    }

    /// Embed payload in pixel data using LSB
    fn embed_pixel_data_lsb(
        &self,
        dicom: &mut DicomFile,
        payload: &[u8],
    ) -> Result<DicomEmbedResult> {
        let pixel_data = dicom.pixel_data.as_mut().ok_or_else(|| {
            SteganoError::IntegrityError("No pixel data found in DICOM".to_string())
        })?;

        // Convert payload to bits
        let mut payload_bits = Vec::new();
        for byte in payload {
            for i in 0..8 {
                payload_bits.push((byte >> i) & 1);
            }
        }

        // Check capacity
        let available_bits = pixel_data.pixel_data.len();
        if payload_bits.len() > available_bits {
            return Err(SteganoError::InsufficientCapacity {
                needed: payload_bits.len(),
                available: available_bits,
            });
        }

        let mut pixels_modified = 0;

        // Embed bits in LSB of pixel data
        for (i, &bit) in payload_bits.iter().enumerate() {
            if i < pixel_data.pixel_data.len() {
                // Clear LSB and set new bit
                pixel_data.pixel_data[i] = (pixel_data.pixel_data[i] & 0xFE) | bit;
                pixels_modified += 1;
            }
        }

        Ok(DicomEmbedResult {
            success: true,
            bytes_embedded: payload.len(),
            strategy_used: DicomEmbedStrategy::PixelDataLSB,
            elements_modified: 1, // Only pixel data element modified
            pixels_modified,
            medical_integrity_preserved: true, // LSB changes are minimal
            dicom_compliance_maintained: true,
        })
    }

    /// Extract payload from pixel data LSB
    fn extract_pixel_data_lsb(&self, dicom: &DicomFile, payload_length: usize) -> Result<Vec<u8>> {
        let pixel_data = dicom.pixel_data.as_ref().ok_or_else(|| {
            SteganoError::IntegrityError("No pixel data found in DICOM".to_string())
        })?;

        let required_bits = payload_length * 8;

        if required_bits > pixel_data.pixel_data.len() {
            return Err(SteganoError::IntegrityError(
                "Insufficient pixel data for extraction".to_string(),
            ));
        }

        // Extract bits from LSB
        let mut extracted_bits = Vec::new();
        for i in 0..required_bits {
            if i < pixel_data.pixel_data.len() {
                extracted_bits.push(pixel_data.pixel_data[i] & 1);
            }
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

    /// Estimate capacity for DICOM file
    fn estimate_dicom_capacity(
        &self,
        data: &[u8],
        strategy: DicomEmbedStrategy,
    ) -> Result<CapacityInfo> {
        let dicom = self.parse_dicom(data)?;

        let estimated_bytes = match strategy {
            DicomEmbedStrategy::PrivateDataElements => {
                // Conservative estimate: can add many private elements
                std::cmp::min(data.len() / 10, 10000) // Up to 10KB or 10% of file size
            }
            DicomEmbedStrategy::PixelDataLSB => {
                if let Some(ref pixel_data) = dicom.pixel_data {
                    pixel_data.pixel_data.len() / 8 // 1 bit per byte = 1/8 capacity
                } else {
                    0
                }
            }
            DicomEmbedStrategy::UnusedMetadata => {
                // Can add custom metadata fields
                data.len() / 100 // Conservative 1% estimate
            }
            DicomEmbedStrategy::SequencePadding => {
                // Padding in sequence items
                data.len() / 200 // Very conservative estimate
            }
            DicomEmbedStrategy::MultiStrategy => {
                // Combined capacity of multiple strategies
                let private_capacity = std::cmp::min(data.len() / 10, 10000);
                let pixel_capacity = if let Some(ref pixel_data) = dicom.pixel_data {
                    pixel_data.pixel_data.len() / 8
                } else {
                    0
                };
                let metadata_capacity = data.len() / 100;

                private_capacity + pixel_capacity + metadata_capacity
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
            "estimated_capacity_bytes".to_string(),
            serde_json::Value::Number(estimated_bytes.into()),
        );

        if let Some(ref pixel_data) = dicom.pixel_data {
            format_info.insert(
                "image_dimensions".to_string(),
                serde_json::Value::String(format!("{}x{}", pixel_data.columns, pixel_data.rows)),
            );
            format_info.insert(
                "bits_per_pixel".to_string(),
                serde_json::Value::Number(pixel_data.bits_allocated.into()),
            );
        }

        format_info.insert(
            "data_elements".to_string(),
            serde_json::Value::Number(dicom.data_elements.len().into()),
        );

        Ok(CapacityInfo {
            total_bits,
            recommended_bits,
            steganographic_ratio: recommended_bits as f64 / total_bits as f64,
            format_specific_info: format_info,
        })
    }

    /// Serialize DICOM file back to binary format
    fn serialize_dicom(&self, dicom: &DicomFile) -> Vec<u8> {
        let mut result = Vec::new();

        // Add preamble
        result.extend_from_slice(&dicom.preamble);

        // Add DICOM prefix
        result.extend_from_slice(dicom.prefix.as_bytes());

        // Add data elements
        for element in &dicom.data_elements {
            // Group and element
            result.extend_from_slice(&element.group.to_le_bytes());
            result.extend_from_slice(&element.element.to_le_bytes());

            // VR (if explicit)
            if element.group == 0x0002 || self.is_explicit_vr() {
                let mut vr_bytes = element.vr.as_bytes().to_vec();
                vr_bytes.resize(2, 0); // Pad to 2 bytes
                result.extend_from_slice(&vr_bytes[..2]);
            }

            // Length
            if self.has_explicit_length(&element.vr) {
                result.extend_from_slice(&element.length.to_le_bytes());
            } else {
                result.extend_from_slice(&(element.length as u16).to_le_bytes());
                result.extend_from_slice(&[0, 0]); // Reserved bytes
            }

            // Value
            result.extend_from_slice(&element.value);

            // Ensure even length (DICOM requirement)
            if element.value.len() % 2 == 1 {
                result.push(0);
            }
        }

        result
    }

    /// Verify DICOM medical integrity
    fn verify_dicom_integrity(&self, data: &[u8]) -> Result<bool> {
        match self.parse_dicom(data) {
            Ok(dicom) => {
                // Check essential DICOM structure
                if dicom.prefix != DICOM_PREFIX {
                    return Ok(false);
                }

                // Verify file meta information is present
                let has_meta_info = dicom.data_elements.iter().any(|e| e.group == 0x0002);

                if !has_meta_info {
                    return Ok(false);
                }

                // Check pixel data consistency if present
                if let Some(ref pixel_data) = dicom.pixel_data {
                    let expected_size = pixel_data.rows as usize
                        * pixel_data.columns as usize
                        * pixel_data.samples_per_pixel as usize
                        * (pixel_data.bits_allocated as usize / 8);

                    // Allow some tolerance for compression
                    if pixel_data.pixel_data.len() < expected_size / 2 {
                        return Ok(false);
                    }
                }

                Ok(true)
            }
            Err(_) => Ok(false),
        }
    }

    /// Check if data is DICOM format
    fn is_dicom(&self, data: &[u8]) -> bool {
        if data.len() < DICOM_PREAMBLE_LENGTH + 4 {
            return false;
        }

        let prefix_start = DICOM_PREAMBLE_LENGTH;
        let prefix_bytes = &data[prefix_start..prefix_start + 4];
        let prefix = String::from_utf8_lossy(prefix_bytes);

        prefix == DICOM_PREFIX
    }
}

impl Default for DicomFileMeta {
    fn default() -> Self {
        Self {
            group_length: 0,
            version: vec![0, 1],
            media_storage_sop_class_uid: String::new(),
            media_storage_sop_instance_uid: String::new(),
            transfer_syntax_uid: "1.2.840.10008.1.2".to_string(), // Implicit VR Little Endian
            implementation_class_uid: "1.2.840.10008.1.2.1".to_string(),
            implementation_version_name: None,
        }
    }
}

#[async_trait]
impl DataHandler for DicomHandler {
    fn detect_format(&self, data: &[u8]) -> Option<DataFormat> {
        if self.is_dicom(data) {
            Some(DataFormat::DICOM)
        } else {
            None
        }
    }

    fn estimate_capacity(&self, carrier: &CarrierData) -> Result<CapacityInfo> {
        match carrier.format {
            DataFormat::DICOM => {
                // Use private data elements as default strategy
                self.estimate_dicom_capacity(
                    &carrier.raw_data,
                    DicomEmbedStrategy::PrivateDataElements,
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
            DataFormat::DICOM => {
                let mut dicom = self.parse_dicom(&carrier.raw_data)?;

                // Determine strategy from parameters
                let strategy = params
                    .format_specific
                    .get("dicom_strategy")
                    .and_then(|v| v.as_str())
                    .map(|s| match s {
                        "private" => DicomEmbedStrategy::PrivateDataElements,
                        "pixel_lsb" => DicomEmbedStrategy::PixelDataLSB,
                        "metadata" => DicomEmbedStrategy::UnusedMetadata,
                        "sequence" => DicomEmbedStrategy::SequencePadding,
                        "multi" => DicomEmbedStrategy::MultiStrategy,
                        _ => DicomEmbedStrategy::PrivateDataElements,
                    })
                    .unwrap_or(DicomEmbedStrategy::PrivateDataElements);

                let embed_result = match strategy {
                    DicomEmbedStrategy::PrivateDataElements => {
                        self.embed_private_data_elements(&mut dicom, payload)?
                    }
                    DicomEmbedStrategy::PixelDataLSB => {
                        self.embed_pixel_data_lsb(&mut dicom, payload)?
                    }
                    DicomEmbedStrategy::MultiStrategy => {
                        // Try private data elements first, fallback to pixel LSB
                        self.embed_private_data_elements(&mut dicom, payload)
                            .or_else(|_| self.embed_pixel_data_lsb(&mut dicom, payload))?
                    }
                    _ => {
                        // For other strategies, use private data elements as fallback
                        self.embed_private_data_elements(&mut dicom, payload)?
                    }
                };

                // Serialize modified DICOM back to data
                carrier.raw_data = self.serialize_dicom(&dicom);

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
            DataFormat::DICOM => {
                let dicom = self.parse_dicom(&carrier.raw_data)?;

                // Determine strategy from parameters
                let strategy = params
                    .format_specific
                    .get("dicom_strategy")
                    .and_then(|v| v.as_str())
                    .map(|s| match s {
                        "private" => DicomEmbedStrategy::PrivateDataElements,
                        "pixel_lsb" => DicomEmbedStrategy::PixelDataLSB,
                        "metadata" => DicomEmbedStrategy::UnusedMetadata,
                        "sequence" => DicomEmbedStrategy::SequencePadding,
                        "multi" => DicomEmbedStrategy::MultiStrategy,
                        _ => DicomEmbedStrategy::PrivateDataElements,
                    })
                    .unwrap_or(DicomEmbedStrategy::PrivateDataElements);

                match strategy {
                    DicomEmbedStrategy::PrivateDataElements => {
                        self.extract_private_data_elements(&dicom)
                    }
                    DicomEmbedStrategy::PixelDataLSB => {
                        // For LSB, we need to know the payload length
                        // This is a limitation - in practice, would store length in metadata
                        let estimated_length = 100; // Default estimate
                        self.extract_pixel_data_lsb(&dicom, estimated_length)
                    }
                    DicomEmbedStrategy::MultiStrategy => {
                        // Try strategies in order of preference
                        self.extract_private_data_elements(&dicom).or_else(|_| {
                            let estimated_length = 100;
                            self.extract_pixel_data_lsb(&dicom, estimated_length)
                        })
                    }
                    _ => {
                        // For other strategies, use private data elements as fallback
                        self.extract_private_data_elements(&dicom)
                    }
                }
            }
            _ => Err(SteganoError::UnsupportedFormat(carrier.format.clone())),
        }
    }

    fn verify_integrity(&self, carrier: &CarrierData) -> Result<bool> {
        match carrier.format {
            DataFormat::DICOM => self.verify_dicom_integrity(&carrier.raw_data),
            _ => Err(SteganoError::UnsupportedFormat(carrier.format.clone())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn create_minimal_dicom() -> Vec<u8> {
        let mut data = Vec::new();

        // Preamble (128 bytes of zeros)
        data.extend_from_slice(&[0u8; 128]);

        // DICOM prefix
        data.extend_from_slice(b"DICM");

        // File Meta Information Group Length (0002,0000)
        data.extend_from_slice(&[0x02, 0x00, 0x00, 0x00]); // Tag
        data.extend_from_slice(b"UL"); // VR
        data.extend_from_slice(&[0x04, 0x00]); // Length
        data.extend_from_slice(&[0x00, 0x00]); // Reserved
        data.extend_from_slice(&[0x04, 0x00, 0x00, 0x00]); // Value: 4

        // Transfer Syntax UID (0002,0010)
        data.extend_from_slice(&[0x02, 0x00, 0x10, 0x00]); // Tag
        data.extend_from_slice(b"UI"); // VR
        data.extend_from_slice(&[0x1A, 0x00]); // Length
        data.extend_from_slice(&[0x00, 0x00]); // Reserved
        data.extend_from_slice(b"1.2.840.10008.1.2\0"); // Implicit VR Little Endian

        data
    }

    #[test]
    fn test_dicom_detection() {
        let handler = DicomHandler::new();
        let dicom_data = create_minimal_dicom();
        assert_eq!(handler.detect_format(&dicom_data), Some(DataFormat::DICOM));

        // Test non-DICOM data
        let non_dicom = b"Not a DICOM file";
        assert_eq!(handler.detect_format(non_dicom), None);
    }

    #[test]
    fn test_dicom_parsing() {
        let handler = DicomHandler::new();
        let dicom_data = create_minimal_dicom();

        let dicom = handler.parse_dicom(&dicom_data).unwrap();
        assert_eq!(dicom.prefix, "DICM");
        assert_eq!(dicom.preamble.len(), 128);
        assert!(!dicom.data_elements.is_empty());
    }

    #[test]
    fn test_dicom_capacity_estimation() {
        let handler = DicomHandler::new();
        let dicom_data = create_minimal_dicom();

        let carrier = CarrierData {
            format: DataFormat::DICOM,
            raw_data: dicom_data,
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };

        let capacity = handler.estimate_capacity(&carrier).unwrap();
        assert!(capacity.recommended_bits > 0);
        assert!(capacity.steganographic_ratio > 0.0);
    }

    #[tokio::test]
    async fn test_dicom_private_elements_roundtrip() {
        let handler = DicomHandler::new();
        let dicom_data = create_minimal_dicom();

        let mut carrier = CarrierData {
            format: DataFormat::DICOM,
            raw_data: dicom_data,
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };

        let payload = b"secret medical data";
        let mut params = EmbedParams {
            encryption_key: vec![],
            error_correction_level: ECLevel::None,
            stealth_level: StealthLevel::Low,
            use_gpu: false,
            enable_validation: false,
            format_specific: HashMap::new(),
        };
        params.format_specific.insert(
            "dicom_strategy".to_string(),
            serde_json::Value::String("private".to_string()),
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

    #[test]
    fn test_dicom_value_parsing() {
        let handler = DicomHandler::new();

        // Test string value
        let value = b"TEST_VALUE\0\0";
        let parsed = handler.parse_value("LO", value);
        assert!(matches!(parsed, Some(DicomValue::LO(_))));

        // Test numeric value
        let value = [0x34, 0x12]; // Little endian 0x1234
        let parsed = handler.parse_value("US", &value);
        assert!(matches!(parsed, Some(DicomValue::US(0x1234))));
    }

    #[test]
    fn test_dicom_integrity_verification() {
        let handler = DicomHandler::new();

        // Valid DICOM
        let valid_dicom = create_minimal_dicom();
        assert!(handler.verify_dicom_integrity(&valid_dicom).unwrap());

        // Invalid DICOM (corrupted prefix)
        let mut invalid_dicom = valid_dicom.clone();
        invalid_dicom[130] = b'X'; // Corrupt the 'D' in "DICM"
        assert!(!handler.verify_dicom_integrity(&invalid_dicom).unwrap());
    }
}
