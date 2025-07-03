//! Basic phantom-science integration example
//!
//! This example demonstrates how to integrate stegano with phantom-science
//! for genomics data steganography operations.

use stegano::*;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Phantom-science integration metadata structure
#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct PhantomScienceMetadata {
    pub sample_id: String,
    pub processing_timestamp: u64,
    pub chain_of_custody: Vec<String>,
    pub quality_metrics: HashMap<String, f64>,
    pub lab_notes: String,
}

impl PhantomScienceMetadata {
    pub fn new(sample_id: &str) -> Self {
        Self {
            sample_id: sample_id.to_string(),
            processing_timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            chain_of_custody: vec!["phantom-science-lab".to_string()],
            quality_metrics: HashMap::new(),
            lab_notes: String::new(),
        }
    }
    
    pub fn add_processing_step(&mut self, step: &str) {
        self.chain_of_custody.push(step.to_string());
    }
    
    pub fn add_quality_metric(&mut self, name: &str, value: f64) {
        self.quality_metrics.insert(name.to_string(), value);
    }
    
    pub fn set_lab_notes(&mut self, notes: &str) {
        self.lab_notes = notes.to_string();
    }
    
    pub fn to_json(&self) -> Result<Vec<u8>> {
        serde_json::to_vec(self)
            .map_err(|e| SteganoError::InvalidParameter(format!("JSON serialization failed: {}", e)))
    }
    
    pub fn from_json(data: &[u8]) -> Result<Self> {
        serde_json::from_slice(data)
            .map_err(|e| SteganoError::InvalidParameter(format!("JSON deserialization failed: {}", e)))
    }
}

/// Phantom-science integration helper
pub struct PhantomScienceIntegration {
    registry: handlers::HandlerRegistry,
}

impl PhantomScienceIntegration {
    pub fn new() -> Self {
        Self {
            registry: handlers::HandlerRegistry::new(),
        }
    }
    
    /// Embed phantom-science metadata into genomics data
    pub async fn embed_metadata(
        &self,
        genomics_file: &Path,
        metadata: &PhantomScienceMetadata,
        password: Option<&str>,
        output_file: &Path,
    ) -> Result<EmbedResult> {
        // Read the genomics file
        let genomics_data = fs::read(genomics_file)
            .map_err(|e| SteganoError::IoError(e))?;
        
        // Detect format
        let mut carrier = CarrierData {
            format: self.detect_genomics_format(&genomics_data)?,
            raw_data: genomics_data,
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };
        
        // Serialize metadata
        let metadata_json = metadata.to_json()?;
        
        // Setup embedding parameters
        let params = EmbedParams {
            encryption_key: password.map(|p| p.as_bytes().to_vec()).unwrap_or_default(),
            error_correction_level: ECLevel::Medium,
            stealth_level: StealthLevel::Low,
            use_gpu: false,
            format_specific: HashMap::new(),
        };
        
        // Get appropriate handler
        let handler = self.registry.get_handler(&carrier.format)
            .ok_or_else(|| SteganoError::UnsupportedFormat(carrier.format.clone()))?;
        
        // Embed metadata
        let result = handler.embed(&mut carrier, &metadata_json, &params).await?;
        
        // Write output file
        fs::write(output_file, &carrier.raw_data)
            .map_err(|e| SteganoError::IoError(e))?;
        
        Ok(result)
    }
    
    /// Extract phantom-science metadata from genomics data
    pub async fn extract_metadata(
        &self,
        genomics_file: &Path,
        password: Option<&str>,
    ) -> Result<PhantomScienceMetadata> {
        // Read the genomics file
        let genomics_data = fs::read(genomics_file)
            .map_err(|e| SteganoError::IoError(e))?;
        
        // Detect format
        let carrier = CarrierData {
            format: self.detect_genomics_format(&genomics_data)?,
            raw_data: genomics_data,
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };
        
        // Setup extraction parameters
        let params = EmbedParams {
            encryption_key: password.map(|p| p.as_bytes().to_vec()).unwrap_or_default(),
            error_correction_level: ECLevel::Medium,
            stealth_level: StealthLevel::Low,
            use_gpu: false,
            format_specific: HashMap::new(),
        };
        
        // Get appropriate handler
        let handler = self.registry.get_handler(&carrier.format)
            .ok_or_else(|| SteganoError::UnsupportedFormat(carrier.format.clone()))?;
        
        // Extract metadata
        let metadata_json = handler.extract(&carrier, &params).await?;
        
        // Deserialize metadata
        PhantomScienceMetadata::from_json(&metadata_json)
    }
    
