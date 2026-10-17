//! Stream-embed into / extract from a large carrier file with bounded memory.
//!
//! ```text
//! cargo run --release --example stream_file -- embed BAM in.bam out.bam auxiliary
//! cargo run --release --example stream_file -- extract BAM out.bam
//! ```
//!
//! Formats: FASTQ, SAM, BAM, VCF.  The optional strategy is passed as the
//! format's `*_strategy` key.  The password is `$STEGANO_PASSWORD` (default
//! `"stream example"`); the payload is 16 KiB of test data.

use std::fs::File;
use std::io::{BufReader, Read};
use stegano::stream::{embed_file, extract_stream};
use stegano::{DataFormat, EmbedParams};

struct Counting<R> {
    inner: R,
    count: u64,
}

impl<R: Read> Read for Counting<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.count += n as u64;
        Ok(n)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = "usage: stream_file embed <FORMAT> <in> <out> [strategy] | extract <FORMAT> <in>";
    let (cmd, fmt) = match (args.first(), args.get(1)) {
        (Some(c), Some(f)) => (c.as_str(), f.as_str()),
        _ => return Err(usage.into()),
    };
    let (format, key) = match fmt {
        "FASTQ" => (DataFormat::FASTQ, "genomics_strategy"),
        "SAM" => (DataFormat::SAM, "sam_strategy"),
        "BAM" => (DataFormat::BAM, "sam_strategy"),
        "VCF" => (DataFormat::VCF, "vcf_strategy"),
        _ => return Err(usage.into()),
    };
    let password =
        std::env::var("STEGANO_PASSWORD").unwrap_or_else(|_| "stream example".to_string());
    let mut params = EmbedParams::new(password.into_bytes());
    let payload: Vec<u8> = (0..16 * 1024u32).map(|i| (i % 251) as u8).collect();
    let t = std::time::Instant::now();
    match (cmd, args.len()) {
        ("embed", 4 | 5) => {
            if let Some(s) = args.get(4) {
                params
                    .format_specific
                    .insert(key.to_string(), serde_json::Value::from(s.as_str()));
            }
            embed_file(format, &args[2], &args[3], &payload, &params)?;
            let len = std::fs::metadata(&args[2])?.len();
            println!(
                "embedded {} bytes into {len} bytes in {:?}",
                payload.len(),
                t.elapsed()
            );
        }
        ("extract", 3) => {
            let mut counting = Counting {
                inner: File::open(&args[2])?,
                count: 0,
            };
            let got = extract_stream(
                format,
                BufReader::with_capacity(256 * 1024, &mut counting),
                &params,
            )?;
            let len = std::fs::metadata(&args[2])?.len();
            assert_eq!(got, payload, "extracted payload differs");
            println!(
                "extracted {} bytes; read {} of {len} bytes in {:?}",
                got.len(),
                counting.count,
                t.elapsed()
            );
        }
        _ => return Err(usage.into()),
    }
    Ok(())
}
