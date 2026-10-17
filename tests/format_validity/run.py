#!/usr/bin/env python3
"""End-to-end format-validity suite for the ``stegano`` CLI (issue #49).

For every carrier format the CLI supports, this script

1. generates a realistic carrier with a standard tool/library,
2. checks the *original* carrier passes the external validator (baseline),
3. for each applicable ``--strategy``: embeds a random payload with
   ``--password``, extracts it, checks the bytes match exactly, and validates
   the stego output with an independent external tool,
4. runs the documented expected-error cases and checks they fail *cleanly*
   (non-zero exit, no panic, no output file),

then prints a results table and exits non-zero on any failure.

Usage::

    python3 tests/format_validity/run.py target/release/stegano
    python3 tests/format_validity/run.py target/release/stegano --only wav --only fits

External tools: Pillow, numpy, soundfile, astropy, Biopython, pydicom, pysam
(see requirements.txt), Python's ``csv`` module (CSV) and UTF-8 line comparison
(plain text), plus the ``samtools`` and ``bcftools`` executables.
``bcftools`` is optional when pysam is available; ``samtools`` is required for
the SAM/BAM carriers.
"""

from __future__ import annotations

import argparse
import hashlib
import io
import os
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import time
import wave
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable, Optional

import numpy as np

PASSWORD = "format-validity-pw"
SEED = 0x5713_0049

# --------------------------------------------------------------------------
# Helpers
# --------------------------------------------------------------------------


def rng() -> np.random.Generator:
    return np.random.default_rng(SEED)


def run(cmd: list[str], timeout: int = 600, **kw) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, timeout=timeout, **kw)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def first_line(b: bytes) -> str:
    s = b.decode("utf-8", "replace").strip()
    return s.splitlines()[0] if s else ""


def tail(b: bytes, n: int = 3) -> str:
    lines = [
        ln
        for ln in b.decode("utf-8", "replace").splitlines()
        if ln.strip() and "--password is visible" not in ln
    ]
    return " | ".join(lines[-n:])


class Problems(list):
    """List of validation problems (empty == valid)."""

    def check(self, cond: bool, msg: str) -> None:
        if not cond:
            self.append(msg)


Validator = Callable[[Path, Path, Optional[str]], Problems]


@dataclass
class Carrier:
    name: str  # unique id shown in the table, e.g. "png-rgb8"
    family: str  # value accepted by --only, e.g. "png"
    filename: str
    generate: Callable[[Path], None]
    validate: Validator
    # Strategies to round-trip. ``None`` means "no --strategy flag" (default).
    strategies: list[Optional[str]]
    # strategy -> reason. These must fail cleanly at embed time.
    expected_errors: dict[str, str] = field(default_factory=dict)
    payload_len: int = 64
    payload_len_for: dict[str, int] = field(default_factory=dict)


@dataclass
class Result:
    carrier: str
    strategy: str
    status: str  # PASS / FAIL / XFAIL-OK (expected error, clean) / SKIP
    detail: str = ""
    seconds: float = 0.0


# --------------------------------------------------------------------------
# Imagery (Pillow)
# --------------------------------------------------------------------------


def _natural_rgb8(h: int = 256, w: int = 256) -> np.ndarray:
    r = rng()
    y, x = np.mgrid[0:h, 0:w].astype(np.float64)
    base = np.stack(
        [
            128 + 90 * np.sin(x / 23.0) * np.cos(y / 31.0),
            128 + 80 * np.cos((x + y) / 40.0),
            128 + 70 * np.sin(np.hypot(x - w / 2, y - h / 2) / 15.0),
        ],
        axis=-1,
    )
    # some hard edges (rectangles) and sensor-like noise
    base[40:90, 30:120] = [220, 40, 40]
    base[150:220, 140:230] = [30, 30, 200]
    base += r.normal(0, 6, base.shape)
    return np.clip(base, 0, 255).astype(np.uint8)


def _natural_gray16(h: int = 256, w: int = 256) -> np.ndarray:
    r = rng()
    y, x = np.mgrid[0:h, 0:w].astype(np.float64)
    # Microscopy-like: smooth background, many sharp-edged bright "cells",
    # shot noise.  (EdgeBased needs real edges; a smooth gradient has ~none.)
    img = 12000 + 6000 * np.sin(x / 19.0) * np.cos(y / 27.0)
    for _ in range(60):
        cx, cy, rad = r.uniform(0, w), r.uniform(0, h), r.uniform(4, 14)
        img[(x - cx) ** 2 + (y - cy) ** 2 < rad**2] += r.uniform(15000, 40000)
    img += r.normal(0, 1500, img.shape)
    return np.clip(img, 0, 65535).astype(np.uint16)


def gen_png_rgb8(p: Path) -> None:
    from PIL import Image

    Image.fromarray(_natural_rgb8(), "RGB").save(p, format="PNG")


def gen_png_gray16(p: Path) -> None:
    from PIL import Image

    arr = _natural_gray16()
    Image.fromarray(arr).save(p, format="PNG")  # Pillow writes 16-bit grayscale


def gen_tiff_rgb8(p: Path) -> None:
    from PIL import Image

    Image.fromarray(_natural_rgb8(), "RGB").save(p, format="TIFF")


def validate_image(orig: Path, out: Path, strategy: Optional[str]) -> Problems:
    from PIL import Image

    pr = Problems()
    with Image.open(orig) as a:
        a.load()
        a_fmt, a_size, a_mode = a.format, a.size, a.mode
        a_px = np.array(a)
    try:
        with Image.open(out) as b:
            b.load()
            b_fmt, b_size, b_mode = b.format, b.size, b.mode
            b_px = np.array(b)
    except Exception as e:  # noqa: BLE001
        pr.append(f"Pillow failed to load output: {type(e).__name__}: {e}")
        return pr
    pr.check(b_fmt == a_fmt, f"format changed {a_fmt} -> {b_fmt}")
    pr.check(b_size == a_size, f"size changed {a_size} -> {b_size}")
    pr.check(b_mode == a_mode, f"mode changed {a_mode} -> {b_mode}")
    pr.check(b_px.shape == a_px.shape, f"pixel array shape {a_px.shape} -> {b_px.shape}")
    pr.check(b_px.dtype == a_px.dtype, f"pixel dtype {a_px.dtype} -> {b_px.dtype}")
    return pr


