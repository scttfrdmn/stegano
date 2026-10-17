//! Streaming CSV last-digit-parity embedding.
//!
//! Same scheme, eligibility and header rules, `CSVJ` envelope and
//! byte-preservation rules as the in-memory
//! [`CsvHandler`](crate::handlers::csv::CsvHandler), processed one physical
//! line at a time with the same [`CsvScanner`].  Output is byte-identical to
//! the in-memory handler's for the same encoded payload, and streaming embed
//! succeeds iff the in-memory embed would.

use super::{StreamHandler, StreamStats};
use crate::core::{EmbedParams, Result, SteganoError};
use crate::handlers::csv::{
    check_strategy, delimiter, set_parity, CsvScanner, MAGIC, STRATEGY_BYTE, WHAT,
};
use crate::handlers::envelope::{self, EnvelopeReader};
use crate::handlers::vcf::split_eol;
use std::io::{BufRead, Write};

/// CSV stream handler.
pub(crate) struct CsvStream;

impl StreamHandler for CsvStream {
    fn check_params(&self, params: &EmbedParams) -> Result<()> {
        check_strategy(params)?;
        delimiter(params).map(drop)
    }

    fn embed_encoded(
        &self,
        reader: &mut dyn BufRead,
        writer: &mut dyn Write,
        encoded: &[u8],
        params: &EmbedParams,
    ) -> Result<StreamStats> {
        let mut scanner = CsvScanner::new(delimiter(params)?);
        let stream = envelope::build(MAGIC, STRATEGY_BYTE, encoded)?;
        let needed = stream.len() * 8;
        let (mut written, mut available) = (0, 0);
        let mut stats = StreamStats::default();
        let mut line = Vec::new();
        loop {
            line.clear();
            let n = reader.read_until(b'\n', &mut line)?;
            if n == 0 {
                break;
            }
            envelope::check_text(&line, stats.bytes_read as usize, WHAT)?;
            stats.bytes_read += n as u64;
            stats.peak_buffer_bytes = stats.peak_buffer_bytes.max(line.capacity());
            let body_len = split_eol(&line).0.len();
            let cells = scanner.scan_line(&line[..body_len]);
            available += cells.len();
            for &p in cells {
                if written == needed {
                    break;
                }
                set_parity(&mut line[p], envelope::bit(&stream, written));
                written += 1;
            }
            writer.write_all(&line)?;
            stats.bytes_written += n as u64;
        }
        if written < needed {
            return Err(SteganoError::InsufficientCapacity { needed, available });
        }
        Ok(stats)
    }

    fn extract_encoded(&self, reader: &mut dyn BufRead, params: &EmbedParams) -> Result<Vec<u8>> {
        let mut scanner = CsvScanner::new(delimiter(params)?);
        let mut env = EnvelopeReader::new(MAGIC, STRATEGY_BYTE, usize::MAX, WHAT);
        let mut line = Vec::new();
        let mut base = 0;
        loop {
            line.clear();
            let n = reader.read_until(b'\n', &mut line)?;
            if n == 0 {
                break;
            }
            envelope::check_text(&line, base, WHAT)?;
            base += n;
            let body_len = split_eol(&line).0.len();
            for &p in scanner.scan_line(&line[..body_len]) {
                if env.push(line[p] & 1)? {
                    return env.finish();
                }
            }
        }
        env.finish()
    }
}
