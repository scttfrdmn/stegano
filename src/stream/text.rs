//! Streaming plain-text trailing-whitespace embedding.
//!
//! Same scheme, line eligibility, `TXTJ` envelope and byte-preservation
//! rules as the in-memory [`TextHandler`](crate::handlers::text::TextHandler),
//! processed one line at a time.
//!
//! Embedding needs a fixed `format_specific["text_bits_per_line"]`: the
//! in-memory default picks the smallest `k` that fits, which depends on the
//! total line count and so cannot be decided in one pass.  Without it the
//! format is reported as not streamable (callers fall back to the in-memory
//! API).  With the same `k` the output is byte-identical to the in-memory
//! handler's, and streaming embed succeeds iff the in-memory embed would.
//! Extraction reads `k` from the carrier and works for any file.

use super::{not_streamable, StreamHandler, StreamStats};
use crate::core::{DataFormat, EmbedParams, Result, SteganoError};
use crate::handlers::envelope::{self, ENVELOPE_LEN};
use crate::handlers::text::{
    bits_per_line, check_strategy, eligible, write_line, TextExtractor, MAGIC, WHAT,
};
use crate::handlers::vcf::split_eol;
use std::io::{BufRead, Write};

/// Text stream handler.
pub(crate) struct TextStream;

/// The fixed bits per line, or the standard not-streamable error.
fn stream_k(params: &EmbedParams) -> Result<usize> {
    check_strategy(params)?;
    bits_per_line(params)?.ok_or_else(|| not_streamable(&DataFormat::TEXT, params))
}

impl StreamHandler for TextStream {
    fn check_params(&self, params: &EmbedParams) -> Result<()> {
        stream_k(params).map(drop)
    }

    fn embed_encoded(
        &self,
        reader: &mut dyn BufRead,
        writer: &mut dyn Write,
        encoded: &[u8],
        params: &EmbedParams,
    ) -> Result<StreamStats> {
        let k = stream_k(params)?;
        let needed = (ENVELOPE_LEN + encoded.len()) * 8;
        let stream = envelope::build(MAGIC, k as u8, encoded)?;
        let (mut written, mut n_eligible) = (0, 0);
        let mut stats = StreamStats::default();
        let (mut line, mut out) = (Vec::new(), Vec::new());
        loop {
            line.clear();
            let n = reader.read_until(b'\n', &mut line)?;
            if n == 0 {
                break;
            }
            envelope::check_text(&line, stats.bytes_read as usize, WHAT)?;
            stats.bytes_read += n as u64;
            let (body, eol) = split_eol(&line);
            let carries = if eligible(body) {
                n_eligible += 1;
                written < needed
            } else {
                false
            };
            let data: &[u8] = if carries {
                out.clear();
                write_line(&mut out, body, eol, &stream, written, k);
                written += k;
                &out
            } else {
                &line
            };
            writer.write_all(data)?;
            stats.bytes_written += data.len() as u64;
            stats.peak_buffer_bytes = stats
                .peak_buffer_bytes
                .max(line.capacity() + out.capacity());
        }
        if written < needed {
            return Err(SteganoError::InsufficientCapacity {
                needed,
                available: n_eligible * k,
            });
        }
        Ok(stats)
    }

    fn extract_encoded(&self, reader: &mut dyn BufRead, _params: &EmbedParams) -> Result<Vec<u8>> {
        let mut ex = TextExtractor::new(None);
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
            if ex.line(split_eol(&line).0)? {
                break;
            }
        }
        ex.finish()
    }
}
