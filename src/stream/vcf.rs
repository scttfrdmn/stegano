//! Streaming VCF embedding (`info`, `id` and `quality` strategies).
//!
//! Same stream layout (`'V'` + strategy byte + codec frame), chunk encoding
//! (`<decimal index>_<hex>`), header declaration and byte-preservation rules
//! as the in-memory [`VcfHandler`](crate::handlers::vcf::VcfHandler),
//! processed one line at a time.  Header lines are passed through as they
//! are read; the `##INFO=<ID=STEG,...>` declaration (info strategy, unless
//! already declared) is written just before `#CHROM`, as in memory.  The
//! three strategies fill records strictly in file order, so the output is
//! byte-identical to the in-memory handler's for the same encoded payload,
//! and streaming embed succeeds iff the in-memory embed would.
//!
//! `format` (alias `sample`) and `multi` need a global view of the carrier
//! (sample-column capacity, the minimum over channels) and are not
//! streamable; they return the standard `InvalidParameter`.
//!
//! Extraction auto-detects the channel (INFO, FORMAT, ID, QUAL, in the
//! in-memory preference order), so files written by any in-memory strategy
//! whose stream starts in the first records can be read too, and stops once
//! the codec frame is complete.

use super::{not_streamable, StreamHandler, StreamStats};
use crate::codec::{frame_len, MAX_HEADER_BLOCK};
use crate::core::{DataFormat, EmbedParams, Result, SteganoError};
use crate::handlers::vcf::{
    already_embedded_error, capacity_error, format_chunks, id_chunks, info_chunks, is_data_line,
    is_meta_line, is_plain_decimal, plausible_channel, push_id_chunk, push_info_chunk,
    push_qual_chunk, qual_chunk, record_fields, split_eol, strategy_from_params, VcfEmbedStrategy,
    ID_CHUNK_BYTES, INFO_CHUNK_BYTES, INFO_META, QUAL_CHUNK_BYTES, STEG_KEY, STREAM_MAGIC,
    VCF_MAGIC_HEADER,
};
use std::collections::BTreeMap;
use std::io::{BufRead, Write};

/// VCF stream handler.
pub(crate) struct VcfStream;

/// Length of the `[STREAM_MAGIC, strategy]` prefix.
const PREFIX_LEN: usize = 2;

fn missing_magic() -> SteganoError {
    SteganoError::IntegrityError("Invalid VCF format - missing ##fileformat=VCF header".to_string())
}

fn missing_chrom() -> SteganoError {
    SteganoError::IntegrityError("Invalid VCF format - missing #CHROM line".into())
}

/// The streamable strategy selected by `params`.
fn stream_strategy(params: &EmbedParams) -> Result<VcfEmbedStrategy> {
    match strategy_from_params(params)?.unwrap_or(VcfEmbedStrategy::InfoFieldSteganography) {
        s @ (VcfEmbedStrategy::InfoFieldSteganography
        | VcfEmbedStrategy::VariantIdSteganography
        | VcfEmbedStrategy::QualityScoreSteganography) => Ok(s),
        _ => Err(not_streamable(&DataFormat::VCF, params)),
    }
}

/// Reads one line at a time into a reusable buffer.
struct LineReader<'a> {
    reader: &'a mut dyn BufRead,
    line: Vec<u8>,
    bytes_read: u64,
    peak: usize,
}

impl<'a> LineReader<'a> {
    fn new(reader: &'a mut dyn BufRead) -> Self {
        Self {
            reader,
            line: Vec::new(),
            bytes_read: 0,
            peak: 0,
        }
    }

    /// Read the next line (terminator included); `false` at EOF.
    fn next(&mut self) -> Result<bool> {
        self.line.clear();
        let n = self.reader.read_until(b'\n', &mut self.line)?;
        self.bytes_read += n as u64;
        self.peak = self.peak.max(self.line.capacity());
        Ok(n > 0)
    }

    /// Check the magic on the first line, then pass every line up to (not
    /// including) `#CHROM` to `header`.  Returns with `#CHROM` in `line`.
    fn read_header(&mut self, mut header: impl FnMut(&[u8]) -> Result<()>) -> Result<()> {
        if !self.next()? || !self.line.starts_with(VCF_MAGIC_HEADER.as_bytes()) {
            return Err(missing_magic());
        }
        loop {
            if split_eol(&self.line).0.starts_with(b"#CHROM") {
                return Ok(());
            }
            header(&self.line)?;
            if !self.next()? {
                return Err(missing_chrom());
            }
        }
    }
}