    /// Analyze capacity for phantom-science metadata
    pub fn analyze_capacity(&self, genomics_file: &Path) -> Result<CapacityInfo> {
        // Read the genomics file
        let genomics_data = fs::read(genomics_file)
            .map_err(|e| SteganoError::IoError(e))?;
        
        // Create carrier data
        let carrier = CarrierData {
            format: self.detect_genomics_format(&genomics_data)?,
            raw_data: genomics_data,
            metadata: HashMap::new(),
            dimensions: None,
            sampling_rate: None,
        };
        
        // Get appropriate handler
        let handler = self.registry.get_handler(&carrier.format)
            .ok_or_else(|| SteganoError::UnsupportedFormat(carrier.format.clone()))?;
        
        // Estimate capacity
        handler.estimate_capacity(&carrier)
    }
    
    /// Detect genomics format from data
    fn detect_genomics_format(&self, data: &[u8]) -> Result<DataFormat> {
        let handler = handlers::genomics::GenomicsHandler::new();
        handler.detect_format(data)
            .ok_or_else(|| SteganoError::UnsupportedFormat(DataFormat::BINARY))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("🧬 Phantom-Science Integration Demo");
    println!("===================================\n");
    
    // Initialize phantom-science integration
    let integration = PhantomScienceIntegration::new();
    
    // Create sample FASTA data
    let sample_fasta = ">sample_001 Homo sapiens chromosome 1, reference sequence
ATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCG
GCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTA
TTTTAAAACCCCGGGGTTTTAAAACCCCGGGGTTTTAAAACCCCGGGGTTTTAAAACCCC
>sample_002 Homo sapiens chromosome 2, reference sequence
GGGGCCCCAAAATTTTGGGGCCCCAAAATTTTGGGGCCCCAAAATTTTGGGGCCCCAAAA
ATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCGATCG
CGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTA";
    
    // Write sample file
    let input_file = Path::new("/tmp/phantom_science_sample.fasta");
    let output_file = Path::new("/tmp/phantom_science_embedded.fasta");
    fs::write(input_file, sample_fasta).unwrap();
    
    // Create phantom-science metadata
    let mut metadata = PhantomScienceMetadata::new("PS-2025-001");
    metadata.add_processing_step("DNA extraction");
    metadata.add_processing_step("Library preparation");
    metadata.add_processing_step("Sequencing");
    metadata.add_quality_metric("read_count", 1000000.0);
    metadata.add_quality_metric("quality_score", 35.2);
    metadata.add_quality_metric("gc_content", 42.5);
    metadata.set_lab_notes("High quality sample, good coverage across all regions");
    
    println!("📊 Original Metadata:");
    println!("{:#?}\n", metadata);
    
    // Analyze capacity
    println!("📈 Analyzing capacity...");
    let capacity = integration.analyze_capacity(input_file)?;
    println!("   Total capacity: {} bits ({} bytes)", 
             capacity.total_bits, capacity.total_bits / 8);
    println!("   Recommended capacity: {} bits ({} bytes)", 
             capacity.recommended_bits, capacity.recommended_bits / 8);
    println!("   Steganographic ratio: {:.2}%\n", 
             capacity.steganographic_ratio * 100.0);
    
    // Embed metadata
    println!("🔐 Embedding metadata...");
    let password = "phantom-science-secure-2025";
    let embed_result = integration.embed_metadata(
        input_file,
        &metadata,
        Some(password),
        output_file,
    ).await?;
    
    println!("   Embedded {} bytes", embed_result.bytes_embedded);
    println!("   Embedding time: {} ms", embed_result.performance_stats.embedding_time_ms);
    println!("   Success: {}\n", embed_result.success);
    
    // Extract metadata
    println!("🔍 Extracting metadata...");
    let extracted_metadata = integration.extract_metadata(
        output_file,
        Some(password),
    ).await?;
    
    println!("📊 Extracted Metadata:");
    println!("{:#?}\n", extracted_metadata);
    
    // Verify roundtrip
    println!("✅ Verification:");
    println!("   Sample ID matches: {}", 
             metadata.sample_id == extracted_metadata.sample_id);
    println!("   Timestamp matches: {}", 
             metadata.processing_timestamp == extracted_metadata.processing_timestamp);
    println!("   Chain of custody matches: {}", 
             metadata.chain_of_custody == extracted_metadata.chain_of_custody);
    println!("   Quality metrics match: {}", 
             metadata.quality_metrics == extracted_metadata.quality_metrics);
    println!("   Lab notes match: {}", 
             metadata.lab_notes == extracted_metadata.lab_notes);
    
    // Cleanup
    let _ = fs::remove_file(input_file);
    let _ = fs::remove_file(output_file);
    
    println!("\n🎯 Integration test completed successfully!");
    
    Ok(())
}