IMAGE_STRATEGIES = [None, "lsb", "adaptive", "edge", "dct"]

# --------------------------------------------------------------------------
# WAV (wave / soundfile)
# --------------------------------------------------------------------------

WAV_RATE = 44100
WAV_SECONDS = 30  # echo hiding carries 1 bit per 1024 frames per channel


def _audio(channels: int) -> np.ndarray:
    """Music-like test signal in [-1, 1]: chords + vibrato + noise + envelope."""
    r = rng()
    n = WAV_RATE * WAV_SECONDS
    t = np.arange(n) / WAV_RATE
    out = np.zeros((n, channels))
    for ch in range(channels):
        sig = np.zeros(n)
        for k, f in enumerate([220.0, 277.18, 329.63, 440.0, 1318.5, 3520.0]):
            vib = 1 + 0.003 * np.sin(2 * np.pi * (5 + ch) * t)
            sig += (0.25 / (k + 1)) * np.sin(2 * np.pi * f * vib * t + ch)
        sig *= 0.6 + 0.4 * np.abs(np.sin(2 * np.pi * 0.5 * t))
        sig += r.normal(0, 0.01, n)
        out[:, ch] = sig
    return np.clip(out / np.max(np.abs(out)) * 0.8, -1, 1)


def _gen_wav16(p: Path, channels: int) -> None:
    pcm = (_audio(channels) * 32767).astype("<i2")
    with wave.open(str(p), "wb") as w:
        w.setnchannels(channels)
        w.setsampwidth(2)
        w.setframerate(WAV_RATE)
        w.writeframes(pcm.tobytes())


def gen_wav16_mono(p: Path) -> None:
    _gen_wav16(p, 1)


def gen_wav16_stereo(p: Path) -> None:
    _gen_wav16(p, 2)


def gen_wav24_stereo(p: Path) -> None:
    import soundfile as sf

    sf.write(str(p), _audio(2), WAV_RATE, subtype="PCM_24", format="WAV")


def validate_wav(orig: Path, out: Path, strategy: Optional[str]) -> Problems:
    import soundfile as sf

    pr = Problems()

    def wave_params(path: Path):
        with wave.open(str(path), "rb") as w:
            prm = w.getparams()
            data = w.readframes(prm.nframes)
        return prm, len(data)

    a, a_len = wave_params(orig)
    try:
        b, b_len = wave_params(out)
    except Exception as e:  # noqa: BLE001
        pr.append(f"wave failed to read output: {type(e).__name__}: {e}")
    else:
        pr.check(b.nframes == a.nframes, f"wave nframes {a.nframes} -> {b.nframes}")
        pr.check(b.nchannels == a.nchannels, f"channels {a.nchannels} -> {b.nchannels}")
        pr.check(b.sampwidth == a.sampwidth, f"sampwidth {a.sampwidth} -> {b.sampwidth}")
        pr.check(b.framerate == a.framerate, f"rate {a.framerate} -> {b.framerate}")
        pr.check(b_len == a_len, f"readframes length {a_len} -> {b_len}")
    try:
        ia, ib = sf.info(str(orig)), sf.info(str(out))
        da, _ = sf.read(str(orig), dtype="int32")
        db, _ = sf.read(str(out), dtype="int32")
    except Exception as e:  # noqa: BLE001
        pr.append(f"soundfile failed to read output: {type(e).__name__}: {e}")
        return pr
    pr.check(ib.frames == ia.frames, f"soundfile frames {ia.frames} -> {ib.frames}")
    pr.check(ib.subtype == ia.subtype, f"subtype {ia.subtype} -> {ib.subtype}")
    pr.check(db.shape == da.shape, f"sample array shape {da.shape} -> {db.shape}")
    return pr


WAV_STRATEGIES = [None, "timedomain", "frequency", "phase", "spread", "echo"]

# --------------------------------------------------------------------------
# FITS (astropy)
# --------------------------------------------------------------------------


def gen_fits(p: Path) -> None:
    from astropy.io import fits

    r = rng()
    y, x = np.mgrid[0:128, 0:160]
    sky = 1200 + 40 * np.sin(x / 9.0) + r.normal(0, 25, (128, 160))
    for cx, cy, amp in [(40, 30, 9000), (100, 90, 15000), (140, 20, 4000)]:
        sky += amp * np.exp(-((x - cx) ** 2 + (y - cy) ** 2) / 8.0)
    prim = fits.PrimaryHDU(np.clip(sky, -32768, 32767).astype(np.int16))
    prim.header["OBJECT"] = ("FV-TEST", "synthetic star field")
    prim.header["EXPTIME"] = (30.0, "s")
    prim.header["TELESCOP"] = "stegano-ci"
    ext = fits.ImageHDU(r.integers(-500, 500, (64, 64), dtype=np.int16), name="ERR")
    ext.header["BUNIT"] = "adu"
    tab = fits.BinTableHDU.from_columns(
        [
            fits.Column(name="ID", format="J", array=np.arange(50, dtype=np.int32)),
            fits.Column(name="RA", format="D", array=r.uniform(0, 360, 50)),
            fits.Column(name="MAG", format="E", array=r.uniform(10, 20, 50).astype(np.float32)),
        ],
        name="CATALOG",
    )
    fits.HDUList([prim, ext, tab]).writeto(p, overwrite=True)