/// Write `fields` joined by tabs, with column `col` replaced by `value`.
fn write_record(
    writer: &mut dyn Write,
    fields: &[&str],
    col: usize,
    value: &str,
    eol: &[u8],
) -> Result<u64> {
    let mut n = 0u64;
    for (j, f) in fields.iter().enumerate() {
        if j > 0 {
            writer.write_all(b"\t")?;
            n += 1;
        }
        let s = if j == col { value } else { f };
        writer.write_all(s.as_bytes())?;
        n += s.len() as u64;
    }
    writer.write_all(eol)?;
    Ok(n + eol.len() as u64)
}

impl StreamHandler for VcfStream {
    fn check_params(&self, params: &EmbedParams) -> Result<()> {
        stream_strategy(params).map(|_| ())
    }

    fn embed_encoded(
        &self,
        reader: &mut dyn BufRead,
        writer: &mut dyn Write,
        encoded: &[u8],
        params: &EmbedParams,
    ) -> Result<StreamStats> {
        use VcfEmbedStrategy::*;
        let strategy = stream_strategy(params)?;
        let (chunk_bytes, col) = match strategy {
            InfoFieldSteganography => (INFO_CHUNK_BYTES, 7),
            VariantIdSteganography => (ID_CHUNK_BYTES, 2),
            _ => (QUAL_CHUNK_BYTES, 5),
        };
        let mut stream = Vec::with_capacity(PREFIX_LEN + encoded.len());
        stream.extend_from_slice(&[STREAM_MAGIC, strategy.code()]);
        stream.extend_from_slice(encoded);
        let mut chunks = stream.chunks(chunk_bytes).enumerate().peekable();

        let mut written = 0u64;
        let mut lr = LineReader::new(reader);

        // Header: passed through; remember whether STEG is already declared.
        let mut declared = false;
        lr.read_header(|line| {
            declared |= is_meta_line(split_eol(line).0, "INFO", STEG_KEY);
            writer.write_all(line)?;
            written += line.len() as u64;
            Ok(())
        })?;
        if strategy == InfoFieldSteganography && !declared {
            let eol = split_eol(&lr.line).1;
            let eol: &[u8] = if eol.is_empty() { b"\n" } else { eol };
            writer.write_all(INFO_META.as_bytes())?;
            writer.write_all(eol)?;
            written += (INFO_META.len() + eol.len()) as u64;
        }
        writer.write_all(&lr.line)?;
        written += lr.line.len() as u64;

        // Records.  Chunk indices already present in the target channel are
        // collected to reproduce the in-memory "already embedded" check.
        let mut carriers = 0usize;
        let mut existing: Vec<usize> = Vec::new();
        let mut existing_malformed = false;
        let mut value = String::new();
        while lr.next()? {
            let (body, eol) = split_eol(&lr.line);
            let fields = if is_data_line(body) {
                record_fields(body)
            } else {
                None
            };
            let Some(f) = fields else {
                writer.write_all(&lr.line)?;
                written += lr.line.len() as u64;
                continue;
            };
            match strategy {
                InfoFieldSteganography => match info_chunks(f[7]) {
                    Ok(c) => existing.extend(c.into_iter().map(|(i, _)| i)),
                    Err(_) => existing_malformed = true,
                },
                VariantIdSteganography => {
                    existing.extend(id_chunks(f[2]).into_iter().map(|(i, _)| i))
                }
                _ => {}
            }
            let eligible = strategy != QualityScoreSteganography || is_plain_decimal(f[5]);
            match eligible.then(|| chunks.next()).flatten() {
                Some((index, chunk)) => {
                    carriers += 1;
                    value.clear();
                    value.push_str(f[col]);
                    match strategy {
                        InfoFieldSteganography => push_info_chunk(&mut value, index, chunk),
                        VariantIdSteganography => push_id_chunk(&mut value, index, chunk),
                        _ => push_qual_chunk(&mut value, chunk),
                    }
                    written += write_record(writer, &f, col, &value, eol)?;
                }
                None => {
                    carriers += usize::from(eligible);
                    writer.write_all(&lr.line)?;
                    written += lr.line.len() as u64;
                }
            }
        }

        if strategy != QualityScoreSteganography && !existing_malformed && !existing.is_empty() {
            existing.sort_unstable();
            if existing.iter().enumerate().all(|(k, &i)| k == i) {
                return Err(already_embedded_error(strategy.name()));
            }
        }
        if chunks.peek().is_some() {
            return Err(capacity_error(stream.len(), carriers * chunk_bytes));
        }
        Ok(StreamStats {
            bytes_read: lr.bytes_read,
            bytes_written: written,
            peak_buffer_bytes: lr.peak
                + value.capacity()
                + stream.len()
                + existing.capacity() * std::mem::size_of::<usize>(),
        })
    }

