//! DICOM (Digital Imaging and Communications in Medicine) handler.
//!
//! # Parsing
//!
//! [`DicomFile`] is a lossless structural parser for DICOM PS3.10 files:
//!
//! * Files with the 128-byte preamble + `DICM` prefix, and files without it
//!   (a bare File Meta group, or a bare data set).
//! * The File Meta Information group (0002) is always explicit VR little
//!   endian (PS3.10 §7.1).  The data set is decoded according to the
//!   Transfer Syntax UID (0002,0010):
//!   * `1.2.840.10008.1.2` — implicit VR little endian,
//!   * `1.2.840.10008.1.2.2` — explicit VR big endian (retired),
//!   * every other (non-deflated) syntax — explicit VR little endian
//!     (this includes the compressed syntaxes, whose pixel data is
//!     encapsulated).
//!
//!   Deflated syntaxes are rejected.
//! * Explicit VR header layout per PS3.5 §7.1.2: `OB OD OF OL OV OW SQ SV UC
//!   UN UR UT UV` use VR + 2 reserved bytes + 32-bit length; every other VR
//!   uses VR + 16-bit length.
//! * Undefined-length (`FFFFFFFF`) sequences and items, their delimiters
//!   (FFFE,E000 / E00D / E0DD) and encapsulated pixel data fragments are kept
//!   structurally, so an unmodified parse → serialize is byte-identical.
//! * Any malformed input (truncation, invalid VR, unordered tags, missing
//!   delimiters, …) is an error — elements are never silently dropped.
//!
//! # Strategies (`format_specific["dicom_strategy"]`)
//!
//! | name        | location                                          | capacity (frame bytes)          |
//! |-------------|---------------------------------------------------|---------------------------------|
//! | `private`   | private block `(gggg,00xx)` LO `STEGANO` + `(gggg,xx10)` OB | 16 MiB (policy limit) |
//! | `metadata`  | 128-byte preamble                                 | 128                              |
//! | `sequence`  | `(FFFC,FFFC)` Data Set Trailing Padding, OB       | 16 MiB (policy limit)            |
//! | `pixel_lsb` | LSB of each native pixel sample                   | samples / 8                      |
//! | `multi`     | `private` + `pixel_lsb` (redundant copies)        | min of the two                   |
//!
//! Every location stores a self-describing frame
//! `"STGD" | strategy:u8 | len:u32 LE | codec bytes`, where the codec bytes
//! come from [`crate::codec::encode_payload`].  Extraction locates the frame
//! by its marker and calls [`crate::codec::decode_payload`] exactly once.

use super::DataHandler;
use crate::core::*;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use crate::time::Instant;

/// A DICOM attribute tag `(group, element)`.
pub type Tag = (u16, u16);

const PREAMBLE_LEN: usize = 128;
const PREFIX: &[u8; 4] = b"DICM";
const UNDEFINED_LENGTH: u32 = 0xFFFF_FFFF;
/// Maximum sequence nesting accepted by the parser.
const MAX_DEPTH: usize = 32;

const ITEM: Tag = (0xFFFE, 0xE000);
const ITEM_DELIMITATION: Tag = (0xFFFE, 0xE00D);
const SEQUENCE_DELIMITATION: Tag = (0xFFFE, 0xE0DD);

mod tags {
    use super::Tag;
    pub const TRANSFER_SYNTAX_UID: Tag = (0x0002, 0x0010);
    pub const SAMPLES_PER_PIXEL: Tag = (0x0028, 0x0002);
    pub const NUMBER_OF_FRAMES: Tag = (0x0028, 0x0008);
    pub const ROWS: Tag = (0x0028, 0x0010);
    pub const COLUMNS: Tag = (0x0028, 0x0011);
    pub const BITS_ALLOCATED: Tag = (0x0028, 0x0100);
    pub const BITS_STORED: Tag = (0x0028, 0x0101);
    pub const HIGH_BIT: Tag = (0x0028, 0x0102);
    pub const PIXEL_REPRESENTATION: Tag = (0x0028, 0x0103);
    pub const PIXEL_DATA: Tag = (0x7FE0, 0x0010);
    pub const TRAILING_PADDING: Tag = (0xFFFC, 0xFFFC);
}

/// Implicit VR Little Endian transfer syntax UID.
pub const UID_IMPLICIT_VR_LE: &str = "1.2.840.10008.1.2";
/// Explicit VR Little Endian transfer syntax UID.
pub const UID_EXPLICIT_VR_LE: &str = "1.2.840.10008.1.2.1";
/// Explicit VR Big Endian transfer syntax UID (retired).
pub const UID_EXPLICIT_VR_BE: &str = "1.2.840.10008.1.2.2";
const UID_DEFLATED_LE: &str = "1.2.840.10008.1.2.1.99";
const UID_JPIP_REFERENCED_DEFLATE: &str = "1.2.840.10008.1.2.4.95";

const KNOWN_VRS: [&[u8; 2]; 34] = [
    b"AE", b"AS", b"AT", b"CS", b"DA", b"DS", b"DT", b"FD", b"FL", b"IS", b"LO", b"LT", b"OB",
    b"OD", b"OF", b"OL", b"OV", b"OW", b"PN", b"SH", b"SL", b"SQ", b"SS", b"ST", b"SV", b"TM",
    b"UC", b"UI", b"UL", b"UN", b"UR", b"US", b"UT", b"UV",
];

/// VRs whose explicit-VR header is VR + 2 reserved bytes + u32 length
/// (PS3.5 §7.1.2, Table 7.1-1).
const LONG_VRS: [&[u8; 2]; 13] = [
    b"OB", b"OD", b"OF", b"OL", b"OV", b"OW", b"SQ", b"SV", b"UC", b"UN", b"UR", b"UT", b"UV",
];

fn is_known_vr(vr: &[u8; 2]) -> bool {
    KNOWN_VRS.contains(&vr)
}

fn is_long_vr(vr: &[u8; 2]) -> bool {
    LONG_VRS.contains(&vr)
}

fn fmt_tag(tag: Tag) -> String {
    format!("({:04X},{:04X})", tag.0, tag.1)
}

fn parse_err(offset: usize, msg: impl AsRef<str>) -> SteganoError {
    SteganoError::IntegrityError(format!(
        "DICOM parse error at byte {offset}: {}",
        msg.as_ref()
    ))
}

// ---------------------------------------------------------------------------
// Transfer syntax
// ---------------------------------------------------------------------------

/// Byte-level encoding of a DICOM data set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferSyntax {
    /// 1.2.840.10008.1.2
    ImplicitVrLittleEndian,
    /// 1.2.840.10008.1.2.1 and all non-deflated compressed syntaxes
    ExplicitVrLittleEndian,
    /// 1.2.840.10008.1.2.2
    ExplicitVrBigEndian,
}

impl TransferSyntax {
    /// Map a Transfer Syntax UID to its data set encoding.
    pub fn from_uid(uid: &str) -> Result<Self> {
        if uid.is_empty() || !uid.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
            return Err(SteganoError::IntegrityError(format!(
                "Invalid Transfer Syntax UID {uid:?}"
            )));
        }
        match uid {
            UID_IMPLICIT_VR_LE => Ok(Self::ImplicitVrLittleEndian),
            UID_EXPLICIT_VR_BE => Ok(Self::ExplicitVrBigEndian),
            UID_DEFLATED_LE | UID_JPIP_REFERENCED_DEFLATE => Err(SteganoError::InvalidParameter(
                format!("Deflated DICOM transfer syntax {uid} is not supported"),
            )),
            _ => Ok(Self::ExplicitVrLittleEndian),
        }
    }

    /// Whether the data set carries explicit VRs.
    pub fn explicit_vr(self) -> bool {
        !matches!(self, Self::ImplicitVrLittleEndian)
    }

    /// Whether numbers (tags, lengths, binary values) are big endian.
    pub fn big_endian(self) -> bool {
        matches!(self, Self::ExplicitVrBigEndian)
    }
}

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

/// A data element, kept losslessly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DicomElement {
    /// Attribute tag.
    pub tag: Tag,
    /// Explicit VR as stored in the file; `None` in implicit VR data sets.
    pub vr: Option<[u8; 2]>,
    /// The two reserved bytes of a long-form explicit VR header (normally 0).
    pub reserved: [u8; 2],
    /// Element value.
    pub value: ElementValue,
}

/// Value of a [`DicomElement`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElementValue {
    /// Defined-length raw value bytes (exactly as stored; no padding added).
    Bytes(Vec<u8>),
    /// A sequence of items.
    Sequence(Sequence),
    /// Encapsulated (compressed) pixel data fragments.
    Encapsulated(Encapsulated),
}

/// A sequence (SQ, or UN with undefined length).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sequence {
    /// Encoded with undefined length and terminated by (FFFE,E0DD).
    pub undefined_length: bool,
    /// Encoding of the items (implicit VR LE for UN-undefined-length).
    pub item_syntax: TransferSyntax,
    /// Items in order.
    pub items: Vec<Item>,
    /// Length field of the sequence delimitation item (normally 0).
    pub delimiter_length: u32,
}

/// An item (FFFE,E000) of a sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Encoded with undefined length and terminated by (FFFE,E00D).
    pub undefined_length: bool,
    /// Nested data set.
    pub elements: Vec<DicomElement>,
    /// Length field of the item delimitation item (normally 0).
    pub delimiter_length: u32,
}

/// Encapsulated pixel data: basic offset table + fragments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Encapsulated {
    /// Item values in order (the first is the basic offset table).
    pub fragments: Vec<Vec<u8>>,
    /// Length field of the sequence delimitation item (normally 0).
    pub delimiter_length: u32,
}

/// A parsed DICOM file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DicomFile {
    /// 128-byte preamble; when present the `DICM` prefix follows it.
    pub preamble: Option<Vec<u8>>,
    /// File Meta Information group (0002), always explicit VR LE.
    pub meta: Vec<DicomElement>,
    /// Encoding of the data set.
    pub transfer_syntax: TransferSyntax,
    /// Top-level data set elements, in ascending tag order.
    pub dataset: Vec<DicomElement>,
}