def validate_fits(orig: Path, out: Path, strategy: Optional[str]) -> Problems:
    from astropy.io import fits

    pr = Problems()
    try:
        hb = fits.open(out, memmap=False)
        hb.verify("exception")
    except Exception as e:  # noqa: BLE001
        pr.append(f"astropy verify('exception') failed: {type(e).__name__}: {e}")
        return pr
    with fits.open(orig, memmap=False) as ha, hb:
        pr.check(len(hb) == len(ha), f"HDU count {len(ha)} -> {len(hb)}")
        for i, (a, b) in enumerate(zip(ha, hb)):
            pr.check(
                a.header.tostring() == b.header.tostring(), f"HDU {i} ({a.name}) header changed"
            )
            if a.data is None:
                pr.check(b.data is None, f"HDU {i} gained data")
                continue
            if b.data is None:
                pr.append(f"HDU {i} lost its data")
                continue
            if i == 0:  # the stego image HDU: shape/dtype must be preserved
                pr.check(a.data.shape == b.data.shape, f"HDU 0 shape {a.data.shape} -> {b.data.shape}")
                pr.check(a.data.dtype == b.data.dtype, f"HDU 0 dtype {a.data.dtype} -> {b.data.dtype}")
            elif isinstance(a, fits.BinTableHDU):
                pr.check(
                    a.data.tobytes() == b.data.tobytes(), f"HDU {i} ({a.name}) table data changed"
                )
            else:
                pr.check(
                    np.array_equal(a.data, b.data), f"HDU {i} ({a.name}) image data changed"
                )
    pr.check(out.stat().st_size == orig.stat().st_size,
             f"file size {orig.stat().st_size} -> {out.stat().st_size}")
    return pr


# --------------------------------------------------------------------------
# FASTQ / FASTA (Biopython)
# --------------------------------------------------------------------------

BASES = np.array(list("ACGT"))
# Illumina 1.8+ binned qualities (Phred+33); '#' (Q2) is very common in tails.
QBINS = np.array(list("#,:F"))


def _fastq_text(eol: str) -> str:
    r = rng()
    recs = []
    for i in range(400):
        n = 151
        seq = "".join(r.choice(BASES, n))
        q = r.choice(QBINS, n, p=[0.15, 0.05, 0.15, 0.65])
        if i % 3 == 0:  # quality-trimmed tail of Q2 (very common in real data)
            q[-int(r.integers(20, 80)) :] = "#"
        if i % 50 == 7:  # a read of mostly '#' with N calls
            q[:] = "#"
            seq = "N" * n
        hdr = f"@A00123:8:H5KJ2DSXY:1:1101:{1000 + i * 7}:{2000 + i * 3} 1:N:0:ACGTACGT+TTGCAGAC"
        recs.append(eol.join([hdr, seq, "+", "".join(q)]))
    return eol.join(recs) + eol


def gen_fastq_lf(p: Path) -> None:
    p.write_bytes(_fastq_text("\n").encode())


def gen_fastq_crlf(p: Path) -> None:
    p.write_bytes(_fastq_text("\r\n").encode())


def gen_fasta(p: Path) -> None:
    r = rng()
    lines = []
    for i in range(24):
        seq = "".join(r.choice(BASES, int(r.integers(300, 2500))))
        lines.append(f">contig_{i:03d} len={len(seq)} organism=Synthetica testii")
        lines += [seq[j : j + 60] for j in range(0, len(seq), 60)]
    p.write_text("\n".join(lines) + "\n")


def _parse_seqio(path: Path, fmt: str):
    from Bio import SeqIO

    with open(path, newline="") as fh:
        text = fh.read()
    # Biopython parses universal newlines; feed it text read without translation
    # replaced to LF so CRLF files are parsed the same way a typical tool would.
    return list(SeqIO.parse(io.StringIO(text.replace("\r\n", "\n")), fmt))


def _validate_seqio(orig: Path, out: Path, fmt: str) -> Problems:
    pr = Problems()
    a = _parse_seqio(orig, fmt)
    try:
        b = _parse_seqio(out, fmt)
    except Exception as e:  # noqa: BLE001
        pr.append(f"Biopython SeqIO.parse({fmt}) failed: {type(e).__name__}: {e}")
        return pr
    pr.check(len(b) == len(a), f"record count {len(a)} -> {len(b)}")
    seq_diff = sum(1 for x, y in zip(a, b) if str(x.seq) != str(y.seq))
    id_diff = sum(1 for x, y in zip(a, b) if x.id != y.id)
    pr.check(seq_diff == 0, f"{seq_diff} sequences changed")
    pr.check(id_diff == 0, f"{id_diff} record ids changed")
    if fmt == "fastq":
        bad = sum(
            1
            for y in b
            if len(y.letter_annotations["phred_quality"]) != len(y.seq)
        )
        pr.check(bad == 0, f"{bad} records with quality length != sequence length")
    return pr


def validate_fastq(orig: Path, out: Path, strategy: Optional[str]) -> Problems:
    pr = _validate_seqio(orig, out, "fastq")
    a, b = orig.read_bytes(), out.read_bytes()
    pr.check(a.count(b"\r\n") == b.count(b"\r\n"), "CRLF line terminators not preserved")
    na, nb = a.count(b"\n"), b.count(b"\n")
    pr.check(na == nb, f"line count {na} -> {nb}")
    return pr


def validate_fasta(orig: Path, out: Path, strategy: Optional[str]) -> Problems:
    return _validate_seqio(orig, out, "fasta")


# --------------------------------------------------------------------------
# VCF (bcftools and/or pysam)
# --------------------------------------------------------------------------

VCF_HEADER = """##fileformat=VCFv4.2
##fileDate=20261016
##source=stegano-format-validity
##reference=GRCh38
##contig=<ID=chr1,length=248956422>
##contig=<ID=chr2,length=242193529>
##INFO=<ID=DP,Number=1,Type=Integer,Description="Total read depth">
##INFO=<ID=AF,Number=A,Type=Float,Description="Allele frequency">
##INFO=<ID=MQ,Number=1,Type=Float,Description="RMS mapping quality">
##INFO=<ID=DB,Number=0,Type=Flag,Description="dbSNP membership">
##FILTER=<ID=LowQual,Description="QUAL below 30">
##FORMAT=<ID=GT,Number=1,Type=String,Description="Genotype">
##FORMAT=<ID=GQ,Number=1,Type=Integer,Description="Genotype quality">
##FORMAT=<ID=DP,Number=1,Type=Integer,Description="Read depth">
##FORMAT=<ID=AD,Number=R,Type=Integer,Description="Allelic depths">
#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\tNA12878\tNA12891\tNA12892
"""


