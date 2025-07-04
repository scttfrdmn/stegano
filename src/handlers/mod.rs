//! Data format handlers for different scientific data types

pub mod genomics;
pub mod vcf;
pub mod imagery;
pub mod signals;
pub mod generic;

use crate::core::*;
use async_trait::async_trait;

#[async_trait]
pub trait DataHandler: Send + Sync {
    /// Detect if this handler can process the given data
    fn detect_format(&self, data: &[u8]) -> Option<DataFormat>;
    
    /// Estimate embedding capacity for this data
    fn estimate_capacity(&self, carrier: &CarrierData) -> Result<CapacityInfo>;
    
    /// Embed payload into carrier data
    async fn embed(&self, carrier: &mut CarrierData, payload: &[u8], params: &EmbedParams) -> Result<EmbedResult>;
    
    /// Extract payload from carrier data
    async fn extract(&self, carrier: &CarrierData, params: &EmbedParams) -> Result<Vec<u8>>;
    
    /// Verify data integrity
    fn verify_integrity(&self, carrier: &CarrierData) -> Result<bool>;
}

/// Registry for all data handlers
pub struct HandlerRegistry {
    handlers: Vec<Box<dyn DataHandler>>,
}

impl HandlerRegistry {
    pub fn new() -> Self {
        Self {
            handlers: vec![
                Box::new(genomics::GenomicsHandler::new()),
                Box::new(vcf::VcfHandler::new()),
                Box::new(imagery::ImageryHandler::new()),
                Box::new(signals::SignalHandler::new()),
                Box::new(generic::GenericHandler::new()),
            ],
        }
    }
    
    pub fn get_handler(&self, format: &DataFormat) -> Option<&dyn DataHandler> {
        match format {
            DataFormat::FASTA | DataFormat::FASTQ | DataFormat::SAM | DataFormat::BAM => {
                Some(&*self.handlers[0]) // GenomicsHandler
            },
            DataFormat::VCF => {
                Some(&*self.handlers[1]) // VcfHandler
            },
            DataFormat::PNG | DataFormat::TIFF | DataFormat::MICROSCOPY | DataFormat::DICOM => {
                Some(&*self.handlers[2]) // ImageryHandler
            },
            DataFormat::WAV | DataFormat::BinarySignal | DataFormat::CsvTimeseries => {
                Some(&*self.handlers[3]) // SignalHandler
            },
            DataFormat::BINARY | DataFormat::TEXT => {
                Some(&*self.handlers[4]) // GenericHandler
            },
        }
    }
    
    /// Get handler by data content detection
    pub fn get_handler_by_data(&self, data: &[u8]) -> Option<&dyn DataHandler> {
        self.handlers.iter()
            .find(|h| h.detect_format(data).is_some())
            .map(|h| h.as_ref())
    }
    
    /// Get all handlers for FFI iteration
    pub fn get_handlers(&self) -> &[Box<dyn DataHandler>] {
        &self.handlers
    }
}
