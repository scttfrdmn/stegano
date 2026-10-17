// SPDX-License-Identifier: Apache-2.0
//! Path-based embed/extract shared by the CLI, the C FFI
//! (`stegano_embed_file` / `stegano_extract_file`, used by Go) and the
//! Python `embed_file` / `extract_file` functions.
//!
//! Each call picks one of two code paths:
//!
//! * **Streamed** ([`crate::stream`]) when the format and strategy are
//!   streamable ([`crate::stream::is_streamable`]): constant memory, no size
//!   limit on the carrier.
//! * **In memory** otherwise: the whole carrier is read with
//!   [`std::fs::read`] and handed to the regular handler.
//!
//! Both paths produce interoperable output, and embedding always writes
//! `out_path` atomically (temporary file in the same directory, renamed on
//! success), so a failed embed never leaves a partial output behind.
//!
//! The decision is format-agnostic: any format registered with the stream
//! module is streamed automatically.

use crate::carrier;
use crate::core::{CarrierData, DataFormat, EmbedParams, EmbedResult, Result, SteganoError};
use crate::embedding::SteganoEmbedder;
use crate::stream;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Number of bytes read from the start of a file for magic-byte detection.
pub const HEAD_BYTES: usize = 4096;

/// Which code path a path-based call used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoMode {
    /// The carrier was streamed ([`crate::stream`]).
    Streamed,
    /// The whole carrier was read into memory.
    InMemory,
}

impl IoMode {
    /// `true` for [`IoMode::Streamed`].
    pub fn is_streamed(self) -> bool {
        self == IoMode::Streamed
    }
}

/// Read at most `n` bytes from the start of `path`.
pub fn read_head(path: &Path, n: usize) -> Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(n);
    File::open(path)?.take(n as u64).read_to_end(&mut buf)?;
    Ok(buf)
}

/// Cheap format detection that never reads the whole file: the extension,
/// then the magic bytes of the first [`HEAD_BYTES`] bytes.  Returns `None`
/// if neither is conclusive (callers then fall back to full-content
/// detection, see [`detect_file_format`]).
pub fn detect_format_cheap(path: &Path) -> Result<Option<DataFormat>> {
    if let Some(f) = carrier::detect_format_from_extension(&path.to_string_lossy()) {
        return Ok(Some(f));
    }
    Ok(carrier::detect_format_from_magic(&read_head(
        path, HEAD_BYTES,
    )?))
}

/// Detect the format of the file at `path` from its extension and its first
/// [`HEAD_BYTES`] bytes (magic bytes, then handler probing of the head).
pub fn detect_file_format(path: &Path) -> Result<DataFormat> {
    if let Some(f) = carrier::detect_format_from_extension(&path.to_string_lossy()) {
        return Ok(f);
    }
    Ok(carrier::detect_format_from_content(&read_head(
        path, HEAD_BYTES,
    )?))
}

/// Whether [`embed_path`] streams `format` with `params`.
pub fn embed_streamable(format: &DataFormat, params: &EmbedParams) -> bool {
    stream::is_streamable(format, params)
}

/// Whether [`extract_path`] streams `format`.  Stream extraction
/// auto-detects the strategy, so `*_strategy` keys are ignored here.
pub fn extract_streamable(format: &DataFormat, params: &EmbedParams) -> bool {
    let mut p = params.clone();
    p.format_specific.retain(|k, _| !k.ends_with("_strategy"));
    stream::is_streamable(format, &p)
}

/// Embed `payload` reading the carrier from `in_path` and writing the result
/// atomically to `out_path` (which may equal `in_path`).
///
/// Streams when `allow_stream` is set and the format/strategy is streamable,
/// otherwise reads the carrier into memory.  `format` is detected with
/// [`detect_file_format`] when `None`.
pub async fn embed_path(
    format: Option<DataFormat>,
    in_path: &Path,
    out_path: &Path,
    payload: &[u8],
    params: &EmbedParams,
    allow_stream: bool,
) -> Result<(EmbedResult, IoMode)> {
    let format = match format {
        Some(f) => f,
        None => detect_file_format(in_path)?,
    };
    if allow_stream && embed_streamable(&format, params) {
        let r = stream::embed_file(format, in_path, out_path, payload, params)?;
        return Ok((r, IoMode::Streamed));
    }
    let mut carrier = carrier::carrier_from_bytes(format, fs::read(in_path)?);
    let r = SteganoEmbedder::new()
        .embed(&mut carrier, payload, params)
        .await?;
    write_atomic(out_path, &carrier.raw_data)?;
    Ok((r, IoMode::InMemory))
}