def gen_vcf(p: Path) -> None:
    r = rng()
    rows = []
    for chrom in ("chr1", "chr2"):
        pos = 10000
        for _ in range(250):
            pos += int(r.integers(50, 5000))
            ref = str(r.choice(BASES))
            alt = str(r.choice([b for b in "ACGT" if b != ref]))
            qual = float(r.uniform(5, 3000))
            flt = "LowQual" if qual < 30 else "PASS"
            vid = f"rs{int(r.integers(1, 10**9))}" if r.random() < 0.6 else "."
            dp = int(r.integers(10, 200))
            info = f"DP={dp};AF={r.uniform(0.01, 1):.3f};MQ={r.uniform(20, 60):.2f}"
            if vid != ".":
                info += ";DB"
            samples = []
            for _s in range(3):
                gt = str(r.choice(["0/0", "0/1", "1/1", "0|1", "./."]))
                sdp = int(r.integers(5, 80))
                ad_alt = int(r.integers(0, sdp))
                samples.append(f"{gt}:{int(r.integers(1, 99))}:{sdp}:{sdp - ad_alt},{ad_alt}")
            rows.append(
                "\t".join(
                    [chrom, str(pos), vid, ref, alt, f"{qual:.2f}", flt, info, "GT:GQ:DP:AD", *samples]
                )
            )
    p.write_text(VCF_HEADER + "\n".join(rows) + "\n")


def _bcftools_warnings(stderr: bytes) -> set[str]:
    return {ln for ln in stderr.decode("utf-8", "replace").splitlines() if ln.strip()}


def validate_vcf(orig: Path, out: Path, strategy: Optional[str]) -> Problems:
    pr = Problems()
    bcftools = shutil.which("bcftools")
    if bcftools:
        ra = run([bcftools, "view", "-Ov", "-o", os.devnull, str(orig)])
        rb = run([bcftools, "view", "-Ov", "-o", os.devnull, str(out)])
        if rb.returncode != 0:
            pr.append(f"bcftools view exit {rb.returncode}: {tail(rb.stderr)}")
            return pr
        new_warn = _bcftools_warnings(rb.stderr) - _bcftools_warnings(ra.stderr)
        pr.check(not new_warn, f"bcftools new warnings: {' | '.join(sorted(new_warn)[:3])}")
        q = ["query", "-f", "%CHROM\t%POS\t%REF\t%ALT[\t%GT]\n"]
        qa, qb = run([bcftools, *q, str(orig)]), run([bcftools, *q, str(out)])
        pr.check(qb.returncode == 0, f"bcftools query exit {qb.returncode}: {tail(qb.stderr)}")
        pr.check(qa.stdout == qb.stdout, "CHROM/POS/REF/ALT/GT changed (bcftools query)")
        sa, sb = run([bcftools, "query", "-l", str(orig)]), run([bcftools, "query", "-l", str(out)])
        pr.check(sa.stdout == sb.stdout, "sample names changed")
    try:
        import pysam
    except ImportError:
        if not bcftools:
            pr.append("neither bcftools nor pysam available")
        return pr
    try:
        def recs(path: Path):
            with pysam.VariantFile(str(path)) as vf:
                samples = list(vf.header.samples)
                rows = [(r.chrom, r.pos, r.ref, r.alts) for r in vf]
            return samples, rows

        a_s, a_r = recs(orig)
        b_s, b_r = recs(out)
    except Exception as e:  # noqa: BLE001
        pr.append(f"pysam.VariantFile failed: {type(e).__name__}: {e}")
        return pr
    pr.check(a_s == b_s, "pysam: sample names changed")
    pr.check(len(a_r) == len(b_r), f"pysam: record count {len(a_r)} -> {len(b_r)}")
    pr.check(a_r == b_r, "pysam: CHROM/POS/REF/ALT changed")
    return pr


# --------------------------------------------------------------------------
# SAM / BAM (samtools)
# --------------------------------------------------------------------------


def _sam_text() -> str:
    r = rng()
    hdr = [
        "@HD\tVN:1.6\tSO:coordinate",
        "@SQ\tSN:chr1\tLN:200000",
        "@SQ\tSN:chr2\tLN:150000",
        "@RG\tID:rg1\tSM:NA12878\tPL:ILLUMINA\tLB:lib1",
        "@PG\tID:bwa\tPN:bwa\tVN:0.7.17\tCL:bwa mem ref.fa r1.fq r2.fq",
    ]
    reads = []
    for chrom, ln in (("chr1", 200000), ("chr2", 150000)):
        positions = np.sort(r.integers(1, ln - 200, 200))
        for k, pos in enumerate(positions):
            seq = "".join(r.choice(BASES, 100))
            qual = r.choice(QBINS, 100, p=[0.1, 0.05, 0.15, 0.7])
            if k % 4 == 0:
                qual[-30:] = "#"
            flag = int(r.choice([99, 147, 83, 163, 0, 16]))
            nm = int(r.integers(0, 4))
            reads.append(
                "\t".join(
                    [
                        f"read_{chrom}_{k:04d}", str(flag), chrom, str(int(pos)),
                        str(int(r.integers(0, 61))), "100M", "=" if flag not in (0, 16) else "*",
                        str(int(pos) + 200) if flag not in (0, 16) else "0",
                        "300" if flag not in (0, 16) else "0",
                        seq, "".join(qual),
                        f"NM:i:{nm}", "MD:Z:100", f"AS:i:{100 - nm * 5}", "RG:Z:rg1",
                    ]
                )
            )
    for k in range(20):  # unmapped reads at the end
        seq = "".join(r.choice(BASES, 100))
        reads.append("\t".join([f"unmapped_{k:03d}", "4", "*", "0", "0", "*", "*", "0", "0",
                                seq, "".join(r.choice(QBINS, 100)), "RG:Z:rg1"]))
    return "\n".join(hdr + reads) + "\n"


