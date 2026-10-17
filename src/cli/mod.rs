use crate::carrier::{DEFAULT_CIPHER_NAME, DEFAULT_EC_NAME, DEFAULT_STEALTH_NAME};
use clap::{Args, Parser, Subcommand};

/// Process exit codes used by the `stegano` binary.
///
/// `2` is avoided because clap uses it for usage errors.
pub mod exit_code {
    /// Success (`detect`: hidden data found).
    pub const SUCCESS: i32 = 0;
    /// Command failed (`detect`: no hidden data found).
    pub const FAILURE: i32 = 1;
    /// `detect` could not inspect the file (I/O error, unsupported format, ...).
    pub const DETECT_ERROR: i32 = 3;
}

/// Name of the environment variable read for the password.
pub const PASSWORD_ENV: &str = "STEGANO_PASSWORD";

#[derive(Parser)]
#[command(name = "stegano")]
#[command(about = "Scientific data steganography tool")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// Password sources, in order of precedence: `--password-file`,
/// `--password` (visible in `ps`/shell history; warns), the
/// `STEGANO_PASSWORD` environment variable, then `--password-prompt`.
#[derive(Args, Debug, Clone, Default)]
pub struct PasswordArgs {
    /// Encryption password. WARNING: visible to other users via `ps` and kept
    /// in shell history; prefer --password-file, STEGANO_PASSWORD or
    /// --password-prompt.
    #[arg(long, conflicts_with_all = ["password_file", "password_prompt"])]
    pub password: Option<String>,
    /// Read the password from the first line of this file.
    #[arg(long, value_name = "PATH", conflicts_with = "password_prompt")]
    pub password_file: Option<String>,
    /// Prompt for the password on the terminal (input is not echoed).
    #[arg(long)]
    pub password_prompt: bool,
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
        #[command(flatten)]
        password: PasswordArgs,
        /// Enable statistical validation analysis
        #[arg(long)]
        validate: bool,
        /// Experimental; has no effect on embedding
        #[arg(long)]
        gpu: bool,
        /// Stealth level (maximum, high, medium, low)
        #[arg(long, default_value = DEFAULT_STEALTH_NAME)]
        stealth: String,
        /// Error-correction level (none, low, medium, high, maximum).
        /// Without a password, extract must then be run with a non-"none"
        /// --ec-level too (the actual level is read from the payload).
        #[arg(long, default_value = DEFAULT_EC_NAME)]
        ec_level: String,
        /// Format-specific embedding strategy.
        /// VCF: info|quality|format|id|multi
        /// SAM/BAM: optional|comment|quality|auxiliary|multi
        /// DICOM: private|metadata|sequence|pixel_lsb|multi
        /// PNG/TIFF: lsb|adaptive|edge|dct
        /// WAV: timedomain|frequency|phase|spread|echo
        #[arg(long)]
        strategy: Option<String>,
        /// Encryption cipher (aes256gcm, chacha20). Auto-detected on extract.
        #[arg(long, default_value = DEFAULT_CIPHER_NAME)]
        cipher: String,
        /// Force the in-memory code path. By default streamable carriers
        /// (FASTQ, VCF info/id/quality, SAM/BAM auxiliary/quality) are streamed.
        #[arg(long)]
        no_stream: bool,
    },
    /// Extract secret data from carrier file
    Extract {
        /// Input file with embedded data
        #[arg(short, long)]
        input: String,
        /// Output file for extracted data
        #[arg(short, long)]
        output: String,
        #[command(flatten)]
        password: PasswordArgs,
        /// Experimental; has no effect on embedding
        #[arg(long)]
        gpu: bool,
        /// Error-correction level. Only matters for password-less payloads:
        /// use any non-"none" value if --ec-level was set at embed time (the
        /// actual level is read from the payload).
        #[arg(long, default_value = DEFAULT_EC_NAME)]
        ec_level: String,
        /// Format-specific extraction strategy (must match the strategy used during embed).
        /// VCF: info|quality|format|id|multi
        /// SAM/BAM: optional|comment|quality|auxiliary|multi
        /// DICOM: private|metadata|sequence|pixel_lsb|multi
        /// WAV: timedomain|frequency|phase|spread|echo (auto-detected)
        #[arg(long)]
        strategy: Option<String>,
        /// Force the in-memory code path. By default streamable carriers
        /// (FASTQ, VCF info/id/quality, SAM/BAM auxiliary/quality) are streamed.
        #[arg(long)]
        no_stream: bool,
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
    /// Detect whether a carrier file contains hidden data.
    /// Exit status: 0 = hidden data found, 1 = not found, 3 = error.
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
pub mod files;