    fn extract_encoded(&self, reader: &mut dyn BufRead) -> Result<Vec<u8>> {
        extract(reader).map(|(raw, _)| raw)
    }
}

/// Extraction state of one channel.
struct Channel {
    strategy: VcfEmbedStrategy,
    /// Some chunk has been seen.
    active: bool,
    /// Malformed, duplicated or implausible: ignored from now on.
    dropped: bool,
    /// QUAL only: the run of chunk-carrying values has ended.
    ended: bool,
    /// Contiguous stream bytes (chunks `0..next`).
    bytes: Vec<u8>,
    next: usize,
    /// Out-of-order chunks.
    pending: BTreeMap<usize, Vec<u8>>,
    /// The `'V'` + strategy prefix was checked.
    prefix_ok: bool,
    /// Total stream length once known from the codec header.
    target: Option<usize>,
}

impl Channel {
    fn new(strategy: VcfEmbedStrategy) -> Self {
        Self {
            strategy,
            active: false,
            dropped: false,
            ended: false,
            bytes: Vec::new(),
            next: 0,
            pending: BTreeMap::new(),
            prefix_ok: false,
            target: None,
        }
    }

    fn live(&self) -> bool {
        self.active && !self.dropped
    }

    fn push(&mut self, index: usize, data: Vec<u8>) {
        self.active = true;
        if index < self.next || self.pending.contains_key(&index) {
            self.dropped = true; // duplicated chunk (an error in memory too)
            return;
        }
        self.pending.insert(index, data);
        while let Some(d) = self.pending.remove(&self.next) {
            self.bytes.extend_from_slice(&d);
            self.next += 1;
        }
    }

    fn push_all(&mut self, chunks: Result<Vec<(usize, Vec<u8>)>>) {
        match chunks {
            Ok(c) => c.into_iter().for_each(|(i, d)| self.push(i, d)),
            Err(_) => {
                self.active = true;
                self.dropped = true;
            }
        }
    }

    /// Feed one record's columns.
    fn feed(&mut self, f: &[&str]) {
        if self.dropped || self.ended {
            return;
        }
        match self.strategy {
            VcfEmbedStrategy::InfoFieldSteganography => self.push_all(info_chunks(f[7])),
            VcfEmbedStrategy::SampleFormatSteganography => self.push_all(format_chunks(f)),
            VcfEmbedStrategy::VariantIdSteganography => self.push_all(Ok(id_chunks(f[2]))),
            _ if is_plain_decimal(f[5]) => match qual_chunk(f[5]) {
                Some(b) => {
                    self.active = true;
                    self.bytes.extend_from_slice(&b);
                }
                None => self.ended = true,
            },
            _ => {}
        }
    }

    /// Validate the prefix and learn the frame length as bytes arrive.
    fn update(&mut self) {
        if !self.live() {
            return;
        }
        if !self.prefix_ok && self.bytes.len() >= PREFIX_LEN {
            let found = (self.bytes[0] == STREAM_MAGIC)
                .then(|| VcfEmbedStrategy::from_code(self.bytes[1]))
                .flatten();
            if !found.is_some_and(|s| plausible_channel(s, self.strategy)) {
                self.dropped = true;
                return;
            }
            self.prefix_ok = true;
        }
        if self.prefix_ok && self.target.is_none() {
            let codec = &self.bytes[PREFIX_LEN..];
            // frame_len needs MAX_HEADER_BLOCK bytes or the whole (shorter)
            // frame; the header records its own EC level, so a partial prefix
            // either yields the right length or `None`.
            match frame_len(&codec[..codec.len().min(MAX_HEADER_BLOCK)]) {
                Some(n) => self.target = Some(PREFIX_LEN.saturating_add(n)),
                // Not a valid codec frame: hand what we have to the decoder,
                // which reports the precise error.
                None if codec.len() >= MAX_HEADER_BLOCK => {
                    self.target = Some(PREFIX_LEN + MAX_HEADER_BLOCK)
                }
                None => {}
            }
        }
    }