def gen_sam(p: Path) -> None:
    p.write_text(_sam_text())


def gen_bam(p: Path) -> None:
    sam = p.with_suffix(".src.sam")
    sam.write_text(_sam_text())
    res = run([samtools_bin(), "view", "-b", "-o", str(p), str(sam)])
    sam.unlink()
    if res.returncode != 0:
        raise RuntimeError(f"samtools view -b failed: {tail(res.stderr)}")


def samtools_bin() -> str:
    s = shutil.which("samtools")
    if not s:
        raise RuntimeError("samtools not found on PATH")
    return s


def validate_sam(orig: Path, out: Path, strategy: Optional[str]) -> Problems:
    pr = Problems()
    st = samtools_bin()
    qc = run([st, "quickcheck", "-v", str(out)])
    pr.check(qc.returncode == 0, f"samtools quickcheck exit {qc.returncode}: {tail(qc.stdout + qc.stderr)}")
    ca, cb = run([st, "view", "-c", str(orig)]), run([st, "view", "-c", str(out)])
    if cb.returncode != 0:
        pr.append(f"samtools view -c exit {cb.returncode}: {tail(cb.stderr)}")
        return pr
    pr.check(ca.stdout.strip() == cb.stdout.strip(),
             f"read count {ca.stdout.strip().decode()} -> {cb.stdout.strip().decode()}")
    va, vb = run([st, "view", str(orig)]), run([st, "view", str(out)])
    pr.check(vb.returncode == 0, f"samtools view exit {vb.returncode}: {tail(vb.stderr)}")
    new_err = set(vb.stderr.decode("utf-8", "replace").splitlines()) - set(
        va.stderr.decode("utf-8", "replace").splitlines())
    pr.check(not new_err, f"samtools view new stderr: {' | '.join(sorted(new_err)[:3])}")

    def core(text: bytes):
        # QNAME FLAG RNAME POS MAPQ CIGAR RNEXT PNEXT TLEN SEQ
        return [ln.split(b"\t")[:10] for ln in text.splitlines()]

    pr.check(core(va.stdout) == core(vb.stdout), "alignment core fields (cols 1-10) changed")
    try:
        import pysam

        with pysam.AlignmentFile(str(out), check_sq=False) as af:
            n = sum(1 for _ in af.fetch(until_eof=True))
        pr.check(str(n).encode() == cb.stdout.strip(), f"pysam read count {n}")
    except ImportError:
        pass
    except Exception as e:  # noqa: BLE001
        pr.append(f"pysam.AlignmentFile failed: {type(e).__name__}: {e}")
    return pr


SAM_STRATEGIES = [None, "optional", "comment", "quality", "auxiliary", "multi"]

# --------------------------------------------------------------------------
# DICOM (pydicom)
# --------------------------------------------------------------------------


def gen_dicom(p: Path) -> None:
    from pydicom.dataset import Dataset, FileMetaDataset
    from pydicom.uid import (
        CTImageStorage,
        ExplicitVRLittleEndian,
        PYDICOM_IMPLEMENTATION_UID,
        generate_uid,
    )

    r = rng()
    y, x = np.mgrid[0:128, 0:128]
    img = 1000 + 800 * np.exp(-((x - 64) ** 2 + (y - 64) ** 2) / 900.0) + r.normal(0, 20, (128, 128))
    pixels = np.clip(img, 0, 4095).astype(np.uint16)

    meta = FileMetaDataset()
    meta.MediaStorageSOPClassUID = CTImageStorage
    meta.MediaStorageSOPInstanceUID = generate_uid(entropy_srcs=["fv-sop"])
    meta.TransferSyntaxUID = ExplicitVRLittleEndian
    meta.ImplementationClassUID = PYDICOM_IMPLEMENTATION_UID

    ds = Dataset()
    ds.file_meta = meta
    ds.SOPClassUID = CTImageStorage
    ds.SOPInstanceUID = meta.MediaStorageSOPInstanceUID
    ds.StudyInstanceUID = generate_uid(entropy_srcs=["fv-study"])
    ds.SeriesInstanceUID = generate_uid(entropy_srcs=["fv-series"])
    ds.PatientName = "Validity^Format"
    ds.PatientID = "FV-0049"
    ds.StudyDate = "20261016"
    ds.Modality = "CT"
    ds.Manufacturer = "stegano-ci"
    ds.InstanceNumber = 1
    ds.SamplesPerPixel = 1
    ds.PhotometricInterpretation = "MONOCHROME2"
    ds.Rows, ds.Columns = pixels.shape
    ds.BitsAllocated = 16
    ds.BitsStored = 12
    ds.HighBit = 11
    ds.PixelRepresentation = 0
    ds.RescaleIntercept = -1024
    ds.RescaleSlope = 1
    ds.PixelData = pixels.tobytes()
    ds.save_as(p, enforce_file_format=True)


DICOM_KEEP_TAGS = [
    "SOPClassUID", "SOPInstanceUID", "StudyInstanceUID", "SeriesInstanceUID", "PatientName",
    "PatientID", "Modality", "Rows", "Columns", "BitsAllocated", "BitsStored", "HighBit",
    "PixelRepresentation", "PhotometricInterpretation", "SamplesPerPixel",
]