// ---------------------------------------------------------------------------
// Reader
// ---------------------------------------------------------------------------

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    base: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8], base: usize) -> Self {
        Self { data, pos: 0, base }
    }

    fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    fn offset(&self) -> usize {
        self.base + self.pos
    }

    fn take(&mut self, n: usize, what: &str) -> Result<&'a [u8]> {
        if self.remaining() < n {
            return Err(parse_err(
                self.offset(),
                format!(
                    "truncated {what}: need {n} bytes, {} available",
                    self.remaining()
                ),
            ));
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn u16(&mut self, be: bool, what: &str) -> Result<u16> {
        let b = self.take(2, what)?;
        let b = [b[0], b[1]];
        Ok(if be {
            u16::from_be_bytes(b)
        } else {
            u16::from_le_bytes(b)
        })
    }

    fn u32(&mut self, be: bool, what: &str) -> Result<u32> {
        let b = self.take(4, what)?;
        let b = [b[0], b[1], b[2], b[3]];
        Ok(if be {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        })
    }

    fn tag(&mut self, be: bool) -> Result<Tag> {
        Ok((self.u16(be, "tag")?, self.u16(be, "tag")?))
    }

    fn peek_tag(&self, be: bool) -> Option<Tag> {
        if self.remaining() < 4 {
            return None;
        }
        let d = &self.data[self.pos..self.pos + 4];
        Some(if be {
            (
                u16::from_be_bytes([d[0], d[1]]),
                u16::from_be_bytes([d[2], d[3]]),
            )
        } else {
            (
                u16::from_le_bytes([d[0], d[1]]),
                u16::from_le_bytes([d[2], d[3]]),
            )
        })
    }

    fn peek_group_le(&self) -> Option<u16> {
        if self.remaining() < 2 {
            return None;
        }
        Some(u16::from_le_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
        ]))
    }
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parse data elements until the reader is exhausted, or (if `delimited`)
/// until an item delimitation item, whose length field is returned.
fn parse_elements(
    r: &mut Reader<'_>,
    ts: TransferSyntax,
    depth: usize,
    delimited: bool,
) -> Result<(Vec<DicomElement>, u32)> {
    let be = ts.big_endian();
    let mut out: Vec<DicomElement> = Vec::new();
    loop {
        if r.is_empty() {
            if delimited {
                return Err(parse_err(
                    r.offset(),
                    "missing item delimitation item (FFFE,E00D)",
                ));
            }
            return Ok((out, 0));
        }
        if delimited && r.peek_tag(be) == Some(ITEM_DELIMITATION) {
            r.tag(be)?;
            let len = r.u32(be, "item delimitation length")?;
            return Ok((out, len));
        }
        let at = r.offset();
        let el = parse_element(r, ts, depth)?;
        if let Some(prev) = out.last() {
            if prev.tag >= el.tag {
                return Err(parse_err(
                    at,
                    format!(
                        "element {} out of order (follows {})",
                        fmt_tag(el.tag),
                        fmt_tag(prev.tag)
                    ),
                ));
            }
        }
        out.push(el);
    }
}

fn parse_element(r: &mut Reader<'_>, ts: TransferSyntax, depth: usize) -> Result<DicomElement> {
    let start = r.offset();
    let be = ts.big_endian();
    let tag = r.tag(be)?;
    if tag.0 == 0xFFFE {
        return Err(parse_err(
            start,
            format!("unexpected item/delimiter tag {}", fmt_tag(tag)),
        ));
    }
    let (vr, reserved, len) = if ts.explicit_vr() {
        let v = r.take(2, "VR")?;
        let vr = [v[0], v[1]];
        if !is_known_vr(&vr) {
            return Err(parse_err(
                start + 4,
                format!(
                    "invalid VR {:?} for element {}",
                    String::from_utf8_lossy(&vr),
                    fmt_tag(tag)
                ),
            ));
        }
        if is_long_vr(&vr) {
            let res = r.take(2, "reserved bytes")?;
            (Some(vr), [res[0], res[1]], r.u32(be, "value length")?)
        } else {
            (Some(vr), [0, 0], u32::from(r.u16(be, "value length")?))
        }
    } else {
        (None, [0, 0], r.u32(be, "value length")?)
    };

    let value = if len == UNDEFINED_LENGTH {
        if tag == tags::PIXEL_DATA {
            ElementValue::Encapsulated(parse_fragments(r, be)?)
        } else {
            let item_syntax = match vr.as_ref() {
                None => ts,
                Some(b"SQ") => ts,
                // PS3.5 §6.2.2: UN with undefined length is an implicit VR LE sequence.
                Some(b"UN") => TransferSyntax::ImplicitVrLittleEndian,
                Some(v) => {
                    return Err(parse_err(
                        start,
                        format!(
                            "undefined length not allowed for VR {} (element {})",
                            String::from_utf8_lossy(v),
                            fmt_tag(tag)
                        ),
                    ))
                }
            };
            ElementValue::Sequence(parse_sequence(r, item_syntax, depth + 1, false)?)
        }
    } else {
        let value_start = r.offset();
        let bytes = r.take(len as usize, "element value")?;
        if vr.as_ref() == Some(b"SQ") {
            let mut sub = Reader::new(bytes, value_start);
            ElementValue::Sequence(parse_sequence(&mut sub, ts, depth + 1, true)?)
        } else {
            ElementValue::Bytes(bytes.to_vec())
        }
    };
    Ok(DicomElement {
        tag,
        vr,
        reserved,
        value,
    })
}

fn parse_sequence(
    r: &mut Reader<'_>,
    ts: TransferSyntax,
    depth: usize,
    defined: bool,
) -> Result<Sequence> {
    if depth > MAX_DEPTH {
        return Err(parse_err(r.offset(), "sequence nesting too deep"));
    }
    let be = ts.big_endian();
    let mut items = Vec::new();
    loop {
        if r.is_empty() {
            if defined {
                return Ok(Sequence {
                    undefined_length: false,
                    item_syntax: ts,
                    items,
                    delimiter_length: 0,
                });
            }
            return Err(parse_err(
                r.offset(),
                "missing sequence delimitation item (FFFE,E0DD)",
            ));
        }
        let at = r.offset();
        let tag = r.tag(be)?;
        let len = r.u32(be, "item length")?;
        match tag {
            ITEM if len == UNDEFINED_LENGTH => {
                let (elements, delimiter_length) = parse_elements(r, ts, depth, true)?;
                items.push(Item {
                    undefined_length: true,
                    elements,
                    delimiter_length,
                });
            }
            ITEM => {
                let body = r.take(len as usize, "item")?;
                let mut sub = Reader::new(body, at + 8);
                let (elements, _) = parse_elements(&mut sub, ts, depth, false)?;
                items.push(Item {
                    undefined_length: false,
                    elements,
                    delimiter_length: 0,
                });
            }
            SEQUENCE_DELIMITATION if !defined => {
                return Ok(Sequence {
                    undefined_length: true,
                    item_syntax: ts,
                    items,
                    delimiter_length: len,
                });
            }
            other => {
                return Err(parse_err(
                    at,
                    format!(
                        "expected item (FFFE,E000) in sequence, found {}",
                        fmt_tag(other)
                    ),
                ))
            }
        }
    }
}

