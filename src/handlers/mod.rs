//! Data format handlers for different scientific data types

pub mod dicom;
pub mod fits;
pub mod generic;
pub mod genomics;
pub mod imagery;
pub mod sam_bam;
pub mod signals;
pub mod vcf;

use crate::core::*;
use async_trait::async_trait;

#[async_trait]
pub trait DataHandler: Send + Sync {
    /// Detect if this handler can process the given data
    fn detect_format(&self, data: &[u8]) -> Option<DataFormat>;

    /// Estimate embedding capacity for this data
    fn estimate_capacity(&self, carrier: &CarrierData) -> Result<CapacityInfo>;

    /// Embed payload into carrier data
    async fn embed(
        &self,
        carrier: &mut CarrierData,
        payload: &[u8],
        params: &EmbedParams,
    ) -> Result<EmbedResult>;

    /// Extract payload from carrier data
    async fn extract(&self, carrier: &CarrierData, params: &EmbedParams) -> Result<Vec<u8>>;

    /// Verify data integrity
    fn verify_integrity(&self, carrier: &CarrierData) -> Result<bool>;
}

/// Registry for all data handlers
pub struct HandlerRegistry {
    handlers: Vec<Box<dyn DataHandler>>,
}

impl Default for HandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl HandlerRegistry {
    /// Create a registry pre-populated with all built-in handlers.
    pub fn new() -> Self {
        Self {
            handlers: vec![
                Box::new(genomics::GenomicsHandler::new()),   // 0
                Box::new(vcf::VcfHandler::new()),             // 1
                Box::new(dicom::DicomHandler::new()),         // 2
                Box::new(sam_bam::SamBamHandler),             // 3
                Box::new(imagery::ImageryHandler::new()),     // 4
                Box::new(signals::SignalHandler::new()),      // 5
                Box::new(fits::FitsHandler::new()),           // 6
                Box::new(generic::GenericHandler::new()),     // 7
            ],
        }
    }

    /// Return the handler responsible for the given [`DataFormat`], or `None`
    /// if no handler covers that format.
    pub fn get_handler(&self, format: &DataFormat) -> Option<&dyn DataHandler> {
        match format {
            DataFormat::FASTA | DataFormat::FASTQ => {
                Some(&*self.handlers[0]) // GenomicsHandler
            }
            DataFormat::VCF => {
                Some(&*self.handlers[1]) // VcfHandler
            }
            DataFormat::DICOM => {
                Some(&*self.handlers[2]) // DicomHandler
            }
            DataFormat::SAM | DataFormat::BAM => {
                Some(&*self.handlers[3]) // SamBamHandler
            }
            DataFormat::PNG | DataFormat::TIFF | DataFormat::MICROSCOPY => {
                Some(&*self.handlers[4]) // ImageryHandler
            }
            DataFormat::WAV | DataFormat::BinarySignal | DataFormat::CsvTimeseries => {
                Some(&*self.handlers[5]) // SignalHandler
            }
            DataFormat::FITS => {
                Some(&*self.handlers[6]) // FitsHandler
            }
            DataFormat::BINARY | DataFormat::TEXT => {
                Some(&*self.handlers[7]) // GenericHandler
            }
        }
    }

    /// Get handler by data content detection
    pub fn get_handler_by_data(&self, data: &[u8]) -> Option<&dyn DataHandler> {
        self.handlers
            .iter()
            .find(|h| h.detect_format(data).is_some())
            .map(|h| h.as_ref())
    }

    /// Get all handlers for FFI iteration
    pub fn get_handlers(&self) -> &[Box<dyn DataHandler>] {
        &self.handlers
    }
}