def validate_dicom(orig: Path, out: Path, strategy: Optional[str]) -> Problems:
    import pydicom
    from pydicom import config

    pr = Problems()
    a = pydicom.dcmread(orig)
    old_mode = config.settings.reading_validation_mode
    config.settings.reading_validation_mode = config.RAISE
    try:
        b = pydicom.dcmread(out)  # no force=True: must be a conformant Part 10 file
        for elem in b.iterall():  # decode every element value
            _ = elem.value
        b_px = b.pixel_array
    except Exception as e:  # noqa: BLE001
        pr.append(f"pydicom dcmread/pixel_array failed: {type(e).__name__}: {e}")
        return pr
    finally:
        config.settings.reading_validation_mode = old_mode
    a_px = a.pixel_array
    pr.check(b.file_meta.TransferSyntaxUID == a.file_meta.TransferSyntaxUID, "transfer syntax changed")
    pr.check(b_px.shape == a_px.shape, f"pixel_array shape {a_px.shape} -> {b_px.shape}")
    pr.check(b_px.dtype == a_px.dtype, f"pixel_array dtype {a_px.dtype} -> {b_px.dtype}")
    for tag in DICOM_KEEP_TAGS:
        pr.check(a.get(tag) == b.get(tag), f"{tag} changed {a.get(tag)!r} -> {b.get(tag)!r}")
    if strategy not in ("pixel_lsb", "multi"):
        pr.check(np.array_equal(a_px, b_px), f"pixel data changed by strategy {strategy}")
    else:
        bits_stored = int(a.BitsStored)
        pr.check(int(b_px.max()) < (1 << bits_stored), f"pixel values exceed BitsStored={bits_stored}")
    return pr


DICOM_STRATEGIES = [None, "private", "metadata", "sequence", "pixel_lsb", "multi"]

# --------------------------------------------------------------------------
# CSV (Python csv module) and plain text
# --------------------------------------------------------------------------


# Same eligibility rule as src/handlers/csv.rs: unquoted plain decimal whose
# mantissa ends in a digit.
CSV_NUMERIC = re.compile(r"[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?\Z")


# Default csv_min_significant_digits: cells with fewer significant digits
# must be untouched.
CSV_MIN_SIG_DIGITS = 2


def _sig_digits(cell: str) -> int:
    """Significant digits of a numeric cell's mantissa (as in csv.rs)."""
    mant = re.split(r"[eE]", cell, maxsplit=1)[0]
    return max(1, len(re.sub(r"\D", "", mant).lstrip("0")))


def _ulp(cell: str) -> float:
    """One unit in the last printed mantissa place of a numeric cell."""
    m = re.match(r"([^eE]*)(?:[eE]([+-]?\d+))?\Z", cell)
    mant, exp = m.group(1), int(m.group(2) or 0)
    frac = len(mant.split(".", 1)[1]) if "." in mant else 0
    return 10.0 ** (exp - frac)


def gen_csv(p: Path) -> None:
    # Header with a quoted name, quoted cells containing the delimiter, escaped
    # quotes and a line break, negative and exponent numbers, CRLF endings and
    # no final newline.
    r = rng()
    rows = ['time_s,"sensor, id",temp_c,flux,note']
    for i in range(600):
        note = ['"ok"', '"says ""hi"", twice"', '"two\r\nlines"', "plain", ""][i % 5]
        rows.append(
            f"{i * 0.05:.2f},S{i % 4},{r.normal(21, 3):.3f},{r.lognormal(-9, 2):.4e},{note}"
        )
    p.write_bytes("\r\n".join(rows).encode())


def _csv_rows(path: Path) -> list[list[str]]:
    import csv

    with open(path, newline="", encoding="utf-8") as fh:
        return list(csv.reader(fh, strict=True))


def validate_csv(orig: Path, out: Path, strategy: Optional[str]) -> Problems:
    pr = Problems()
    a = _csv_rows(orig)
    try:
        b = _csv_rows(out)
    except Exception as e:  # noqa: BLE001
        pr.append(f"csv.reader failed: {type(e).__name__}: {e}")
        return pr
    pr.check(len(a) == len(b), f"row count {len(a)} -> {len(b)}")
    pr.check(bool(a) and bool(b) and a[0] == b[0], "header row changed")
    shape = sum(1 for x, y in zip(a, b) if len(x) != len(y))
    pr.check(shape == 0, f"{shape} rows changed column count")
    text_diff = num_bad = low_sig = 0
    for x, y in zip(a, b):
        for cx, cy in zip(x, y):
            if cx == cy:
                continue
            if not CSV_NUMERIC.match(cx):
                text_diff += 1
            elif _sig_digits(cx) < CSV_MIN_SIG_DIGITS:
                low_sig += 1
            elif not CSV_NUMERIC.match(cy) or len(cx) != len(cy) or \
                    abs(float(cx) - float(cy)) > _ulp(cx) * (1 + 1e-9):
                num_bad += 1
    pr.check(text_diff == 0, f"{text_diff} non-numeric cells changed")
    pr.check(low_sig == 0, f"{low_sig} cells with < {CSV_MIN_SIG_DIGITS} significant digits changed")
    pr.check(num_bad == 0, f"{num_bad} numeric cells changed by more than one last-place unit")
    ra, rb = orig.read_bytes(), out.read_bytes()
    pr.check(ra.count(b"\r\n") == rb.count(b"\r\n"), "CRLF line terminators not preserved")
    pr.check(ra.endswith(b"\n") == rb.endswith(b"\n"), "final-newline state changed")
    pr.check(len(ra) == len(rb), f"file size {len(ra)} -> {len(rb)}")
    return pr


def _text_lines(eol: str) -> str:
    r = rng()
    words = ["alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta"]
    lines = []
    for i in range(300):
        line = " ".join(r.choice(words, int(r.integers(0, 9))))
        if i % 7 == 3:
            line += "   "  # existing trailing spaces
        if i % 11 == 5:
            line += " \t"  # existing trailing tab
        if i % 13 == 0:
            line = "Ünïcödé — 日本語 ✓ " + line
        lines.append(line)
    return eol.join(lines) + eol


def gen_text_lf(p: Path) -> None:
    p.write_bytes(_text_lines("\n").encode())