    fn complete(&self) -> bool {
        self.live() && self.prefix_ok && self.target.is_some_and(|t| self.bytes.len() >= t)
    }

    fn take(&self) -> Vec<u8> {
        let end = self.target.unwrap_or(usize::MAX).min(self.bytes.len());
        self.bytes[PREFIX_LEN..end].to_vec()
    }
}

/// Extract the codec bytes; also returns the carrier bytes consumed.
///
/// All channels are read in parallel.  A channel is returned as soon as its
/// frame is complete and no higher-priority channel (INFO, FORMAT, ID, QUAL)
/// is still being filled; a channel in which no chunk has been seen yet does
/// not hold up a lower one.  (The in-memory handler, with the whole file in
/// view, prefers a higher channel even if its stream starts late; every
/// embedder starts a stream at the first eligible record, so the two agree
/// on all carriers they produce.)
pub(crate) fn extract(reader: &mut dyn BufRead) -> Result<(Vec<u8>, u64)> {
    use VcfEmbedStrategy::*;
    let mut chans: Vec<Channel> = [
        InfoFieldSteganography,
        SampleFormatSteganography,
        VariantIdSteganography,
        QualityScoreSteganography,
    ]
    .into_iter()
    .map(Channel::new)
    .collect();

    let mut lr = LineReader::new(reader);
    lr.read_header(|_| Ok(()))?;
    while lr.next()? {
        let body = split_eol(&lr.line).0;
        if !is_data_line(body) {
            continue;
        }
        let Some(f) = record_fields(body) else {
            continue;
        };
        for c in chans.iter_mut() {
            c.feed(&f);
            c.update();
        }
        for c in &chans {
            if c.complete() {
                return Ok((c.take(), lr.bytes_read));
            }
            if c.live() {
                break; // a higher-priority channel is still being filled
            }
        }
    }

    // EOF: the first channel whose stream is complete (gap-free and, if the
    // frame length is known, long enough) wins, as in memory.
    let usable = |c: &&Channel| {
        c.live()
            && c.prefix_ok
            && c.pending.is_empty()
            && c.target.is_none_or(|t| c.bytes.len() >= t)
    };
    if let Some(c) = chans.iter().find(usable) {
        return Ok((c.take(), lr.bytes_read));
    }
    if chans.iter().any(|c| c.live() && c.prefix_ok) {
        return Err(SteganoError::IntegrityError(
            "Carrier ended before the payload (truncated or corrupted carrier)".to_string(),
        ));
    }
    Err(SteganoError::IntegrityError(
        "No stegano data found in VCF".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{decode_payload, encode_payload};
    use crate::core::{CarrierData, ECLevel};
    use crate::handlers::vcf::VcfHandler;
    use crate::handlers::DataHandler;
    use crate::stream::{
        embed_encoded_stream, embed_file, embed_stream, extract_file, extract_stream, is_streamable,
    };
    use std::io::{BufReader, Cursor, Read};

    const STREAMED: [&str; 3] = ["info", "id", "quality"];

    fn params(password: &str, strategy: Option<&str>) -> EmbedParams {
        let mut p = EmbedParams {
            encryption_key: password.as_bytes().to_vec(),
            error_correction_level: ECLevel::None,
            ..Default::default()
        };
        if let Some(s) = strategy {
            p.format_specific
                .insert("vcf_strategy".to_string(), serde_json::Value::from(s));
        }
        p
    }

    /// One record; QUAL cycles through float, integer, missing and
    /// exponent notation, ID through missing and rsIDs.
    fn record(i: usize, samples: bool, eol: &str) -> String {
        let qual = match i % 5 {
            0 => "29.47".to_string(),
            1 => format!("{}", 30 + i % 50),
            2 => ".".to_string(),
            3 => "1e3".to_string(),
            _ => format!("{}.{}", i % 90, i % 7),
        };
        let id = if i.is_multiple_of(3) {
            ".".to_string()
        } else {
            format!("rs{i}")
        };
        let info = if i.is_multiple_of(7) {
            ".".to_string()
        } else {
            format!("DP={};AF=0.5", i % 60)
        };
        let mut s = format!("chr1\t{}\t{id}\tA\tT\t{qual}\tPASS\t{info}", 100 + i);
        if samples {
            s.push_str("\tGT:DP\t0/1:12\t1|1");
        }
        s.push_str(eol);
        s
    }

    fn header(samples: bool, eol: &str) -> String {
        let mut s = String::new();
        for l in [
            "##fileformat=VCFv4.2",
            "##contig=<ID=chr1,length=1000000000>",
            "##INFO=<ID=DP,Number=1,Type=Integer,Description=\"Depth\">",
            "##INFO=<ID=AF,Number=A,Type=Float,Description=\"Allele frequency\">",
            "##FORMAT=<ID=GT,Number=1,Type=String,Description=\"Genotype\">",
            "##FORMAT=<ID=DP,Number=1,Type=Integer,Description=\"Depth\">",
        ] {
            s.push_str(l);
            s.push_str(eol);
        }
        s.push_str("#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO");
        if samples {
            s.push_str("\tFORMAT\tS1\tS2");
        }
        s.push_str(eol);
        s
    }

    fn make_vcf(n: usize, samples: bool, eol: &str) -> Vec<u8> {
        let mut s = header(samples, eol);
        for i in 0..n {
            s.push_str(&record(i, samples, eol));
            if i == 3 {
                // Lines that are passed through untouched.
                s.push_str(&format!("#a comment line{eol}"));
                s.push_str(eol);
                s.push_str(&format!("garbage line without tabs{eol}"));
            }
        }
        s.into_bytes()
    }

    /// Carriers covering LF/CRLF, samples, an existing STEG declaration and
    /// a missing final newline.
    fn carriers(n: usize) -> Vec<(&'static str, Vec<u8>)> {
        let declared = {
            let mut s = b"##fileformat=VCFv4.2\r\n".to_vec();
            s.extend_from_slice(INFO_META.as_bytes());
            s.extend_from_slice(b"\r\n");
            s.extend_from_slice(&make_vcf(n, false, "\r\n")[22..]);
            s
        };
        let mut no_final_nl = make_vcf(n, true, "\n");
        no_final_nl.pop();
        vec![
            ("lf", make_vcf(n, false, "\n")),
            ("crlf", make_vcf(n, true, "\r\n")),
            ("declared", declared),
            ("no_final_nl", no_final_nl),
        ]
    }

    fn payload(n: usize) -> Vec<u8> {
        (0..n as u32).map(|i| (i * 31 % 251) as u8).collect()
    }

    fn stream_embed_encoded(data: &[u8], encoded: &[u8], p: &EmbedParams) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        embed_encoded_stream(
            &DataFormat::VCF,
            &mut Cursor::new(data),
            &mut out,
            encoded,
            p,
        )?;
        Ok(out)
    }

    #[test]
    fn test_interop_byte_identical_with_in_memory() {
        let handler = VcfHandler::new();
        let carriers = carriers(1000);
        for pw in ["", "pw"] {
            // 3500 bytes: > 100 chunks in every channel.
            for n in [0usize, 1, 37, 3500] {
                let encoded = encode_payload(&payload(n), &params(pw, None)).unwrap();
                for s in STREAMED {
                    let p = params(pw, Some(s));
                    let strategy = VcfEmbedStrategy::from_name(s).unwrap();
                    for (name, orig) in &carriers {
                        let mem = handler.embed_encoded(orig, &encoded, strategy).unwrap();
                        let streamed = stream_embed_encoded(orig, &encoded, &p).unwrap();
                        assert!(
                            streamed == mem,
                            "{name}/{s}/pw={}/n={n}: outputs differ",
                            !pw.is_empty()
                        );
                        assert_ne!(&streamed, orig);
                        if n == 3500 {
                            let text = String::from_utf8_lossy(&streamed);
                            match s {
                                "info" => assert!(text.contains("STEG=110_")),
                                "id" => assert!(text.contains("STEG220_")),
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn test_stream_embed_then_in_memory_extract() {
        let handler = VcfHandler::new();
        for (name, orig) in carriers(600) {
            for s in STREAMED {
                for pw in ["", "hunter2"] {
                    let p = params(pw, Some(s));
                    for data in [format!("secret for {name}/{s}").into_bytes(), payload(2000)] {
                        let mut out = Vec::new();
                        let res =
                            embed_stream(DataFormat::VCF, Cursor::new(&orig), &mut out, &data, &p)
                                .unwrap();
                        assert_eq!(res.bytes_embedded, data.len());
                        let c = CarrierData::new(DataFormat::VCF, out.clone());
                        assert_eq!(handler.extract(&c, &p).await.unwrap(), data, "{name}/{s}");
                        let auto = params(pw, None);
                        assert_eq!(handler.extract(&c, &auto).await.unwrap(), data);
                        let got = extract_stream(DataFormat::VCF, Cursor::new(&out), &auto)
                            .unwrap_or_else(|e| panic!("{name}/{s}: {e}"));
                        assert_eq!(got, data, "{name}/{s}");
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn test_in_memory_embed_then_stream_extract() {
        let handler = VcfHandler::new();
        for (name, orig) in carriers(600) {
            // format/multi are extract-only for streaming (need samples).
            let mut strategies = STREAMED.to_vec();
            if name == "crlf" || name == "no_final_nl" {
                strategies.extend(["format", "sample", "multi"]);
            }
            for s in strategies {
                for pw in ["", "hunter2"] {
                    let p = params(pw, Some(s));
                    for data in [format!("{name}/{s}").repeat(3).into_bytes(), payload(1500)] {
                        let mut c = CarrierData::new(DataFormat::VCF, orig.clone());
                        handler.embed(&mut c, &data, &p).await.unwrap();
                        // Extraction ignores the strategy key (auto-detected).
                        let got = extract_stream(
                            DataFormat::VCF,
                            Cursor::new(&c.raw_data),
                            &params(pw, None),
                        )
                        .unwrap_or_else(|e| panic!("{name}/{s}: {e}"));
                        assert_eq!(got, data, "{name}/{s}");
                    }
                }
            }
        }
    }

    #[test]
    fn test_ec_levels_and_wrong_password() {
        let orig = make_vcf(800, false, "\n");
        for s in STREAMED {
            for level in [ECLevel::Low, ECLevel::Maximum] {
                let mut p = params("pw", Some(s));
                p.error_correction_level = level;
                let mut out = Vec::new();
                embed_stream(DataFormat::VCF, Cursor::new(&orig), &mut out, b"ec", &p).unwrap();
                let got = extract_stream(DataFormat::VCF, Cursor::new(&out), &params("pw", None));
                assert_eq!(got.unwrap(), b"ec", "{s}/{level:?}");
                let bad = extract_stream(DataFormat::VCF, Cursor::new(&out), &params("no", None));
                assert!(bad.is_err());
            }
        }
    }

    /// Reader adapter counting the bytes handed out.
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

    #[test]
    fn test_extract_stops_early() {
        let orig = make_vcf(20_000, false, "\n");
        for s in STREAMED {
            let p = params("", Some(s));
            let mut out = Vec::new();
            embed_stream(DataFormat::VCF, Cursor::new(&orig), &mut out, b"early", &p).unwrap();
            let (raw, consumed) = extract(&mut Cursor::new(&out)).unwrap();
            assert_eq!(decode_payload(&raw, &p).unwrap(), b"early");
            assert!(consumed < 4000, "{s}: consumed {consumed} of {}", out.len());
            let mut counting = Counting {
                inner: Cursor::new(&out),
                count: 0,
            };
            let got = extract_stream(
                DataFormat::VCF,
                BufReader::with_capacity(512, &mut counting),
                &p,
            )
            .unwrap();
            assert_eq!(got, b"early");
            assert!(counting.count < 8192, "{s}: read {} bytes", counting.count);
        }
    }

    #[tokio::test]
    async fn test_errors_match_in_memory() {
        let handler = VcfHandler::new();
        let small = make_vcf(4, false, "\n");
        let mut cases: Vec<(Vec<u8>, &str, Vec<u8>)> = vec![
            (b"".to_vec(), "info", b"x".to_vec()),
            (b"not a vcf\n".to_vec(), "id", b"x".to_vec()),
            (
                b"##fileformat=VCFv4.2\n##x=y\n".to_vec(),
                "info",
                b"x".to_vec(),
            ),
            (header(false, "\n").into_bytes(), "info", b"x".to_vec()),
        ];
        for s in STREAMED {
            cases.push((small.clone(), s, vec![0u8; 500]));
        }
        for s in ["info", "id"] {
            // Already embedded in the same channel.
            let mut c = CarrierData::new(DataFormat::VCF, make_vcf(50, false, "\n"));
            handler
                .embed(&mut c, b"one", &params("", Some(s)))
                .await
                .unwrap();
            cases.push((c.raw_data, s, b"two".to_vec()));
        }
        for (data, s, pl) in cases {
            let p = params("", Some(s));
            let mut c = CarrierData::new(DataFormat::VCF, data.clone());
            let mem = handler.embed(&mut c, &pl, &p).await.unwrap_err();
            let st =
                embed_stream(DataFormat::VCF, Cursor::new(&data), Vec::new(), &pl, &p).unwrap_err();
            assert_eq!(st.to_string(), mem.to_string(), "{s}");
            if let (
                SteganoError::InsufficientCapacity { needed, available },
                SteganoError::InsufficientCapacity {
                    needed: n2,
                    available: a2,
                },
            ) = (&st, &mem)
            {
                assert_eq!((needed, available), (n2, a2));
            }
        }
    }

    #[test]
    fn test_no_payload_and_truncated() {
        let orig = make_vcf(500, false, "\n");
        let p = params("", None);
        assert!(matches!(
            extract_stream(DataFormat::VCF, Cursor::new(&orig), &p),
            Err(SteganoError::IntegrityError(_))
        ));
        let mut out = Vec::new();
        embed_stream(
            DataFormat::VCF,
            Cursor::new(&orig),
            &mut out,
            &[9u8; 2000],
            &p,
        )
        .unwrap();
        let cut = out.len() / 20;
        let cut = cut + out[cut..].iter().position(|&b| b == b'\n').unwrap() + 1;
        let err = extract_stream(DataFormat::VCF, Cursor::new(&out[..cut]), &p).unwrap_err();
        assert!(err.to_string().contains("ended before"), "{err}");
    }

    #[test]
    fn test_streamable_strategies() {
        assert!(is_streamable(&DataFormat::VCF, &params("", None)));
        for s in [
            "info",
            "info_fields",
            "id",
            "quality",
            "quality_precision",
            "ID",
        ] {
            assert!(is_streamable(&DataFormat::VCF, &params("", Some(s))), "{s}");
        }
        for s in ["format", "sample", "format_fields", "multi"] {
            let p = params("", Some(s));
            assert!(!is_streamable(&DataFormat::VCF, &p), "{s}");
            let err =
                embed_stream(DataFormat::VCF, Cursor::new(b""), Vec::new(), b"x", &p).unwrap_err();
            match err {
                SteganoError::InvalidParameter(m) => assert_eq!(
                    m,
                    format!("VCF/{s} is not streamable; use the in-memory API")
                ),
                other => panic!("{other:?}"),
            }
        }
        assert!(!is_streamable(&DataFormat::VCF, &params("", Some("bogus"))));
    }

    #[test]
    fn test_embed_file_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.vcf");
        let output = dir.path().join("out.vcf");
        std::fs::write(&input, make_vcf(300, true, "\n")).unwrap();
        let p = params("pw", Some("id"));
        embed_file(DataFormat::VCF, &input, &output, b"file payload", &p).unwrap();
        assert_eq!(
            extract_file(DataFormat::VCF, &output, &p).unwrap(),
            b"file payload"
        );
    }

    /// Lazily generated VCF (never held in memory as a whole).
    struct GenReader {
        record: usize,
        records: usize,
        buf: Vec<u8>,
        pos: usize,
    }

    impl GenReader {
        fn new(records: usize) -> Self {
            Self {
                record: 0,
                records,
                buf: header(true, "\n").into_bytes(),
                pos: 0,
            }
        }
    }

    impl Read for GenReader {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            if self.pos == self.buf.len() {
                if self.record == self.records {
                    return Ok(0);
                }
                self.buf = record(self.record, true, "\n").into_bytes();
                self.record += 1;
                self.pos = 0;
            }
            let n = out.len().min(self.buf.len() - self.pos);
            out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
            self.pos += n;
            Ok(n)
        }
    }

    /// Writer that keeps only a prefix and counts the rest.
    struct PrefixSink {
        prefix: Vec<u8>,
        keep: usize,
        total: u64,
    }

    impl Write for PrefixSink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let room = self.keep.saturating_sub(self.prefix.len()).min(buf.len());
            self.prefix.extend_from_slice(&buf[..room]);
            self.total += buf.len() as u64;
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_bounded_memory_30mb() {
        // ~65 bytes per record -> ~30 MB.
        let records = 450_000;
        for s in STREAMED {
            let p = params("", Some(s));
            let encoded = encode_payload(&[0xC3u8; 2048], &p).unwrap();
            let mut reader = BufReader::with_capacity(64 * 1024, GenReader::new(records));
            let mut sink = PrefixSink {
                prefix: Vec::new(),
                keep: 1 << 20,
                total: 0,
            };
            let stats =
                embed_encoded_stream(&DataFormat::VCF, &mut reader, &mut sink, &encoded, &p)
                    .unwrap();
            assert!(stats.bytes_read > 25_000_000, "{}", stats.bytes_read);
            assert_eq!(sink.total, stats.bytes_written);
            // One line buffer + the 2 KiB stream; independent of size.
            assert!(
                stats.peak_buffer_bytes < 16 * 1024,
                "{s}: peak {}",
                stats.peak_buffer_bytes
            );
            let (raw, consumed) = extract(&mut Cursor::new(&sink.prefix)).unwrap();
            assert_eq!(decode_payload(&raw, &p).unwrap(), [0xC3u8; 2048]);
            assert!(consumed < 100_000, "{s}: {consumed}");
        }
    }

    /// Manual large-file check (not run in CI):
    ///
    /// ```text
    /// STEGANO_BIG_VCF=/path/big.vcf cargo test --release --lib \
    ///     stream::vcf::tests::big_vcf_manual -- --ignored --nocapture
    /// ```
    ///
    /// `STEGANO_BIG_VCF_GEN=<records>` first writes a synthetic file there.
    #[test]
    #[ignore]
    fn big_vcf_manual() {
        let Ok(path) = std::env::var("STEGANO_BIG_VCF") else {
            eprintln!("STEGANO_BIG_VCF not set; skipping");
            return;
        };
        if let Ok(n) = std::env::var("STEGANO_BIG_VCF_GEN") {
            let n: usize = n.parse().unwrap();
            let mut w = std::io::BufWriter::new(std::fs::File::create(&path).unwrap());
            std::io::copy(&mut GenReader::new(n), &mut w).unwrap();
            w.flush().unwrap();
            eprintln!("generated {path}");
            return;
        }
        let out = format!("{path}.stego");
        let strategy = std::env::var("STEGANO_BIG_VCF_STRATEGY").unwrap_or("info".into());
        let p = params("big password", Some(&strategy));
        let payload = vec![0x42u8; 16 * 1024];
        let t = std::time::Instant::now();
        let res = embed_file(DataFormat::VCF, &path, &out, &payload, &p).unwrap();
        let in_len = std::fs::metadata(&path).unwrap().len();
        eprintln!(
            "embed ({strategy}): {in_len} bytes in {:?} (reported memory {} bytes)",
            t.elapsed(),
            res.performance_stats.memory_usage_bytes
        );
        let t = std::time::Instant::now();
        let mut counting = Counting {
            inner: std::fs::File::open(&out).unwrap(),
            count: 0,
        };
        let got = extract_stream(
            DataFormat::VCF,
            BufReader::with_capacity(256 * 1024, &mut counting),
            &p,
        )
        .unwrap();
        assert_eq!(got, payload);
        eprintln!(
            "extract: read {} of {in_len} bytes in {:?}",
            counting.count,
            t.elapsed()
        );
        if std::env::var("STEGANO_BIG_VCF_KEEP").is_err() {
            std::fs::remove_file(&out).unwrap();
        }
    }
}