fn parse_fragments(r: &mut Reader<'_>, be: bool) -> Result<Encapsulated> {
    let mut fragments = Vec::new();
    loop {
        if r.is_empty() {
            return Err(parse_err(
                r.offset(),
                "missing sequence delimitation item after encapsulated pixel data",
            ));
        }
        let at = r.offset();
        let tag = r.tag(be)?;
        let len = r.u32(be, "fragment length")?;
        match tag {
            ITEM if len != UNDEFINED_LENGTH => {
                fragments.push(r.take(len as usize, "pixel data fragment")?.to_vec())
            }
            SEQUENCE_DELIMITATION => {
                return Ok(Encapsulated {
                    fragments,
                    delimiter_length: len,
                })
            }
            other => {
                return Err(parse_err(
                    at,
                    format!("invalid item {} in encapsulated pixel data", fmt_tag(other)),
                ))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Serialization
// ---------------------------------------------------------------------------

fn put_u16(out: &mut Vec<u8>, v: u16, be: bool) {
    out.extend_from_slice(&if be { v.to_be_bytes() } else { v.to_le_bytes() });
}

fn put_u32(out: &mut Vec<u8>, v: u32, be: bool) {
    out.extend_from_slice(&if be { v.to_be_bytes() } else { v.to_le_bytes() });
}

fn put_tag(out: &mut Vec<u8>, tag: Tag, be: bool) {
    put_u16(out, tag.0, be);
    put_u16(out, tag.1, be);
}

fn defined_len(n: usize, what: &str) -> Result<u32> {
    u32::try_from(n)
        .ok()
        .filter(|&l| l != UNDEFINED_LENGTH)
        .ok_or_else(|| SteganoError::InvalidParameter(format!("{what} too large for DICOM")))
}

fn write_elements(out: &mut Vec<u8>, els: &[DicomElement], ts: TransferSyntax) -> Result<()> {
    for el in els {
        write_element(out, el, ts)?;
    }
    Ok(())
}

fn write_element(out: &mut Vec<u8>, el: &DicomElement, ts: TransferSyntax) -> Result<()> {
    let be = ts.big_endian();
    let mut body = Vec::new();
    let len = match &el.value {
        ElementValue::Bytes(b) => defined_len(b.len(), "element value")?,
        ElementValue::Sequence(seq) => {
            let ibe = seq.item_syntax.big_endian();
            for item in &seq.items {
                let mut ib = Vec::new();
                write_elements(&mut ib, &item.elements, seq.item_syntax)?;
                put_tag(&mut body, ITEM, ibe);
                if item.undefined_length {
                    put_u32(&mut body, UNDEFINED_LENGTH, ibe);
                    body.extend_from_slice(&ib);
                    put_tag(&mut body, ITEM_DELIMITATION, ibe);
                    put_u32(&mut body, item.delimiter_length, ibe);
                } else {
                    put_u32(&mut body, defined_len(ib.len(), "item")?, ibe);
                    body.extend_from_slice(&ib);
                }
            }
            if seq.undefined_length {
                put_tag(&mut body, SEQUENCE_DELIMITATION, ibe);
                put_u32(&mut body, seq.delimiter_length, ibe);
                UNDEFINED_LENGTH
            } else {
                defined_len(body.len(), "sequence")?
            }
        }
        ElementValue::Encapsulated(enc) => {
            for f in &enc.fragments {
                put_tag(&mut body, ITEM, be);
                put_u32(&mut body, defined_len(f.len(), "pixel data fragment")?, be);
                body.extend_from_slice(f);
            }
            put_tag(&mut body, SEQUENCE_DELIMITATION, be);
            put_u32(&mut body, enc.delimiter_length, be);
            UNDEFINED_LENGTH
        }
    };

    put_tag(out, el.tag, be);
    if ts.explicit_vr() {
        let vr = el.vr.ok_or_else(|| {
            SteganoError::InvalidParameter(format!(
                "element {} has no VR in an explicit VR data set",
                fmt_tag(el.tag)
            ))
        })?;
        out.extend_from_slice(&vr);
        if is_long_vr(&vr) {
            out.extend_from_slice(&el.reserved);
            put_u32(out, len, be);
        } else {
            let short = u16::try_from(len).map_err(|_| {
                SteganoError::InvalidParameter(format!(
                    "value of element {} ({len} bytes) exceeds the 16-bit length of VR {}",
                    fmt_tag(el.tag),
                    String::from_utf8_lossy(&vr)
                ))
            })?;
            put_u16(out, short, be);
        }
    } else {
        put_u32(out, len, be);
    }
    match &el.value {
        ElementValue::Bytes(b) => out.extend_from_slice(b),
        _ => out.extend_from_slice(&body),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// DicomFile
// ---------------------------------------------------------------------------

fn bytes_of(el: &DicomElement) -> Option<&[u8]> {
    match &el.value {
        ElementValue::Bytes(b) => Some(b),
        _ => None,
    }
}

fn trimmed_str(b: &[u8]) -> String {
    String::from_utf8_lossy(b)
        .trim_end_matches(['\0', ' '])
        .trim_start_matches(' ')
        .to_string()
}

impl DicomFile {
    /// Parse a DICOM file (see module docs for what is supported).
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.is_empty() {
            return Err(SteganoError::IntegrityError(
                "Empty DICOM input".to_string(),
            ));
        }
        let (preamble, start) =
            if data.len() >= PREAMBLE_LEN + 4 && &data[PREAMBLE_LEN..PREAMBLE_LEN + 4] == PREFIX {
                (Some(data[..PREAMBLE_LEN].to_vec()), PREAMBLE_LEN + 4)
            } else {
                (None, 0)
            };
        let mut r = Reader::new(&data[start..], start);

        // File Meta Information: always explicit VR little endian.
        let mut meta: Vec<DicomElement> = Vec::new();
        while r.peek_group_le() == Some(0x0002) {
            let at = r.offset();
            let el = parse_element(&mut r, TransferSyntax::ExplicitVrLittleEndian, 0)?;
            if meta.last().is_some_and(|p| p.tag >= el.tag) {
                return Err(parse_err(
                    at,
                    format!("meta element {} out of order", fmt_tag(el.tag)),
                ));
            }
            meta.push(el);
        }

        let transfer_syntax = if meta.is_empty() {
            // No meta group: guess explicit vs implicit VR LE from the first header.
            let rest = &data[r.offset()..];
            if rest.len() >= 6 && is_known_vr(&[rest[4], rest[5]]) {
                TransferSyntax::ExplicitVrLittleEndian
            } else {
                TransferSyntax::ImplicitVrLittleEndian
            }
        } else {
            let el = meta
                .iter()
                .find(|e| e.tag == tags::TRANSFER_SYNTAX_UID)
                .ok_or_else(|| {
                    SteganoError::IntegrityError(
                        "File Meta Information lacks Transfer Syntax UID (0002,0010)".to_string(),
                    )
                })?;
            let uid = bytes_of(el).map(trimmed_str).ok_or_else(|| {
                SteganoError::IntegrityError("Malformed Transfer Syntax UID".to_string())
            })?;
            TransferSyntax::from_uid(&uid)?
        };

        let ds_start = r.offset();
        let (dataset, _) = parse_elements(&mut r, transfer_syntax, 0, false)?;
        if let Some(bad) = dataset.iter().find(|e| e.tag.0 < 0x0004) {
            return Err(parse_err(
                ds_start,
                format!("element {} is not allowed in a data set", fmt_tag(bad.tag)),
            ));
        }
        if preamble.is_none() && meta.is_empty() && dataset.is_empty() {
            return Err(SteganoError::IntegrityError("No DICOM content".to_string()));
        }
        Ok(Self {
            preamble,
            meta,
            transfer_syntax,
            dataset,
        })
    }

    /// Serialize back to bytes.  Unmodified files round-trip byte-identically.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        if let Some(p) = &self.preamble {
            if p.len() != PREAMBLE_LEN {
                return Err(SteganoError::InvalidParameter(
                    "DICOM preamble must be 128 bytes".to_string(),
                ));
            }
            out.extend_from_slice(p);
            out.extend_from_slice(PREFIX);
        }
        write_elements(&mut out, &self.meta, TransferSyntax::ExplicitVrLittleEndian)?;
        write_elements(&mut out, &self.dataset, self.transfer_syntax)?;
        Ok(out)
    }

    /// Top-level data set element by tag.
    pub fn get(&self, tag: Tag) -> Option<&DicomElement> {
        self.dataset
            .binary_search_by(|e| e.tag.cmp(&tag))
            .ok()
            .map(|i| &self.dataset[i])
    }

    fn get_mut(&mut self, tag: Tag) -> Option<&mut DicomElement> {
        self.dataset
            .binary_search_by(|e| e.tag.cmp(&tag))
            .ok()
            .map(move |i| &mut self.dataset[i])
    }

    /// Insert (or replace) a top-level element, keeping ascending tag order.
    pub fn insert(&mut self, el: DicomElement) {
        match self.dataset.binary_search_by(|e| e.tag.cmp(&el.tag)) {
            Ok(i) => self.dataset[i] = el,
            Err(i) => self.dataset.insert(i, el),
        }
    }

    /// Transfer Syntax UID from the meta group, if present.
    pub fn transfer_syntax_uid(&self) -> Option<String> {
        self.meta
            .iter()
            .find(|e| e.tag == tags::TRANSFER_SYNTAX_UID)
            .and_then(bytes_of)
            .map(trimmed_str)
    }

    /// Read a top-level US value.
    fn read_us(&self, tag: Tag) -> Result<Option<u16>> {
        let Some(el) = self.get(tag) else {
            return Ok(None);
        };
        match bytes_of(el) {
            Some(b) if b.len() >= 2 => Ok(Some(if self.transfer_syntax.big_endian() {
                u16::from_be_bytes([b[0], b[1]])
            } else {
                u16::from_le_bytes([b[0], b[1]])
            })),
            _ => Err(SteganoError::IntegrityError(format!(
                "Malformed US value for {}",
                fmt_tag(tag)
            ))),
        }
    }

    /// Byte-level layout of an element we create (VR when explicit).
    fn new_element(&self, tag: Tag, vr: &[u8; 2], value: Vec<u8>) -> DicomElement {
        DicomElement {
            tag,
            vr: self.transfer_syntax.explicit_vr().then_some(*vr),
            reserved: [0, 0],
            value: ElementValue::Bytes(value),
        }
    }
}

// ---------------------------------------------------------------------------
// Pixel layout
// ---------------------------------------------------------------------------

/// Where the LSB of each native pixel sample lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelLayout {
    /// Bits Allocated (0028,0100).
    pub bits_allocated: u16,
    /// Bits Stored (0028,0101).
    pub bits_stored: u16,
    /// High Bit (0028,0102).
    pub high_bit: u16,
    /// Pixel Representation (0028,0103): 0 unsigned, 1 two's complement.
    pub pixel_representation: u16,
    /// Samples per Pixel (0028,0002).
    pub samples_per_pixel: u16,
    /// Number of samples available for embedding.
    pub samples: usize,
    bytes_per_sample: usize,
    byte_in_sample: usize,
    bit: u8,
}

impl PixelLayout {
    fn locate(&self, i: usize) -> (usize, u8) {
        (i * self.bytes_per_sample + self.byte_in_sample, self.bit)
    }
}

fn parse_is(el: Option<&DicomElement>) -> Option<usize> {
    el.and_then(bytes_of)
        .and_then(|b| trimmed_str(b).parse::<usize>().ok())
}

fn pixel_layout(file: &DicomFile) -> Result<PixelLayout> {
    let el = file
        .get(tags::PIXEL_DATA)
        .ok_or_else(|| SteganoError::IntegrityError("No pixel data found in DICOM".to_string()))?;
    let px = match &el.value {
        ElementValue::Bytes(b) => b,
        ElementValue::Encapsulated(_) => {
            return Err(SteganoError::InvalidParameter(format!(
                "pixel_lsb cannot embed into encapsulated (compressed) pixel data \
                 (transfer syntax {})",
                file.transfer_syntax_uid().unwrap_or_default()
            )))
        }
        ElementValue::Sequence(_) => {
            return Err(SteganoError::IntegrityError(
                "Pixel Data encoded as a sequence".to_string(),
            ))
        }
    };
    let missing =
        |n: &str| SteganoError::IntegrityError(format!("pixel_lsb requires {n} in the data set"));
    let bits_allocated = file
        .read_us(tags::BITS_ALLOCATED)?
        .ok_or_else(|| missing("Bits Allocated (0028,0100)"))?;
    let bits_stored = file.read_us(tags::BITS_STORED)?.unwrap_or(bits_allocated);
    let high_bit = file
        .read_us(tags::HIGH_BIT)?
        .unwrap_or(bits_stored.saturating_sub(1));
    let pixel_representation = file.read_us(tags::PIXEL_REPRESENTATION)?.unwrap_or(0);
    let samples_per_pixel = file.read_us(tags::SAMPLES_PER_PIXEL)?.unwrap_or(1);

    let be = file.transfer_syntax.big_endian();
    let supported = match bits_allocated {
        8 | 16 => true,
        32 => !be,
        _ => false,
    };
    if !supported {
        return Err(SteganoError::InvalidParameter(format!(
            "pixel_lsb does not support Bits Allocated = {bits_allocated}{}",
            if be { " in big endian" } else { "" }
        )));
    }
    if bits_stored == 0
        || bits_stored > bits_allocated
        || high_bit >= bits_allocated
        || high_bit + 1 < bits_stored
    {
        return Err(SteganoError::IntegrityError(format!(
            "Inconsistent pixel module: Bits Allocated {bits_allocated}, \
             Bits Stored {bits_stored}, High Bit {high_bit}"
        )));
    }
    let bytes_per_sample = usize::from(bits_allocated / 8);
    let low_bit = usize::from(high_bit + 1 - bits_stored);
    let byte_in_sample = if be {
        bytes_per_sample - 1 - low_bit / 8
    } else {
        low_bit / 8
    };

    let available = px.len() / bytes_per_sample;
    let samples = match (file.read_us(tags::ROWS)?, file.read_us(tags::COLUMNS)?) {
        (Some(rows), Some(cols)) => {
            let frames = parse_is(file.get(tags::NUMBER_OF_FRAMES)).unwrap_or(1);
            let expected = usize::from(rows)
                .checked_mul(usize::from(cols))
                .and_then(|n| n.checked_mul(usize::from(samples_per_pixel)))
                .and_then(|n| n.checked_mul(frames))
                .ok_or_else(|| SteganoError::IntegrityError("Pixel count overflow".into()))?;
            if expected > available {
                return Err(SteganoError::IntegrityError(format!(
                    "Pixel Data holds {available} samples but Rows×Columns×SamplesPerPixel×Frames = {expected}"
                )));
            }
            expected
        }
        _ => available,
    };

    Ok(PixelLayout {
        bits_allocated,
        bits_stored,
        high_bit,
        pixel_representation,
        samples_per_pixel,
        samples,
        bytes_per_sample,
        byte_in_sample,
        bit: (low_bit % 8) as u8,
    })
}

// ---------------------------------------------------------------------------
// Strategies & frames
// ---------------------------------------------------------------------------

/// DICOM embedding strategies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DicomEmbedStrategy {
    /// Private block: creator `(gggg,00xx)` LO `STEGANO` + data `(gggg,xx10)` OB.
    PrivateDataElements,
    /// LSB of each native pixel sample (changes pixel values by at most 1).
    PixelDataLSB,
    /// The 128-byte preamble (frame ≤ 128 bytes, i.e. ≤ 119 codec bytes).
    UnusedMetadata,
    /// `(FFFC,FFFC)` Data Set Trailing Padding element (OB).
    SequencePadding,
    /// Redundant copies in private elements and (when it fits) pixel LSBs.
    MultiStrategy,
}

impl DicomEmbedStrategy {
    fn code(self) -> u8 {
        match self {
            Self::PrivateDataElements => 1,
            Self::PixelDataLSB => 2,
            Self::UnusedMetadata => 3,
            Self::SequencePadding => 4,
            Self::MultiStrategy => 5,
        }
    }

    fn from_code(c: u8) -> Option<Self> {
        Some(match c {
            1 => Self::PrivateDataElements,
            2 => Self::PixelDataLSB,
            3 => Self::UnusedMetadata,
            4 => Self::SequencePadding,
            5 => Self::MultiStrategy,
            _ => return None,
        })
    }

    /// Parse a `dicom_strategy` value.
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "private" | "private_elements" => Self::PrivateDataElements,
            "pixel_lsb" | "pixel" => Self::PixelDataLSB,
            "metadata" | "unused_metadata" | "preamble" => Self::UnusedMetadata,
            "sequence" | "padding" | "sequence_padding" => Self::SequencePadding,
            "multi" => Self::MultiStrategy,
            _ => return None,
        })
    }

    fn locations(self) -> &'static [Location] {
        match self {
            Self::PrivateDataElements => &[Location::Private],
            Self::PixelDataLSB => &[Location::Pixel],
            Self::UnusedMetadata => &[Location::Preamble],
            Self::SequencePadding => &[Location::Padding],
            Self::MultiStrategy => &[Location::Private, Location::Pixel],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Location {
    Preamble,
    Private,
    Padding,
    Pixel,
}

impl Location {
    const ALL: [Location; 4] = [
        Location::Preamble,
        Location::Private,
        Location::Padding,
        Location::Pixel,
    ];

    fn native(self) -> DicomEmbedStrategy {
        match self {
            Location::Preamble => DicomEmbedStrategy::UnusedMetadata,
            Location::Private => DicomEmbedStrategy::PrivateDataElements,
            Location::Padding => DicomEmbedStrategy::SequencePadding,
            Location::Pixel => DicomEmbedStrategy::PixelDataLSB,
        }
    }
}

const FRAME_MAGIC: &[u8; 4] = b"STGD";
const FRAME_OVERHEAD: usize = 9;
/// Policy limit on the frame stored in a single element (private / padding).
const MAX_ELEMENT_FRAME: usize = 16 * 1024 * 1024;
const PRIVATE_CREATOR: &str = "STEGANO";
const DEFAULT_PRIVATE_GROUP: u16 = 0x0009;

fn build_frame(strategy: DicomEmbedStrategy, body: &[u8]) -> Result<Vec<u8>> {
    let len = u32::try_from(body.len())
        .map_err(|_| SteganoError::InvalidParameter("Payload too large".to_string()))?;
    let mut f = Vec::with_capacity(FRAME_OVERHEAD + body.len());
    f.extend_from_slice(FRAME_MAGIC);
    f.push(strategy.code());
    f.extend_from_slice(&len.to_le_bytes());
    f.extend_from_slice(body);
    Ok(f)
}

/// Parse a frame header: returns (strategy, body length).
fn frame_header(h: &[u8]) -> Option<(DicomEmbedStrategy, usize)> {
    if h.len() < FRAME_OVERHEAD || &h[..4] != FRAME_MAGIC {
        return None;
    }
    let s = DicomEmbedStrategy::from_code(h[4])?;
    Some((s, u32::from_le_bytes([h[5], h[6], h[7], h[8]]) as usize))
}

fn frame_body(buf: &[u8]) -> Option<(DicomEmbedStrategy, &[u8])> {
    let (s, len) = frame_header(buf)?;
    buf.get(FRAME_OVERHEAD..FRAME_OVERHEAD.checked_add(len)?)
        .map(|b| (s, b))
}

fn pad_even(mut v: Vec<u8>) -> Vec<u8> {
    if v.len() % 2 == 1 {
        v.push(0);
    }
    v
}

fn is_stegano_creator(el: &DicomElement) -> bool {
    el.tag.0 % 2 == 1
        && (0x0010..=0x00FF).contains(&el.tag.1)
        && bytes_of(el).is_some_and(|b| trimmed_str(b) == PRIVATE_CREATOR)
}

/// `(group, block)` of the first STEGANO private block.
fn find_private_block(file: &DicomFile) -> Option<(u16, u8)> {
    file.dataset
        .iter()
        .find(|e| is_stegano_creator(e))
        .map(|e| (e.tag.0, e.tag.1 as u8))
}

fn private_data_tag(group: u16, block: u8) -> Tag {
    (group, (u16::from(block) << 8) | 0x0010)
}

/// Result details of a DICOM embedding.
#[derive(Debug, Clone)]
pub struct DicomEmbedResult {
    pub success: bool,
    pub bytes_embedded: usize,
    pub strategy_used: DicomEmbedStrategy,
    pub elements_modified: usize,
    pub pixels_modified: usize,
    /// `false` whenever pixel data was changed.
    pub medical_integrity_preserved: bool,
    pub dicom_compliance_maintained: bool,
    /// Human-readable description of what was modified.
    pub integrity_note: String,
}

/// Handler for DICOM medical imaging files.
pub struct DicomHandler;

impl Default for DicomHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl DicomHandler {
    pub fn new() -> Self {
        Self
    }

    fn strategy_param(params: &EmbedParams) -> Result<Option<DicomEmbedStrategy>> {
        match params.format_specific.get("dicom_strategy") {
            None => Ok(None),
            Some(v) => {
                let name = v.as_str().ok_or_else(|| {
                    SteganoError::InvalidParameter("dicom_strategy must be a string".to_string())
                })?;
                DicomEmbedStrategy::from_name(name).map(Some).ok_or_else(|| {
                    SteganoError::InvalidParameter(format!(
                        "Unknown dicom_strategy {name:?} (expected private|metadata|sequence|pixel_lsb|multi)"
                    ))
                })
            }
        }
    }

    fn private_group_param(params: &EmbedParams) -> Result<u16> {
        let Some(v) = params.format_specific.get("dicom_private_group") else {
            return Ok(DEFAULT_PRIVATE_GROUP);
        };
        let g = v
            .as_u64()
            .and_then(|g| u16::try_from(g).ok())
            .filter(|g| g % 2 == 1 && (0x0009..=0xFFFD).contains(g))
            .ok_or_else(|| {
                SteganoError::InvalidParameter(format!(
                    "dicom_private_group must be an odd group in 0x0009..=0xFFFD, got {v}"
                ))
            })?;
        Ok(g)
    }

    // ---- private --------------------------------------------------------

    fn embed_private(&self, file: &mut DicomFile, frame: &[u8], group: u16) -> Result<usize> {
        if frame.len() > MAX_ELEMENT_FRAME {
            return Err(SteganoError::InsufficientCapacity {
                needed: frame.len() * 8,
                available: MAX_ELEMENT_FRAME * 8,
            });
        }
        // Drop any previous STEGANO blocks (creator + its reserved range).
        let blocks: Vec<(u16, u8)> = file
            .dataset
            .iter()
            .filter(|e| is_stegano_creator(e))
            .map(|e| (e.tag.0, e.tag.1 as u8))
            .collect();
        file.dataset.retain(|e| {
            !blocks.iter().any(|&(g, b)| {
                e.tag.0 == g && (e.tag.1 == u16::from(b) || (e.tag.1 >> 8) as u8 == b)
            })
        });

        // Choose a free creator slot in `group`.
        let block = (0x10u8..=0xFF)
            .find(|&b| {
                !file.dataset.iter().any(|e| {
                    e.tag.0 == group && (e.tag.1 == u16::from(b) || e.tag.1 >> 8 == u16::from(b))
                })
            })
            .ok_or_else(|| {
                SteganoError::InvalidParameter(format!(
                    "No free private creator slot in group {group:04X}"
                ))
            })?;

        let creator = file.new_element(
            (group, u16::from(block)),
            b"LO",
            pad_even_with(PRIVATE_CREATOR, b' '),
        );
        let data = file.new_element(
            private_data_tag(group, block),
            b"OB",
            pad_even(frame.to_vec()),
        );
        file.insert(creator);
        file.insert(data);
        Ok(2)
    }

    fn read_private(&self, file: &DicomFile) -> Option<Vec<u8>> {
        let (g, b) = find_private_block(file)?;
        file.get(private_data_tag(g, b))
            .and_then(bytes_of)
            .map(<[u8]>::to_vec)
    }

    // ---- padding --------------------------------------------------------

    fn embed_padding(&self, file: &mut DicomFile, frame: &[u8]) -> Result<usize> {
        if frame.len() > MAX_ELEMENT_FRAME {
            return Err(SteganoError::InsufficientCapacity {
                needed: frame.len() * 8,
                available: MAX_ELEMENT_FRAME * 8,
            });
        }
        let el = file.new_element(tags::TRAILING_PADDING, b"OB", pad_even(frame.to_vec()));
        file.insert(el);
        Ok(1)
    }

    // ---- preamble -------------------------------------------------------

    fn embed_preamble(&self, file: &mut DicomFile, frame: &[u8], overwrite: bool) -> Result<()> {
        let p = file.preamble.as_mut().ok_or_else(|| {
            SteganoError::InvalidParameter(
                "metadata strategy requires a DICOM Part 10 file with a 128-byte preamble"
                    .to_string(),
            )
        })?;
        if frame.len() > PREAMBLE_LEN {
            return Err(SteganoError::InsufficientCapacity {
                needed: frame.len() * 8,
                available: PREAMBLE_LEN * 8,
            });
        }
        if !overwrite && p.iter().any(|&b| b != 0) && frame_header(p).is_none() {
            return Err(SteganoError::InvalidParameter(
                "DICOM preamble is in use by another application (non-zero); set \
                 format_specific[\"dicom_overwrite_preamble\"] = true to overwrite it"
                    .to_string(),
            ));
        }
        p.fill(0);
        p[..frame.len()].copy_from_slice(frame);
        Ok(())
    }

    // ---- pixel ----------------------------------------------------------

    fn embed_pixel(&self, file: &mut DicomFile, frame: &[u8]) -> Result<usize> {
        let layout = pixel_layout(file)?;
        let needed = frame.len() * 8;
        if needed > layout.samples {
            return Err(SteganoError::InsufficientCapacity {
                needed,
                available: layout.samples,
            });
        }
        let Some(DicomElement {
            value: ElementValue::Bytes(px),
            ..
        }) = file.get_mut(tags::PIXEL_DATA)
        else {
            unreachable!("pixel_layout validated native pixel data");
        };
        let mut modified = 0;
        for i in 0..needed {
            let bit = (frame[i / 8] >> (i % 8)) & 1;
            let (idx, pos) = layout.locate(i);
            let old = px[idx];
            let new = (old & !(1 << pos)) | (bit << pos);
            if new != old {
                px[idx] = new;
                modified += 1;
            }
        }
        Ok(modified)
    }

    fn read_pixel(&self, file: &DicomFile) -> Option<Vec<u8>> {
        let layout = pixel_layout(file).ok()?;
        let px = file.get(tags::PIXEL_DATA).and_then(bytes_of)?;
        let read = |nbytes: usize, from_bit: usize| -> Vec<u8> {
            let mut out = vec![0u8; nbytes];
            for (j, byte) in out.iter_mut().enumerate() {
                for k in 0..8 {
                    let (idx, pos) = layout.locate(from_bit + j * 8 + k);
                    *byte |= ((px[idx] >> pos) & 1) << k;
                }
            }
            out
        };
        if layout.samples < FRAME_OVERHEAD * 8 {
            return None;
        }
        let header = read(FRAME_OVERHEAD, 0);
        let (_, len) = frame_header(&header)?;
        let total = FRAME_OVERHEAD.checked_add(len)?;
        if total.checked_mul(8)? > layout.samples {
            return None;
        }
        let mut frame = header;
        frame.extend(read(len, FRAME_OVERHEAD * 8));
        Some(frame)
    }

    // ---- dispatch -------------------------------------------------------

    fn read_location(&self, file: &DicomFile, loc: Location) -> Option<Vec<u8>> {
        match loc {
            Location::Preamble => file.preamble.clone(),
            Location::Private => self.read_private(file),
            Location::Padding => file
                .get(tags::TRAILING_PADDING)
                .and_then(bytes_of)
                .map(<[u8]>::to_vec),
            Location::Pixel => self.read_pixel(file),
        }
    }

    /// Locate the embedded frame body without decoding it.
    fn find_frame(
        &self,
        file: &DicomFile,
        requested: Option<DicomEmbedStrategy>,
    ) -> Result<(DicomEmbedStrategy, Vec<u8>)> {
        let locations: &[Location] = match requested {
            Some(s) => s.locations(),
            None => &Location::ALL,
        };
        for &loc in locations {
            let Some(buf) = self.read_location(file, loc) else {
                continue;
            };
            if let Some((s, body)) = frame_body(&buf) {
                if s == loc.native() || s == DicomEmbedStrategy::MultiStrategy {
                    return Ok((s, body.to_vec()));
                }
            }
        }
        Err(SteganoError::IntegrityError(
            "No steganographic data found in DICOM file".to_string(),
        ))
    }

    /// Embed into DICOM bytes, returning the new file and detailed result.
    pub fn embed_dicom(
        &self,
        data: &[u8],
        payload: &[u8],
        params: &EmbedParams,
    ) -> Result<(Vec<u8>, DicomEmbedResult)> {
        let mut file = DicomFile::parse(data)?;
        let strategy =
            Self::strategy_param(params)?.unwrap_or(DicomEmbedStrategy::PrivateDataElements);
        let group = Self::private_group_param(params)?;
        let overwrite_preamble = params
            .format_specific
            .get("dicom_overwrite_preamble")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let encoded = crate::codec::encode_payload(payload, params)?;
        let frame = build_frame(strategy, &encoded)?;

        let mut elements_modified = 0;
        let mut pixels_modified = 0;
        let mut pixel_touched = false;
        let note;
        match strategy {
            DicomEmbedStrategy::PrivateDataElements => {
                elements_modified = self.embed_private(&mut file, &frame, group)?;
                note = format!("added private block (group {group:04X}); pixel data unchanged");
            }
            DicomEmbedStrategy::SequencePadding => {
                elements_modified = self.embed_padding(&mut file, &frame)?;
                note = "set (FFFC,FFFC) trailing padding; pixel data unchanged".to_string();
            }
            DicomEmbedStrategy::UnusedMetadata => {
                self.embed_preamble(&mut file, &frame, overwrite_preamble)?;
                note = "rewrote 128-byte preamble; data set unchanged".to_string();
            }
            DicomEmbedStrategy::PixelDataLSB => {
                pixels_modified = self.embed_pixel(&mut file, &frame)?;
                elements_modified = 1;
                pixel_touched = true;
                note = format!(
                    "modified the least-significant stored bit of {} samples (each changed by at most 1)",
                    frame.len() * 8
                );
            }
            DicomEmbedStrategy::MultiStrategy => {
                elements_modified = self.embed_private(&mut file, &frame, group)?;
                let fits = pixel_layout(&file).is_ok_and(|l| frame.len() * 8 <= l.samples);
                if fits {
                    pixels_modified = self.embed_pixel(&mut file, &frame)?;
                    elements_modified += 1;
                    pixel_touched = true;
                    note = "private block + pixel LSB copy (pixel values changed by at most 1)"
                        .to_string();
                } else {
                    note = "private block only (pixel data unavailable or too small)".to_string();
                }
            }
        }

        let out = file.to_bytes()?;
        Ok((
            out,
            DicomEmbedResult {
                success: true,
                bytes_embedded: payload.len(),
                strategy_used: strategy,
                elements_modified,
                pixels_modified,
                medical_integrity_preserved: !pixel_touched,
                dicom_compliance_maintained: true,
                integrity_note: note,
            },
        ))
    }

    /// Extract from DICOM bytes.
    pub fn extract_dicom(&self, data: &[u8], params: &EmbedParams) -> Result<Vec<u8>> {
        let file = DicomFile::parse(data)?;
        let (_, body) = self.find_frame(&file, Self::strategy_param(params)?)?;
        crate::codec::decode_payload(&body, params)
    }

    /// Frame-byte capacity of each strategy (`None` = not available).
    pub fn strategy_capacities(
        &self,
        data: &[u8],
    ) -> Result<Vec<(DicomEmbedStrategy, Option<usize>)>> {
        let file = DicomFile::parse(data)?;
        let body = |frame: usize| frame.saturating_sub(FRAME_OVERHEAD);
        let pixel = pixel_layout(&file).ok().map(|l| body(l.samples / 8));
        let element = body(MAX_ELEMENT_FRAME);
        Ok(vec![
            (DicomEmbedStrategy::PrivateDataElements, Some(element)),
            (
                DicomEmbedStrategy::UnusedMetadata,
                file.preamble.as_ref().map(|_| body(PREAMBLE_LEN)),
            ),
            (DicomEmbedStrategy::SequencePadding, Some(element)),
            (DicomEmbedStrategy::PixelDataLSB, pixel),
            (
                DicomEmbedStrategy::MultiStrategy,
                Some(pixel.map_or(element, |p| p.min(element))),
            ),
        ])
    }

    fn verify_dicom_integrity(&self, data: &[u8]) -> bool {
        let Ok(file) = DicomFile::parse(data) else {
            return false;
        };
        if file.preamble.is_some() && file.meta.is_empty() {
            return false;
        }
        match file.get(tags::PIXEL_DATA).map(|e| &e.value) {
            Some(ElementValue::Bytes(_)) => match file.read_us(tags::BITS_ALLOCATED) {
                Ok(Some(8 | 16 | 32)) => pixel_layout(&file).is_ok(),
                Ok(_) => true,
                Err(_) => false,
            },
            _ => true,
        }
    }

    fn is_dicom(&self, data: &[u8]) -> bool {
        (data.len() >= PREAMBLE_LEN + 4 && &data[PREAMBLE_LEN..PREAMBLE_LEN + 4] == PREFIX)
            // No preamble: starts with (0002,0000) UL.
            || data.starts_with(&[0x02, 0x00, 0x00, 0x00, b'U', b'L'])
    }
}

fn pad_even_with(s: &str, pad: u8) -> Vec<u8> {
    let mut v = s.as_bytes().to_vec();
    if v.len() % 2 == 1 {
        v.push(pad);
    }
    v
}

#[async_trait]
impl DataHandler for DicomHandler {
    fn detect_format(&self, data: &[u8]) -> Option<DataFormat> {
        self.is_dicom(data).then_some(DataFormat::DICOM)
    }

    fn estimate_capacity(&self, carrier: &CarrierData) -> Result<CapacityInfo> {
        if carrier.format != DataFormat::DICOM {
            return Err(SteganoError::UnsupportedFormat(carrier.format.clone()));
        }
        let caps = self.strategy_capacities(&carrier.raw_data)?;
        // Default strategy: private data elements.
        let total_bytes = caps[0].1.unwrap_or(0);
        let recommended_bytes = total_bytes.min((carrier.raw_data.len() / 10).max(4096));
        let mut info = CapacityInfo::new(total_bytes * 8, recommended_bytes * 8);
        info.format_specific_info.insert(
            "method".to_string(),
            serde_json::Value::String("PrivateDataElements".to_string()),
        );
        for (s, c) in caps {
            info.format_specific_info.insert(
                format!("{s:?}_bytes"),
                c.map_or(serde_json::Value::Null, |c| c.into()),
            );
        }
        Ok(info)
    }

    async fn embed(
        &self,
        carrier: &mut CarrierData,
        payload: &[u8],
        params: &EmbedParams,
    ) -> Result<EmbedResult> {
        if carrier.format != DataFormat::DICOM {
            return Err(SteganoError::UnsupportedFormat(carrier.format.clone()));
        }
        let start = Instant::now();
        let (out, result) = self.embed_dicom(&carrier.raw_data, payload, params)?;
        carrier.raw_data = out;
        Ok(EmbedResult {
            success: result.success,
            bytes_embedded: result.bytes_embedded,
            carrier_modified: true,
            performance_stats: PerformanceStats {
                embedding_time_ms: start.elapsed().as_millis() as u64,
                encryption_time_ms: 0,
                gpu_acceleration_used: false,
                memory_usage_bytes: carrier.raw_data.len() + payload.len(),
            },
            validation_report: None,
        })
    }

    async fn extract(&self, carrier: &CarrierData, params: &EmbedParams) -> Result<Vec<u8>> {
        if carrier.format != DataFormat::DICOM {
            return Err(SteganoError::UnsupportedFormat(carrier.format.clone()));
        }
        self.extract_dicom(&carrier.raw_data, params)
    }

    fn verify_integrity(&self, carrier: &CarrierData) -> Result<bool> {
        if carrier.format != DataFormat::DICOM {
            return Err(SteganoError::UnsupportedFormat(carrier.format.clone()));
        }
        Ok(self.verify_dicom_integrity(&carrier.raw_data))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Spec-correct byte builder (independent of the parser/serializer above)
    // -----------------------------------------------------------------------

    /// Writes data elements in a given transfer syntax, per PS3.5 §7.1.
    struct B {
        explicit: bool,
        be: bool,
        out: Vec<u8>,
    }

    impl B {
        fn new(ts: TransferSyntax) -> Self {
            Self {
                explicit: ts.explicit_vr(),
                be: ts.big_endian(),
                out: Vec::new(),
            }
        }

        fn u16(&mut self, v: u16) {
            let b = if self.be {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            };
            self.out.extend_from_slice(&b);
        }

        fn u32(&mut self, v: u32) {
            let b = if self.be {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            };
            self.out.extend_from_slice(&b);
        }

        /// Element header with an explicit length (may be `UNDEFINED_LENGTH`).
        fn header(&mut self, tag: Tag, vr: &[u8; 2], len: u32) -> &mut Self {
            self.u16(tag.0);
            self.u16(tag.1);
            if self.explicit {
                self.out.extend_from_slice(vr);
                if matches!(
                    vr,
                    b"OB"
                        | b"OD"
                        | b"OF"
                        | b"OL"
                        | b"OV"
                        | b"OW"
                        | b"SQ"
                        | b"SV"
                        | b"UC"
                        | b"UN"
                        | b"UR"
                        | b"UT"
                        | b"UV"
                ) {
                    self.out.extend_from_slice(&[0, 0]);
                    self.u32(len);
                } else {
                    self.u16(u16::try_from(len).unwrap());
                }
            } else {
                self.u32(len);
            }
            self
        }

        fn el(&mut self, tag: Tag, vr: &[u8; 2], value: &[u8]) -> &mut Self {
            assert_eq!(value.len() % 2, 0, "values have even length");
            self.header(tag, vr, value.len() as u32);
            self.out.extend_from_slice(value);
            self
        }

        fn us(&mut self, tag: Tag, v: u16) -> &mut Self {
            let b = if self.be {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            };
            self.el(tag, b"US", &b)
        }

        /// Item / delimiter header (FFFE,xxxx) + 32-bit length; no VR ever.
        fn item(&mut self, tag: Tag, len: u32) -> &mut Self {
            self.u16(tag.0);
            self.u16(tag.1);
            self.u32(len);
            self
        }

        fn raw(&mut self, b: &[u8]) -> &mut Self {
            self.out.extend_from_slice(b);
            self
        }
    }

    fn uid_bytes(uid: &str) -> Vec<u8> {
        let mut v = uid.as_bytes().to_vec();
        if v.len() % 2 == 1 {
            v.push(0);
        }
        v
    }

    /// Preamble + DICM + File Meta group (explicit VR LE) + `dataset`.
    fn part10(ts_uid: &str, dataset: &[u8]) -> Vec<u8> {
        let mut meta = B::new(TransferSyntax::ExplicitVrLittleEndian);
        meta.el((0x0002, 0x0001), b"OB", &[0x00, 0x01])
            .el(
                (0x0002, 0x0002),
                b"UI",
                &uid_bytes("1.2.840.10008.5.1.4.1.1.7"),
            )
            .el((0x0002, 0x0003), b"UI", &uid_bytes("1.2.3.4.5.6.7.8"))
            .el((0x0002, 0x0010), b"UI", &uid_bytes(ts_uid))
            .el((0x0002, 0x0012), b"UI", &uid_bytes("1.2.3.4"));
        let mut out = vec![0u8; PREAMBLE_LEN];
        out.extend_from_slice(PREFIX);
        let mut gl = B::new(TransferSyntax::ExplicitVrLittleEndian);
        gl.el(
            (0x0002, 0x0000),
            b"UL",
            &(meta.out.len() as u32).to_le_bytes(),
        );
        out.extend_from_slice(&gl.out);
        out.extend_from_slice(&meta.out);
        out.extend_from_slice(dataset);
        out
    }

    const SYNTAXES: [(&str, TransferSyntax); 3] = [
        (UID_IMPLICIT_VR_LE, TransferSyntax::ImplicitVrLittleEndian),
        (UID_EXPLICIT_VR_LE, TransferSyntax::ExplicitVrLittleEndian),
        (UID_EXPLICIT_VR_BE, TransferSyntax::ExplicitVrBigEndian),
    ];

    const ROWS: u16 = 96;
    const COLS: u16 = 96;

    fn sample_value(i: usize) -> u16 {
        ((i * 37 + 11) % 4096) as u16
    }

    /// A data set with an undefined-length SQ (one undefined-length item and
    /// one defined-length item), a defined-length SQ, an OB element, a
    /// pre-existing private block and 16-bit (12 stored) native pixel data.
    fn dataset(ts: TransferSyntax) -> Vec<u8> {
        let mut b = B::new(ts);
        b.el(
            (0x0008, 0x0016),
            b"UI",
            &uid_bytes("1.2.840.10008.5.1.4.1.1.7"),
        )
        .el((0x0008, 0x0018), b"UI", &uid_bytes("1.2.3.4.5.6.7.8"))
        .el((0x0008, 0x0060), b"CS", b"OT");

        // (0008,1140) Referenced Image Sequence, undefined length.
        let mut inner = B::new(ts);
        inner.el(
            (0x0008, 0x1150),
            b"UI",
            &uid_bytes("1.2.840.10008.5.1.4.1.1.7"),
        );
        b.header((0x0008, 0x1140), b"SQ", UNDEFINED_LENGTH)
            .item(ITEM, UNDEFINED_LENGTH)
            .raw(&inner.out.clone())
            .item(ITEM_DELIMITATION, 0);
        let mut inner2 = B::new(ts);
        inner2.el((0x0008, 0x1155), b"UI", &uid_bytes("1.2.3.4.5.6.7.9"));
        b.item(ITEM, inner2.out.len() as u32)
            .raw(&inner2.out)
            .item(SEQUENCE_DELIMITATION, 0);

        // (0008,9215) Derivation Code Sequence, defined length, one defined item.
        let mut seq = B::new(ts);
        seq.item(ITEM, inner.out.len() as u32).raw(&inner.out);
        b.header((0x0008, 0x9215), b"SQ", seq.out.len() as u32)
            .raw(&seq.out);

        b.el((0x0010, 0x0010), b"PN", b"DOE^JANE")
            .el((0x0010, 0x0020), b"LO", b"12345 ")
            // Vendor private block in group 0019 occupying slot 0x10.
            .el((0x0019, 0x0010), b"LO", b"ACME 1.0")
            .el((0x0019, 0x1001), b"OB", &[1, 2, 3, 4, 5, 6])
            .us(tags::SAMPLES_PER_PIXEL, 1)
            .el((0x0028, 0x0004), b"CS", b"MONOCHROME2 ")
            .us(tags::ROWS, ROWS)
            .us(tags::COLUMNS, COLS)
            .us(tags::BITS_ALLOCATED, 16)
            .us(tags::BITS_STORED, 12)
            .us(tags::HIGH_BIT, 11)
            .us(tags::PIXEL_REPRESENTATION, 0);
        let be = ts.big_endian();
        let px: Vec<u8> = (0..usize::from(ROWS) * usize::from(COLS))
            .flat_map(|i| {
                let v = sample_value(i);
                if be {
                    v.to_be_bytes()
                } else {
                    v.to_le_bytes()
                }
            })
            .collect();
        b.el(tags::PIXEL_DATA, b"OW", &px);
        b.out
    }

    fn sample_file(uid: &str, ts: TransferSyntax) -> Vec<u8> {
        part10(uid, &dataset(ts))
    }

    fn params(strategy: &str, password: Option<&str>) -> EmbedParams {
        let mut p = EmbedParams::new(password.map(|s| s.as_bytes().to_vec()).unwrap_or_default());
        if strategy == "metadata" {
            // The preamble holds at most 119 codec bytes: no RS parity.
            p.error_correction_level = ECLevel::None;
        }
        p.format_specific.insert(
            "dicom_strategy".to_string(),
            serde_json::Value::String(strategy.to_string()),
        );
        p
    }

    fn pixel_samples(file: &DicomFile) -> Vec<u16> {
        let px = file.get(tags::PIXEL_DATA).and_then(bytes_of).unwrap();
        px.as_chunks::<2>()
            .0
            .iter()
            .map(|&c| {
                if file.transfer_syntax.big_endian() {
                    u16::from_be_bytes(c)
                } else {
                    u16::from_le_bytes(c)
                }
            })
            .collect()
    }

    // -----------------------------------------------------------------------
    // Parsing / serialization
    // -----------------------------------------------------------------------

    #[test]
    fn unmodified_roundtrip_is_byte_identical() {
        for (uid, ts) in SYNTAXES {
            let bytes = sample_file(uid, ts);
            let file = DicomFile::parse(&bytes).unwrap();
            assert_eq!(file.transfer_syntax, ts, "{uid}");
            assert_eq!(file.to_bytes().unwrap(), bytes, "{uid}");

            // Structure was understood, not just copied.
            let sq = file.get((0x0008, 0x1140)).unwrap();
            let ElementValue::Sequence(seq) = &sq.value else {
                panic!("{uid}: undefined-length SQ not parsed as a sequence");
            };
            assert!(seq.undefined_length);
            assert_eq!(seq.items.len(), 2);
            assert!(seq.items[0].undefined_length);
            assert!(!seq.items[1].undefined_length);
            assert_eq!(seq.items[0].elements[0].tag, (0x0008, 0x1150));
            let ob = file.get((0x0019, 0x1001)).unwrap();
            assert_eq!(bytes_of(ob), Some(&[1u8, 2, 3, 4, 5, 6][..]));
            assert_eq!(ob.vr, ts.explicit_vr().then_some(*b"OB"));
            assert_eq!(file.read_us(tags::ROWS).unwrap(), Some(ROWS));
            assert_eq!(
                pixel_samples(&file)[..4],
                [
                    sample_value(0),
                    sample_value(1),
                    sample_value(2),
                    sample_value(3)
                ]
            );
        }
    }

    #[test]
    fn roundtrip_without_preamble_and_with_encapsulated_pixels() {
        // Bare File Meta + data set, no preamble.
        let bytes = sample_file(UID_EXPLICIT_VR_LE, TransferSyntax::ExplicitVrLittleEndian);
        let bare = bytes[PREAMBLE_LEN + 4..].to_vec();
        let file = DicomFile::parse(&bare).unwrap();
        assert!(file.preamble.is_none());
        assert_eq!(file.to_bytes().unwrap(), bare);
        assert!(DicomHandler::new().is_dicom(&bare));

        // JPEG Baseline: encapsulated pixel data (empty BOT + two fragments).
        let ts = TransferSyntax::ExplicitVrLittleEndian;
        let mut b = B::new(ts);
        b.us(tags::ROWS, 2)
            .us(tags::COLUMNS, 2)
            .us(tags::BITS_ALLOCATED, 8)
            .header(tags::PIXEL_DATA, b"OB", UNDEFINED_LENGTH)
            .item(ITEM, 0)
            .item(ITEM, 4)
            .raw(&[0xFF, 0xD8, 0xFF, 0xE0])
            .item(ITEM, 2)
            .raw(&[0xFF, 0xD9])
            .item(SEQUENCE_DELIMITATION, 0);
        let bytes = part10("1.2.840.10008.1.2.4.50", &b.out);
        let file = DicomFile::parse(&bytes).unwrap();
        let ElementValue::Encapsulated(enc) = &file.get(tags::PIXEL_DATA).unwrap().value else {
            panic!("expected encapsulated pixel data");
        };
        assert_eq!(enc.fragments.len(), 3);
        assert_eq!(file.to_bytes().unwrap(), bytes);

        // pixel_lsb refuses compressed pixel data; private still works.
        let h = DicomHandler::new();
        let err = h
            .embed_dicom(&bytes, b"x", &params("pixel_lsb", None))
            .unwrap_err();
        assert!(err.to_string().contains("encapsulated"), "{err}");
        let (out, _) = h
            .embed_dicom(&bytes, b"x", &params("private", None))
            .unwrap();
        assert_eq!(
            h.extract_dicom(&out, &params("private", None)).unwrap(),
            b"x"
        );
    }

    #[test]
    fn un_with_undefined_length_is_implicit_le_sequence() {
        let ts = TransferSyntax::ExplicitVrLittleEndian;
        let mut inner = B::new(TransferSyntax::ImplicitVrLittleEndian);
        inner.el((0x0011, 0x1001), b"LO", b"AB");
        let mut b = B::new(ts);
        b.el((0x0011, 0x0010), b"LO", b"VENDOR")
            .header((0x0011, 0x1010), b"UN", UNDEFINED_LENGTH)
            .item(ITEM, UNDEFINED_LENGTH)
            .raw(&inner.out)
            .item(ITEM_DELIMITATION, 0)
            .item(SEQUENCE_DELIMITATION, 0);
        let bytes = part10(UID_EXPLICIT_VR_LE, &b.out);
        let file = DicomFile::parse(&bytes).unwrap();
        let ElementValue::Sequence(seq) = &file.get((0x0011, 0x1010)).unwrap().value else {
            panic!("UN/undefined length must parse as a sequence");
        };
        assert_eq!(seq.item_syntax, TransferSyntax::ImplicitVrLittleEndian);
        assert_eq!(seq.items[0].elements[0].vr, None);
        assert_eq!(file.to_bytes().unwrap(), bytes);
    }

    #[test]
    fn explicit_vr_length_layout_matches_ps3_5() {
        // A short-VR element must not have reserved bytes, a long-VR one must.
        let file = DicomFile::parse(&sample_file(
            UID_EXPLICIT_VR_LE,
            TransferSyntax::ExplicitVrLittleEndian,
        ))
        .unwrap();
        let mut out = Vec::new();
        write_element(
            &mut out,
            file.get(tags::ROWS).unwrap(),
            file.transfer_syntax,
        )
        .unwrap();
        assert_eq!(
            out,
            [0x28, 0x00, 0x10, 0x00, b'U', b'S', 0x02, 0x00, ROWS as u8, 0]
        );
        let mut out = Vec::new();
        write_element(
            &mut out,
            file.get((0x0019, 0x1001)).unwrap(),
            file.transfer_syntax,
        )
        .unwrap();
        assert_eq!(
            out,
            [0x19, 0x00, 0x01, 0x10, b'O', b'B', 0, 0, 6, 0, 0, 0, 1, 2, 3, 4, 5, 6]
        );
        // The first meta element (0002,0000) UL is read at the right offset.
        assert_eq!(file.meta[0].tag, (0x0002, 0x0000));
        assert_eq!(file.meta[1].tag, (0x0002, 0x0001));
        assert_eq!(
            file.transfer_syntax_uid().as_deref(),
            Some(UID_EXPLICIT_VR_LE)
        );
    }

    #[test]
    fn malformed_input_is_an_error() {
        let ok = sample_file(UID_EXPLICIT_VR_LE, TransferSyntax::ExplicitVrLittleEndian);

        // Truncated inside the pixel data value / inside a header.
        assert!(DicomFile::parse(&ok[..ok.len() - 1]).is_err());
        assert!(DicomFile::parse(&ok[..ok.len() - 100]).is_err());
        assert!(DicomFile::parse(&ok[..PREAMBLE_LEN + 4 + 3]).is_err());
        assert!(DicomFile::parse(&[]).is_err());

        // Invalid VR.
        let mut b = B::new(TransferSyntax::ExplicitVrLittleEndian);
        b.raw(&[0x08, 0x00, 0x60, 0x00, b'Z', b'Z', 2, 0, b'O', b'T']);
        assert!(DicomFile::parse(&part10(UID_EXPLICIT_VR_LE, &b.out)).is_err());

        // Out-of-order tags.
        let mut b = B::new(TransferSyntax::ExplicitVrLittleEndian);
        b.el((0x0010, 0x0010), b"PN", b"AB")
            .el((0x0008, 0x0060), b"CS", b"OT");
        assert!(DicomFile::parse(&part10(UID_EXPLICIT_VR_LE, &b.out)).is_err());

        // Undefined-length sequence without its delimiter.
        let mut b = B::new(TransferSyntax::ExplicitVrLittleEndian);
        b.header((0x0008, 0x1140), b"SQ", UNDEFINED_LENGTH)
            .item(ITEM, 0);
        assert!(DicomFile::parse(&part10(UID_EXPLICIT_VR_LE, &b.out)).is_err());

        // Deflated transfer syntax is rejected rather than misparsed.
        assert!(DicomFile::parse(&part10(
            UID_DEFLATED_LE,
            &dataset(TransferSyntax::ExplicitVrLittleEndian)
        ))
        .is_err());

        // Every prefix: no panic, and a successful parse never drops bytes.
        let h = DicomHandler::new();
        for (uid, ts) in SYNTAXES {
            let bytes = sample_file(uid, ts);
            let step = if bytes.len() > 4096 { 7 } else { 1 };
            for n in (0..bytes.len().min(1200)).chain((1200..bytes.len()).step_by(step)) {
                let prefix = &bytes[..n];
                if let Ok(f) = DicomFile::parse(prefix) {
                    assert_eq!(f.to_bytes().unwrap(), prefix, "{uid} prefix {n}");
                }
                let _ = h.extract_dicom(prefix, &EmbedParams::default());
            }
        }

        // Pseudo-random garbage, with and without a valid prefix.
        let mut seed = 0x1234_5678_9abc_def0u64;
        for round in 0..300 {
            let len = 1 + (round * 13) % 700;
            let mut junk = Vec::with_capacity(len);
            for _ in 0..len {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                junk.push(seed as u8);
            }
            let mut with_prefix = ok[..PREAMBLE_LEN + 4].to_vec();
            with_prefix.extend_from_slice(&junk);
            for input in [&junk, &with_prefix] {
                if let Ok(f) = DicomFile::parse(input) {
                    assert_eq!(&f.to_bytes().unwrap(), input);
                }
                let _ = h.extract_dicom(input, &EmbedParams::default());
                let _ = h.embed_dicom(input, b"x", &params("pixel_lsb", None));
            }
        }
    }

    // -----------------------------------------------------------------------
    // Strategies
    // -----------------------------------------------------------------------

    const STRATEGIES: [&str; 5] = ["private", "metadata", "sequence", "pixel_lsb", "multi"];

    #[test]
    fn every_strategy_roundtrips_with_and_without_password() {
        let h = DicomHandler::new();
        let payload = b"PHI-free research payload \x00\x01\xFF";
        for (uid, ts) in SYNTAXES {
            let carrier = sample_file(uid, ts);
            for strategy in STRATEGIES {
                for password in [None, Some("correct horse")] {
                    let p = params(strategy, password);
                    let ctx = format!("{uid} {strategy} {password:?}");
                    let (out, res) = h
                        .embed_dicom(&carrier, payload, &p)
                        .unwrap_or_else(|e| panic!("{ctx}: embed: {e}"));
                    assert_eq!(res.bytes_embedded, payload.len(), "{ctx}");

                    // Output is a valid DICOM file in the same syntax.
                    let file = DicomFile::parse(&out).unwrap_or_else(|e| panic!("{ctx}: {e}"));
                    assert_eq!(file.transfer_syntax, ts, "{ctx}");
                    assert_eq!(file.to_bytes().unwrap(), out, "{ctx}");
                    assert!(h.verify_dicom_integrity(&out), "{ctx}");

                    // Explicit strategy and auto-detection both extract.
                    assert_eq!(h.extract_dicom(&out, &p).unwrap(), payload, "{ctx}");
                    let mut auto = p.clone();
                    auto.format_specific.clear();
                    assert_eq!(h.extract_dicom(&out, &auto).unwrap(), payload, "{ctx} auto");

                    // Password must match.
                    let wrong = match password {
                        Some(_) => params(strategy, Some("wrong")),
                        None => params(strategy, Some("unexpected")),
                    };
                    assert!(h.extract_dicom(&out, &wrong).is_err(), "{ctx} wrong pw");

                    // Honest integrity flag.
                    let pixel = matches!(strategy, "pixel_lsb" | "multi");
                    assert_eq!(res.medical_integrity_preserved, !pixel, "{ctx}");
                    if !pixel {
                        assert_eq!(
                            pixel_samples(&file),
                            pixel_samples(&DicomFile::parse(&carrier).unwrap()),
                            "{ctx}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn pixel_lsb_changes_each_sample_by_at_most_one() {
        let h = DicomHandler::new();
        let payload: Vec<u8> = (0..200u8).collect();
        for (uid, ts) in SYNTAXES {
            let carrier = sample_file(uid, ts);
            let (out, res) = h
                .embed_dicom(&carrier, &payload, &params("pixel_lsb", Some("pw")))
                .unwrap();
            assert!(res.pixels_modified > 0);
            assert!(!res.medical_integrity_preserved);
            assert_eq!(out.len(), carrier.len(), "{uid}");

            let before = DicomFile::parse(&carrier).unwrap();
            let after = DicomFile::parse(&out).unwrap();
            // Only Pixel Data differs.
            assert_eq!(before.meta, after.meta);
            for (a, b) in before.dataset.iter().zip(&after.dataset) {
                if a.tag != tags::PIXEL_DATA {
                    assert_eq!(a, b, "{uid}");
                }
            }
            let (sa, sb) = (pixel_samples(&before), pixel_samples(&after));
            let mut changed = 0;
            for (x, y) in sa.iter().zip(&sb) {
                let d = (i32::from(*x) - i32::from(*y)).abs();
                assert!(d <= 1, "{uid}: sample changed by {d}");
                // Stays within the 12 stored bits.
                assert!(*y < 4096);
                changed += usize::from(d == 1);
            }
            assert_eq!(changed, res.pixels_modified, "{uid}");
            assert_eq!(
                h.extract_dicom(&out, &params("pixel_lsb", Some("pw")))
                    .unwrap(),
                payload
            );
        }
    }

    #[test]
    fn pixel_lsb_respects_high_bit_and_8_bit() {
        // 8-bit samples.
        let ts = TransferSyntax::ExplicitVrLittleEndian;
        let mut b = B::new(ts);
        b.us(tags::ROWS, 64)
            .us(tags::COLUMNS, 64)
            .us(tags::BITS_ALLOCATED, 8)
            .us(tags::BITS_STORED, 8)
            .us(tags::HIGH_BIT, 7)
            .el(tags::PIXEL_DATA, b"OB", &[0x80u8; 64 * 64]);
        let carrier = part10(UID_EXPLICIT_VR_LE, &b.out);
        let h = DicomHandler::new();
        let p = params("pixel_lsb", None);
        // Capacity errors are in BITS: one bit per sample (64×64 = 4096),
        // frame = 9 + 36 + 1000 bytes = 8360 bits.
        let mut p_none = p.clone();
        p_none.error_correction_level = ECLevel::None;
        assert!(matches!(
            h.embed_dicom(&carrier, &[7u8; 1000], &p_none),
            Err(SteganoError::InsufficientCapacity {
                needed: 8360,
                available: 4096
            })
        ));
        let (out, _) = h.embed_dicom(&carrier, b"eight", &p).unwrap();
        let px = DicomFile::parse(&out).unwrap();
        let px = bytes_of(px.get(tags::PIXEL_DATA).unwrap())
            .unwrap()
            .to_vec();
        assert!(px.iter().all(|&v| v == 0x80 || v == 0x81));
        assert_eq!(h.extract_dicom(&out, &p).unwrap(), b"eight");

        // Bits Stored 12, High Bit 15: stored LSB is bit 4 of the 16-bit word.
        let mut b = B::new(ts);
        b.us(tags::ROWS, 64)
            .us(tags::COLUMNS, 64)
            .us(tags::BITS_ALLOCATED, 16)
            .us(tags::BITS_STORED, 12)
            .us(tags::HIGH_BIT, 15)
            .el(tags::PIXEL_DATA, b"OW", &[0u8; 64 * 64 * 2]);
        let carrier = part10(UID_EXPLICIT_VR_LE, &b.out);
        let (out, _) = h.embed_dicom(&carrier, b"high", &p).unwrap();
        let f = DicomFile::parse(&out).unwrap();
        assert!(pixel_samples(&f).iter().all(|&v| v == 0 || v == 0x10));
        assert_eq!(h.extract_dicom(&out, &p).unwrap(), b"high");

        // Unsupported layouts are rejected.
        let mut b = B::new(ts);
        b.us(tags::BITS_ALLOCATED, 1)
            .el(tags::PIXEL_DATA, b"OB", &[0u8; 512]);
        let carrier = part10(UID_EXPLICIT_VR_LE, &b.out);
        assert!(h.embed_dicom(&carrier, b"x", &p).is_err());
    }

    #[test]
    fn private_elements_are_in_tag_order_with_creator() {
        let h = DicomHandler::new();
        for (uid, ts) in SYNTAXES {
            let carrier = sample_file(uid, ts);
            let mut p = params("private", None);
            // Group 0019 already holds the ACME block in slot 0x10.
            p.format_specific
                .insert("dicom_private_group".to_string(), 0x0019.into());
            let (out, res) = h.embed_dicom(&carrier, b"hello", &p).unwrap();
            assert!(res.medical_integrity_preserved);
            // parse() rejects out-of-order tags, so this checks ordering too.
            let file = DicomFile::parse(&out).unwrap();
            let creator = file
                .get((0x0019, 0x0011))
                .expect("creator in next free slot");
            assert_eq!(bytes_of(creator), Some(&b"STEGANO "[..]));
            assert_eq!(creator.vr, ts.explicit_vr().then_some(*b"LO"));
            let data = file
                .get((0x0019, 0x1110))
                .expect("data element in block 0x11");
            assert_eq!(data.vr, ts.explicit_vr().then_some(*b"OB"));
            assert_eq!(bytes_of(data).unwrap().len() % 2, 0);
            // Vendor block untouched; pixel data still last.
            assert_eq!(
                bytes_of(file.get((0x0019, 0x0010)).unwrap()),
                Some(&b"ACME 1.0"[..])
            );
            assert_eq!(file.dataset.last().unwrap().tag, tags::PIXEL_DATA);
            assert!(file.dataset.windows(2).all(|w| w[0].tag < w[1].tag));
            assert_eq!(h.extract_dicom(&out, &p).unwrap(), b"hello");

            // Re-embedding replaces the block rather than adding another.
            let (out2, _) = h.embed_dicom(&out, b"second", &p).unwrap();
            let f2 = DicomFile::parse(&out2).unwrap();
            assert_eq!(
                f2.dataset.iter().filter(|e| is_stegano_creator(e)).count(),
                1
            );
            assert_eq!(h.extract_dicom(&out2, &p).unwrap(), b"second");
        }

        // Default group 0009 with a payload far beyond the old 16-bit limit.
        let carrier = sample_file(UID_EXPLICIT_VR_LE, TransferSyntax::ExplicitVrLittleEndian);
        let mut p = params("private", None);
        p.error_correction_level = ECLevel::None;
        let big: Vec<u8> = (0..200_000u32).map(|i| (i * 31) as u8).collect();
        let (out, _) = h.embed_dicom(&carrier, &big, &p).unwrap();
        let file = DicomFile::parse(&out).unwrap();
        assert!(file.get((0x0009, 0x0010)).is_some());
        assert!(file.get((0x0009, 0x1010)).is_some());
        assert_eq!(h.extract_dicom(&out, &p).unwrap(), big);

        // Even groups are not private.
        let mut p = params("private", None);
        p.format_specific
            .insert("dicom_private_group".to_string(), 0x0010.into());
        assert!(h.embed_dicom(&carrier, b"x", &p).is_err());
    }

    #[test]
    fn padding_is_last_and_preamble_is_guarded() {
        let h = DicomHandler::new();
        let carrier = sample_file(UID_EXPLICIT_VR_LE, TransferSyntax::ExplicitVrLittleEndian);
        let (out, _) = h
            .embed_dicom(&carrier, b"pad", &params("sequence", None))
            .unwrap();
        let f = DicomFile::parse(&out).unwrap();
        assert_eq!(f.dataset.last().unwrap().tag, tags::TRAILING_PADDING);
        // Unchanged prefix: the padding element is appended.
        assert_eq!(&out[..carrier.len()], &carrier[..]);

        // Preamble: too-large payload and a foreign preamble are refused.
        // InsufficientCapacity is reported in BITS: frame = 9 + codec
        // (36-byte header + 200 plaintext bytes) = 245 bytes = 1960 bits,
        // preamble = 128 bytes = 1024 bits.
        let p = params("metadata", None);
        assert!(matches!(
            h.embed_dicom(&carrier, &[0u8; 200], &p),
            Err(SteganoError::InsufficientCapacity {
                needed: 1960,
                available: 1024
            })
        ));
        let mut foreign = carrier.clone();
        foreign[..4].copy_from_slice(b"TIFF");
        assert!(h.embed_dicom(&foreign, b"x", &p).is_err());
        let mut p2 = p.clone();
        p2.format_specific
            .insert("dicom_overwrite_preamble".to_string(), true.into());
        let (out, _) = h.embed_dicom(&foreign, b"x", &p2).unwrap();
        assert_eq!(h.extract_dicom(&out, &p2).unwrap(), b"x");
        // Bare files have no preamble.
        assert!(h
            .embed_dicom(&carrier[PREAMBLE_LEN + 4..], b"x", &p)
            .is_err());
    }

    #[test]
    fn unknown_strategy_and_missing_data_are_errors() {
        let h = DicomHandler::new();
        let carrier = sample_file(UID_EXPLICIT_VR_LE, TransferSyntax::ExplicitVrLittleEndian);
        assert!(h
            .embed_dicom(&carrier, b"x", &params("bogus", None))
            .is_err());
        assert!(h.extract_dicom(&carrier, &EmbedParams::default()).is_err());
        for s in STRATEGIES {
            assert!(h.extract_dicom(&carrier, &params(s, None)).is_err(), "{s}");
        }
    }

    #[tokio::test]
    async fn data_handler_trait_roundtrip() {
        let h = DicomHandler::new();
        let raw = sample_file(UID_IMPLICIT_VR_LE, TransferSyntax::ImplicitVrLittleEndian);
        assert_eq!(h.detect_format(&raw), Some(DataFormat::DICOM));
        let mut carrier = CarrierData::new(DataFormat::DICOM, raw.clone());
        let cap = h.estimate_capacity(&carrier).unwrap();
        assert!(cap.total_bits > 0 && cap.recommended_bits <= cap.total_bits);
        assert!(h.verify_integrity(&carrier).unwrap());
        let p = EmbedParams::new(b"key".to_vec());
        h.embed(&mut carrier, b"trait payload", &p).await.unwrap();
        assert_ne!(carrier.raw_data, raw);
        assert_eq!(h.extract(&carrier, &p).await.unwrap(), b"trait payload");
    }
}