def gen_text_crlf(p: Path) -> None:
    # CRLF and no final newline.
    p.write_bytes(_text_lines("\r\n")[:-2].encode())


def validate_text(orig: Path, out: Path, strategy: Optional[str]) -> Problems:
    pr = Problems()
    ra, rb = orig.read_bytes(), out.read_bytes()
    try:
        ta, tb = ra.decode("utf-8"), rb.decode("utf-8")
    except UnicodeDecodeError as e:
        pr.append(f"output is not UTF-8: {e}")
        return pr
    la, lb = ta.split("\n"), tb.split("\n")
    pr.check(len(la) == len(lb), f"line count {len(la)} -> {len(lb)}")
    diff = sum(1 for x, y in zip(la, lb) if x.rstrip() != y.rstrip())
    pr.check(diff == 0, f"{diff} lines differ after rstrip")
    pr.check(ra.count(b"\r\n") == rb.count(b"\r\n"), "CRLF line terminators not preserved")
    pr.check(ra.endswith(b"\n") == rb.endswith(b"\n"), "final-newline state changed")
    other = set(rb) - set(ra) - {ord(" "), ord("\t")}
    pr.check(not other, f"new byte values introduced: {sorted(other)}")
    return pr

# --------------------------------------------------------------------------
# Carrier matrix
# --------------------------------------------------------------------------

CARRIERS: list[Carrier] = [
    Carrier("png-rgb8", "png", "rgb8.png", gen_png_rgb8, validate_image, IMAGE_STRATEGIES),
    Carrier("png-gray16", "png", "gray16.png", gen_png_gray16, validate_image, IMAGE_STRATEGIES),
    Carrier("tiff-rgb8", "tiff", "rgb8.tiff", gen_tiff_rgb8, validate_image, IMAGE_STRATEGIES),
    Carrier("wav16-mono", "wav", "pcm16-mono.wav", gen_wav16_mono, validate_wav, WAV_STRATEGIES,
            payload_len_for={"echo": 8}),
    Carrier("wav16-stereo", "wav", "pcm16-stereo.wav", gen_wav16_stereo, validate_wav,
            WAV_STRATEGIES, payload_len_for={"echo": 8}),
    Carrier("wav24-stereo", "wav", "pcm24-stereo.wav", gen_wav24_stereo, validate_wav,
            WAV_STRATEGIES, payload_len_for={"echo": 8}),
    Carrier("fits-int16", "fits", "multi-hdu.fits", gen_fits, validate_fits, [None]),
    Carrier("fastq-lf", "fastq", "reads.fastq", gen_fastq_lf, validate_fastq,
            [None, "conservative", "balanced", "aggressive", "quality"]),
    Carrier("fastq-crlf", "fastq", "reads-crlf.fastq", gen_fastq_crlf, validate_fastq,
            [None, "conservative", "balanced", "aggressive", "quality"]),
    Carrier("fasta", "fasta", "contigs.fasta", gen_fasta, validate_fasta, [None, "header"]),
    Carrier("vcf", "vcf", "variants.vcf", gen_vcf, validate_vcf,
            [None, "info", "quality", "format", "id", "multi"]),
    Carrier("sam", "sam", "aln.sam", gen_sam, validate_sam, SAM_STRATEGIES),
    Carrier("bam", "bam", "aln.bam", gen_bam, validate_sam, SAM_STRATEGIES),
    Carrier("dicom-mono16", "dicom", "ct.dcm", gen_dicom, validate_dicom, DICOM_STRATEGIES),
    Carrier("csv-crlf", "csv", "timeseries.csv", gen_csv, validate_csv, [None, "digit"]),
    Carrier("text-lf", "text", "notes.txt", gen_text_lf, validate_text, [None, "whitespace"]),
    Carrier("text-crlf", "text", "notes-crlf.txt", gen_text_crlf, validate_text, [None]),
]

# Strategies that MUST be rejected for a given carrier.  Each must fail
# cleanly: non-zero exit (not a signal), no panic, and no output file.
EXPECTED_ERRORS: dict[str, dict[str, str]] = {
    # A strategy name the handler does not know must be rejected, not
    # silently mapped to a default.
    "png-rgb8": {"bogus": "unknown imagery strategy"},
    "wav16-mono": {"bogus": "unknown signal strategy"},
    "fastq-lf": {"header": "FASTA-only strategy on FASTQ"},
    "fasta": {"quality": "FASTQ-only strategy on FASTA"},
    "vcf": {"bogus": "unknown vcf strategy"},
    "sam": {"bogus": "unknown sam strategy"},
    "dicom-mono16": {"bogus": "unknown dicom strategy"},
    "csv-crlf": {"bogus": "unknown csv strategy"},
    "text-lf": {"bogus": "unknown text strategy"},
}

for _c in CARRIERS:
    _c.expected_errors.update(EXPECTED_ERRORS.get(_c.name, {}))

FAMILIES = sorted({c.family for c in CARRIERS})

# --------------------------------------------------------------------------
# Runner
# --------------------------------------------------------------------------


def strategy_args(s: Optional[str]) -> list[str]:
    return [] if s is None else ["--strategy", s]


def panicked(stderr: bytes) -> bool:
    return b"panicked" in stderr or b"RUST_BACKTRACE" in stderr