/// Extract the payload from the file at `path`.
///
/// Streams (stopping as soon as the payload is complete) when `allow_stream`
/// is set and the format is streamable, otherwise reads the file into
/// memory.  `format` is detected with [`detect_file_format`] when `None`.
pub async fn extract_path(
    format: Option<DataFormat>,
    path: &Path,
    params: &EmbedParams,
    allow_stream: bool,
) -> Result<(Vec<u8>, IoMode)> {
    let format = match format {
        Some(f) => f,
        None => detect_file_format(path)?,
    };
    if allow_stream && extract_streamable(&format, params) {
        return Ok((
            stream::extract_file(format, path, params)?,
            IoMode::Streamed,
        ));
    }
    let carrier: CarrierData = carrier::carrier_from_bytes(format, fs::read(path)?);
    let out = SteganoEmbedder::new().extract(&carrier, params).await?;
    Ok((out, IoMode::InMemory))
}

/// Write `data` to `path` atomically: a temporary file in the same directory
/// is written, synced and renamed over `path`; it is removed on error.
pub fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "out".to_string());

    let mut created = None;
    for _ in 0..16 {
        let tag = hex::encode(crate::crypto::random_array::<6>());
        let tmp = dir.join(format!(".{name}.stegano-{tag}.tmp"));
        match OpenOptions::new().write(true).create_new(true).open(&tmp) {
            Ok(f) => {
                created = Some((tmp, f));
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    let (tmp, mut file) = created.ok_or_else(|| {
        SteganoError::IoError(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not create a unique temporary file",
        ))
    })?;

    let result = (|| -> std::io::Result<()> {
        file.write_all(data)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    Ok(result?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fastq(records: usize) -> Vec<u8> {
        let mut s = String::new();
        for i in 0..records {
            s.push_str(&format!("@r{i}\nACGTACGTACGTACGT\n+\nIIIIIIIIIIIIIIII\n"));
        }
        s.into_bytes()
    }

    #[test]
    fn test_detect_cheap_extension_and_magic() {
        let dir = tempfile::tempdir().unwrap();
        let named = dir.path().join("x.vcf");
        fs::write(&named, b"anything").unwrap();
        assert_eq!(detect_format_cheap(&named).unwrap(), Some(DataFormat::VCF));

        let unnamed = dir.path().join("reads.dat");
        fs::write(&unnamed, fastq(2)).unwrap();
        assert_eq!(
            detect_format_cheap(&unnamed).unwrap(),
            Some(DataFormat::FASTQ)
        );
        assert_eq!(detect_file_format(&unnamed).unwrap(), DataFormat::FASTQ);

        let opaque = dir.path().join("blob.dat");
        fs::write(&opaque, [0x11u8; 64]).unwrap();
        assert_eq!(detect_format_cheap(&opaque).unwrap(), None);
    }

    #[test]
    fn test_extract_streamable_ignores_strategy() {
        let mut p = carrier::default_params(vec![]);
        p.format_specific
            .insert("vcf_strategy".into(), serde_json::Value::from("format"));
        assert!(!embed_streamable(&DataFormat::VCF, &p));
        assert!(extract_streamable(&DataFormat::VCF, &p));
        assert!(!extract_streamable(&DataFormat::PNG, &p));
    }

    #[tokio::test]
    async fn test_paths_interoperate() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.fastq");
        fs::write(&input, fastq(200)).unwrap();
        let params = carrier::default_params(b"pw".to_vec());

        let streamed = dir.path().join("s.fastq");
        let mem = dir.path().join("m.fastq");
        let (_, m) = embed_path(None, &input, &streamed, b"hello", &params, true)
            .await
            .unwrap();
        assert_eq!(m, IoMode::Streamed);
        let (_, m) = embed_path(None, &input, &mem, b"hello", &params, false)
            .await
            .unwrap();
        assert_eq!(m, IoMode::InMemory);

        for (path, allow) in [(&streamed, false), (&mem, true)] {
            let (got, _) = extract_path(None, path, &params, allow).await.unwrap();
            assert_eq!(got, b"hello");
        }
    }

    #[test]
    fn test_write_atomic_replaces_and_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("o.bin");
        fs::write(&out, b"old").unwrap();
        write_atomic(&out, b"new").unwrap();
        assert_eq!(fs::read(&out).unwrap(), b"new");
        let leftovers = fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(leftovers, 1);
        assert!(write_atomic(&dir.path().join("missing/o.bin"), b"x").is_err());
    }
}
