use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "stegano")]
#[command(about = "Scientific data steganography tool")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Embed secret data into carrier file
    Embed {
        /// Input carrier file
        #[arg(short, long)]
        input: String,
        /// Secret payload file
        #[arg(short, long)]
        payload: String,
        /// Output file with embedded data
        #[arg(short, long)]
        output: String,
        /// Encryption password
        #[arg(long)]
        password: Option<String>,
        /// Enable statistical validation analysis
        #[arg(long)]
        validate: bool,
        /// Enable GPU acceleration
        #[arg(long)]
        gpu: bool,
        /// Stealth level (maximum, high, medium, low)
        #[arg(long, default_value = "high")]
        stealth: String,
        /// Error-correction level (none, low, medium, high, maximum)
        #[arg(long, default_value = "none")]
        ec_level: String,
        /// Format-specific embedding strategy.
        /// VCF: info|quality|sample|id|multi
        /// SAM/BAM: optional|comment|quality|auxiliary|multi
        /// DICOM: private|metadata|sequence|pixel_lsb|multi
        /// PNG/TIFF: lsb|adaptive|edge|dct
        /// WAV: timedomain|frequency|phase|spread|echo
        #[arg(long)]
        strategy: Option<String>,
        /// Encryption cipher (aes256gcm, chacha20). Auto-detected on extract.
        #[arg(long, default_value = "aes256gcm")]
        cipher: String,
    },
    /// Extract secret data from carrier file
    Extract {
        /// Input file with embedded data
        #[arg(short, long)]
        input: String,
        /// Output file for extracted data
        #[arg(short, long)]
        output: String,
        /// Decryption password
        #[arg(long)]
        password: Option<String>,
        /// Enable GPU acceleration
        #[arg(long)]
        gpu: bool,
        /// Format-specific extraction strategy (must match the strategy used during embed).
        /// VCF: info|quality|sample|id|multi
        /// SAM/BAM: optional|comment|quality|auxiliary|multi
        /// DICOM: private|metadata|sequence|pixel_lsb|multi
        /// WAV: timedomain|frequency|phase|spread|echo
        #[arg(long)]
        strategy: Option<String>,
    },
    /// Analyze carrier capacity
    Analyze {
        /// Input carrier file
        #[arg(short, long)]
        input: String,
        /// Run statistical validation analysis
        #[arg(long)]
        validate: bool,
    },
    /// Detect whether a carrier file contains hidden data
    Detect {
        /// Input carrier file to inspect
        #[arg(short, long)]
        input: String,
        /// Also run statistical validation analysis
        #[arg(long)]
        validate: bool,
    },
}

pub mod commands;