def run_case(binary: str, carrier: Carrier, src: Path, work: Path, strategy: Optional[str],
             expect_error: Optional[str]) -> Result:
    label = strategy or "(default)"
    t0 = time.monotonic()
    case_dir = work / f"{carrier.name}--{label.strip('()')}"
    case_dir.mkdir()
    out = case_dir / f"stego-{carrier.filename}"
    payload = case_dir / "payload.bin"
    extracted = case_dir / "extracted.bin"
    n = carrier.payload_len_for.get(strategy or "", carrier.payload_len)
    payload_bytes = os.urandom(n)
    payload.write_bytes(payload_bytes)
    src_hash = sha256(src)

    def res(status: str, detail: str = "") -> Result:
        return Result(carrier.name, label, status, detail, time.monotonic() - t0)

    emb = run([binary, "embed", "-i", str(src), "-p", str(payload), "-o", str(out),
               "--password", PASSWORD, *strategy_args(strategy)])
    if sha256(src) != src_hash:
        return res("FAIL", "input carrier was modified in place")

    if expect_error is not None:
        problems = []
        if emb.returncode == 0:
            problems.append("embed unexpectedly succeeded")
        elif emb.returncode < 0:
            problems.append(f"embed killed by signal {-emb.returncode}")
        if panicked(emb.stderr):
            problems.append("embed panicked: " + tail(emb.stderr))
        if out.exists():
            problems.append("output file left behind")
        if problems:
            return res("FAIL", f"expected error ({expect_error}): " + "; ".join(problems))
        return res("XFAIL-OK", f"{expect_error}: {tail(emb.stderr, 1)}")

    if emb.returncode != 0:
        why = "panic" if panicked(emb.stderr) else f"exit {emb.returncode}"
        return res("FAIL", f"embed {why}: {tail(emb.stderr)}")
    if not out.exists():
        return res("FAIL", "embed exited 0 but wrote no output file")

    ext = run([binary, "extract", "-i", str(out), "-o", str(extracted),
               "--password", PASSWORD, *strategy_args(strategy)])
    if ext.returncode != 0:
        why = "panic" if panicked(ext.stderr) else f"exit {ext.returncode}"
        return res("FAIL", f"extract {why}: {tail(ext.stderr)}")
    if not extracted.exists() or extracted.read_bytes() != payload_bytes:
        got = extracted.read_bytes() if extracted.exists() else b""
        return res("FAIL", f"payload mismatch ({n} bytes in, {len(got)} bytes out)")

    try:
        problems = carrier.validate(src, out, strategy)
    except Exception as e:  # noqa: BLE001
        return res("FAIL", f"validator crashed: {type(e).__name__}: {e}")
    if problems:
        return res("FAIL", "; ".join(problems))
    return res("PASS", f"{n}-byte payload; output valid")


def tool_versions(binary: str) -> list[tuple[str, str]]:
    vers: list[tuple[str, str]] = [("python", platform.python_version())]
    for mod, attr in [("PIL", "__version__"), ("numpy", "__version__"),
                      ("soundfile", "__version__"), ("astropy", "__version__"),
                      ("Bio", "__version__"), ("pydicom", "__version__"),
                      ("pysam", "__version__")]:
        try:
            m = __import__(mod)
            v = getattr(m, attr, "?")
            if mod == "soundfile":
                v = f"{v} (libsndfile {getattr(m, '__libsndfile_version__', '?')})"
            vers.append((mod, str(v)))
        except ImportError:
            vers.append((mod, "not installed"))
    for exe in ("samtools", "bcftools"):
        path = shutil.which(exe)
        vers.append((exe, first_line(run([path, "--version"]).stdout) if path else "not found"))
    vers.append(("stegano", f"{binary} (sha256 {sha256(Path(binary))[:12]})"))
    return vers


def print_table(results: list[Result]) -> None:
    w1 = max(len("carrier"), *(len(r.carrier) for r in results))
    w2 = max(len("strategy"), *(len(r.strategy) for r in results))
    w3 = max(len("result"), *(len(r.status) for r in results))
    print(f"\n{'carrier':<{w1}}  {'strategy':<{w2}}  {'result':<{w3}}  {'time':>6}  detail")
    print(f"{'-' * w1}  {'-' * w2}  {'-' * w3}  {'-' * 6}  {'-' * 40}")
    for r in results:
        detail = r.detail if len(r.detail) <= 220 else r.detail[:217] + "..."
        print(f"{r.carrier:<{w1}}  {r.strategy:<{w2}}  {r.status:<{w3}}  {r.seconds:5.1f}s  {detail}")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("binary", help="path to a release `stegano` CLI binary")
    ap.add_argument("--only", action="append", choices=FAMILIES, metavar="FORMAT",
                    help=f"run only this format (repeatable): {', '.join(FAMILIES)}")
    ap.add_argument("--keep", metavar="DIR", help="keep generated files in DIR")
    args = ap.parse_args()

    binary = str(Path(args.binary).resolve())
    if not os.access(binary, os.X_OK):
        print(f"error: {binary} is not an executable", file=sys.stderr)
        return 2

    print("Tool versions:")
    for name, v in tool_versions(binary):
        print(f"  {name:<10} {v}")

    selected = [c for c in CARRIERS if not args.only or c.family in args.only]
    if args.keep:
        work = Path(args.keep).resolve()
        work.mkdir(parents=True, exist_ok=True)
        tmp = None
    else:
        tmp = tempfile.TemporaryDirectory(prefix="stegano-fv-")
        work = Path(tmp.name)

    results: list[Result] = []
    try:
        for c in selected:
            cdir = work / c.name
            cdir.mkdir(exist_ok=True)
            src = cdir / c.filename
            try:
                c.generate(src)
                base = c.validate(src, src, None)
            except Exception as e:  # noqa: BLE001
                results.append(Result(c.name, "(carrier)", "FAIL", f"generation: {type(e).__name__}: {e}"))
                continue
            if base:
                results.append(Result(c.name, "(carrier)", "FAIL", "baseline invalid: " + "; ".join(base)))
                continue
            cases = [(s, None) for s in c.strategies] + list(c.expected_errors.items())
            for s, why in cases:
                r = run_case(binary, c, src, cdir, s, why)
                print(f"  {r.status:<8} {c.name} {r.strategy}", flush=True)
                results.append(r)
    finally:
        if tmp:
            tmp.cleanup()

    print_table(results)
    failed = [r for r in results if r.status == "FAIL"]
    print(f"\n{len(results)} cases: {sum(r.status == 'PASS' for r in results)} pass, "
          f"{sum(r.status == 'XFAIL-OK' for r in results)} expected-error ok, {len(failed)} fail")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
