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
    },
    /// Analyze carrier capacity
    Analyze {
        /// Input carrier file
        #[arg(short, long)]
        input: String,
    },
}

pub mod commands;
