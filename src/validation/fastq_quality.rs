//! FASTQ quality-alphabet analysis: off-bin values and histogram divergence.
//!
//! Binned FASTQ files use only a few Phred values, for example the four
//! values of Illumina RTA3 (`Q2`, `Q12`, `Q23`, `Q37`).  Changing the low bits
//! of a quality character moves it off its bin, so a single pass that counts
//! values outside the alphabet shows whether the qualities were changed after
//! binning.  This is how stegano's own FASTQ quality strategy shows up (see
//! the FASTQ section of `docs/FORMATS.md`).
//!
//! [`analyze_fastq_qualities`] builds the Phred histogram, takes the alphabet
//! from the caller or infers it from a known [`BinningScheme`], counts the
//! off-bin values and, given a reference histogram, measures how far the
//! file's histogram is from it.
//!
//! ```
//! use stegano::validation::fastq_quality::*;
//!
//! let fastq = b"@r1\nACGT\n+\nFF8-\n@r2\nACGT\n+\nFF8-\n@r3\nACGT\n+\nFG8#\n";
//! let report = analyze_fastq_qualities(fastq, &FastqQualityOptions::default())?;
//! assert_eq!(
//!     report.alphabet,
//!     Some(QualityAlphabet::Scheme(BinningScheme::Rta3FourBin))
//! );
//! assert_eq!(report.off_bin_count, 1); // 'G' (Q38) is not an RTA3 bin
//! # Ok::<(), stegano::SteganoError>(())
//! ```

use super::chi_square_survival;
use crate::core::{Result, SteganoError};
use crate::handlers::genomics::fastq_quality_lines;

/// ASCII offset of Phred+33 quality characters.
pub const PHRED_OFFSET: u8 = 33;
/// Number of Phred values representable in Phred+33 (`Q0`..=`Q93`).
pub const PHRED_VALUES: usize = 94;
/// Minimum share of quality characters a known scheme must cover before
/// [`analyze_fastq_qualities`] infers it as the file's alphabet.
pub const INFER_MIN_COVERAGE: f64 = 0.9;
/// Default number of off-bin positions kept in a report.
pub const DEFAULT_MAX_SAMPLES: usize = 10;

/// Published quality-binning schemes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BinningScheme {
    /// Illumina RTA3 (NovaSeq 6000, iSeq, MiniSeq): `Q2`, `Q12`, `Q23`, `Q37`.
    Rta3FourBin,
    /// Illumina 8-level binning (HiSeq X/2500/4000, MiSeq with binning on):
    /// `Q2`, `Q6`, `Q15`, `Q22`, `Q27`, `Q33`, `Q37`, `Q40`.
    Illumina8Bin,
}

impl BinningScheme {
    /// Every known scheme, in inference order.
    pub const ALL: [BinningScheme; 2] = [BinningScheme::Rta3FourBin, BinningScheme::Illumina8Bin];

    /// Name accepted by [`from_name`](Self::from_name) and `--quality-bins`.
    pub fn name(self) -> &'static str {
        match self {
            BinningScheme::Rta3FourBin => "rta3-4bin",
            BinningScheme::Illumina8Bin => "illumina-8bin",
        }
    }

    /// Phred values of the bins, ascending.
    pub fn bins(self) -> &'static [u8] {
        match self {
            BinningScheme::Rta3FourBin => &[2, 12, 23, 37],
            BinningScheme::Illumina8Bin => &[2, 6, 15, 22, 27, 33, 37, 40],
        }
    }

    /// Look up a scheme by name (case-insensitive; `rta3`, `4bin` and
    /// `8bin` are accepted as short forms).
    pub fn from_name(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "rta3-4bin" | "rta3" | "4bin" => Some(BinningScheme::Rta3FourBin),
            "illumina-8bin" | "8bin" => Some(BinningScheme::Illumina8Bin),
            _ => None,
        }
    }
}

/// The set of Phred values a file is expected to use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QualityAlphabet {
    /// A known binning scheme.
    Scheme(BinningScheme),
    /// Caller-supplied Phred values (ascending, no duplicates).
    Custom(Vec<u8>),
}

impl QualityAlphabet {
    /// Parse a scheme name (see [`BinningScheme::from_name`]) or a
    /// comma-separated list of Phred values such as `2,12,23,37`.
    pub fn parse(s: &str) -> Result<Self> {
        if let Some(scheme) = BinningScheme::from_name(s.trim()) {
            return Ok(QualityAlphabet::Scheme(scheme));
        }
        let mut bins = Vec::new();
        for part in s.split(',') {
            let q: u8 = part.trim().parse().map_err(|_| {
                SteganoError::InvalidParameter(format!(
                    "invalid quality alphabet '{s}' (expected rta3-4bin, illumina-8bin \
                     or comma-separated Phred values 0-93)"
                ))
            })?;
            if q as usize >= PHRED_VALUES {
                return Err(SteganoError::InvalidParameter(format!(
                    "Phred value {q} out of range 0-93"
                )));
            }
            bins.push(q);
        }
        bins.sort_unstable();
        bins.dedup();
        Ok(QualityAlphabet::Custom(bins))
    }

    /// Phred values of the alphabet, ascending.
    pub fn bins(&self) -> &[u8] {
        match self {
            QualityAlphabet::Scheme(s) => s.bins(),
            QualityAlphabet::Custom(b) => b,
        }
    }

    /// Scheme name, or `custom` for a caller-supplied list.
    pub fn name(&self) -> &'static str {
        match self {
            QualityAlphabet::Scheme(s) => s.name(),
            QualityAlphabet::Custom(_) => "custom",
        }
    }

    fn contains(&self, phred: u8) -> bool {
        self.bins().binary_search(&phred).is_ok()
    }
}

/// Where a report's alphabet came from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AlphabetSource {
    /// Supplied by the caller ([`FastqQualityOptions::alphabet`]).
    Declared,
    /// The known scheme covering the most quality characters, which covered
    /// at least [`INFER_MIN_COVERAGE`] of them.
    Inferred,
}

/// Histogram of Phred+33 quality values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityHistogram {
    counts: [u64; PHRED_VALUES],
    invalid: u64,
}

impl Default for QualityHistogram {
    fn default() -> Self {
        Self::new()
    }
}

impl QualityHistogram {
    /// Empty histogram.
    pub fn new() -> Self {
        Self {
            counts: [0; PHRED_VALUES],
            invalid: 0,
        }
    }

    /// Histogram of every quality character in the complete records of a
    /// FASTQ file.
    pub fn from_fastq(data: &[u8]) -> Result<Self> {
        let mut h = Self::new();
        for line in fastq_quality_lines(data)? {
            h.add_line(line);
        }
        Ok(h)
    }

    /// Add the characters of one quality line.
    pub fn add_line(&mut self, line: &[u8]) {
        for &c in line {
            match phred(c) {
                Some(q) => self.counts[q as usize] += 1,
                None => self.invalid += 1,
            }
        }
    }

    /// Count per Phred value, indexed by `Q`.
    pub fn counts(&self) -> &[u64; PHRED_VALUES] {
        &self.counts
    }

    /// Characters outside `'!'..='~'` (not valid Phred+33).
    pub fn invalid(&self) -> u64 {
        self.invalid
    }

    /// Number of valid quality characters.
    pub fn total(&self) -> u64 {
        self.counts.iter().sum()
    }

    /// Number of distinct Phred values present.
    pub fn distinct(&self) -> usize {
        self.counts.iter().filter(|&&c| c > 0).count()
    }

    /// Share of valid quality characters whose value is in `alphabet`
    /// (0 for an empty histogram).
    pub fn coverage(&self, alphabet: &QualityAlphabet) -> f64 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        let inside: u64 = alphabet
            .bins()
            .iter()
            .map(|&q| self.counts[q as usize])
            .sum();
        inside as f64 / total as f64
    }

    /// The known scheme with the highest coverage, and that coverage.
    pub fn best_scheme(&self) -> (BinningScheme, f64) {
        let mut best = (BinningScheme::ALL[0], f64::MIN);
        for s in BinningScheme::ALL {
            let c = self.coverage(&QualityAlphabet::Scheme(s));
            if c > best.1 {
                best = (s, c);
            }
        }
        best
    }
}

fn phred(c: u8) -> Option<u8> {
    let q = c.checked_sub(PHRED_OFFSET)?;
    ((q as usize) < PHRED_VALUES).then_some(q)
}

/// One quality character outside the alphabet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OffBinSample {
    /// 0-based index of the complete FASTQ record.
    pub record: usize,
    /// 0-based position in the record's quality line.
    pub column: usize,
    /// Phred value found.
    pub phred: u8,
}

/// Distance between a file's quality histogram and a reference histogram.
///
/// The reference is smoothed with 0.5 per Phred value present in either
/// histogram, so values the reference never uses give a large but finite
/// contribution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HistogramDivergence {
    /// Pearson chi-square statistic of the file against the reference.
    pub chi_square: f64,
    /// Number of Phred values compared, minus one.
    pub degrees_of_freedom: usize,
    /// Upper-tail p-value of `chi_square`.
    pub p_value: f64,
    /// Kullback-Leibler divergence `KL(file || reference)` in nats.
    pub kl_divergence: f64,
}

/// Options for [`analyze_fastq_qualities`].
#[derive(Debug, Clone)]
pub struct FastqQualityOptions {
    /// Expected alphabet. `None` infers a known [`BinningScheme`] when one
    /// covers at least [`INFER_MIN_COVERAGE`] of the quality characters.
    pub alphabet: Option<QualityAlphabet>,
    /// Histogram of a file known to be unmodified (for example the same run
    /// before processing), for [`FastqQualityReport::divergence`].
    pub reference: Option<QualityHistogram>,
    /// Maximum number of off-bin positions to record.
    pub max_samples: usize,
}

impl Default for FastqQualityOptions {
    fn default() -> Self {
        Self {
            alphabet: None,
            reference: None,
            max_samples: DEFAULT_MAX_SAMPLES,
        }
    }
}

/// Result of [`analyze_fastq_qualities`].
#[derive(Debug, Clone)]
pub struct FastqQualityReport {
    /// Number of complete FASTQ records.
    pub records: usize,
    /// Phred histogram of all quality characters.
    pub histogram: QualityHistogram,
    /// The known scheme with the highest coverage, and that coverage.
    pub best_scheme: (BinningScheme, f64),
    /// Alphabet used for the off-bin check; `None` when none was declared
    /// and no known scheme reached [`INFER_MIN_COVERAGE`].
    pub alphabet: Option<QualityAlphabet>,
    /// Where `alphabet` came from.
    pub alphabet_source: Option<AlphabetSource>,
    /// Quality characters outside `alphabet` (0 when `alphabet` is `None`).
    pub off_bin_count: u64,
    /// `off_bin_count` divided by the number of valid quality characters.
    pub off_bin_fraction: f64,
    /// The first off-bin positions, in file order.
    pub off_bin_samples: Vec<OffBinSample>,
    /// Comparison with [`FastqQualityOptions::reference`], when given.
    pub divergence: Option<HistogramDivergence>,
}

/// Analyze the quality alphabet of a FASTQ file (complete 4-line records).
///
/// A non-zero [`off_bin_count`](FastqQualityReport::off_bin_count) against
/// the right alphabet means quality values were changed after binning.
/// Inference needs the file to stay mostly on its bins: when most characters
/// have been moved, no scheme reaches [`INFER_MIN_COVERAGE`] and the caller
/// should declare the alphabet.
pub fn analyze_fastq_qualities(
    data: &[u8],
    options: &FastqQualityOptions,
) -> Result<FastqQualityReport> {
    let lines = fastq_quality_lines(data)?;
    let mut histogram = QualityHistogram::new();
    for line in &lines {
        histogram.add_line(line);
    }

    let best_scheme = histogram.best_scheme();
    let (alphabet, alphabet_source) = match &options.alphabet {
        Some(a) => (Some(a.clone()), Some(AlphabetSource::Declared)),
        None if best_scheme.1 >= INFER_MIN_COVERAGE => (
            Some(QualityAlphabet::Scheme(best_scheme.0)),
            Some(AlphabetSource::Inferred),
        ),
        None => (None, None),
    };

    let mut off_bin_count = 0;
    let mut off_bin_samples = Vec::new();
    if let Some(a) = &alphabet {
        for (record, line) in lines.iter().enumerate() {
            for (column, &c) in line.iter().enumerate() {
                let Some(q) = phred(c) else { continue };
                if !a.contains(q) {
                    off_bin_count += 1;
                    if off_bin_samples.len() < options.max_samples {
                        off_bin_samples.push(OffBinSample {
                            record,
                            column,
                            phred: q,
                        });
                    }
                }
            }
        }
    }

    let total = histogram.total();
    let off_bin_fraction = if total == 0 {
        0.0
    } else {
        off_bin_count as f64 / total as f64
    };
    let divergence = options
        .reference
        .as_ref()
        .and_then(|r| histogram_divergence(&histogram, r));

    Ok(FastqQualityReport {
        records: lines.len(),
        histogram,
        best_scheme,
        alphabet,
        alphabet_source,
        off_bin_count,
        off_bin_fraction,
        off_bin_samples,
        divergence,
    })
}

/// Compare `observed` with `reference` (see [`HistogramDivergence`]).
/// `None` if either histogram is empty.
pub fn histogram_divergence(
    observed: &QualityHistogram,
    reference: &QualityHistogram,
) -> Option<HistogramDivergence> {
    let (n_obs, n_ref) = (observed.total() as f64, reference.total() as f64);
    if n_obs == 0.0 || n_ref == 0.0 {
        return None;
    }
    let support: Vec<usize> = (0..PHRED_VALUES)
        .filter(|&q| observed.counts[q] > 0 || reference.counts[q] > 0)
        .collect();
    let smoothed_total = n_ref + 0.5 * support.len() as f64;

    let mut chi_square = 0.0;
    let mut kl_divergence = 0.0;
    for &q in &support {
        let p_ref = (reference.counts[q] as f64 + 0.5) / smoothed_total;
        let o = observed.counts[q] as f64;
        let e = p_ref * n_obs;
        chi_square += (o - e).powi(2) / e;
        if o > 0.0 {
            let p_obs = o / n_obs;
            kl_divergence += p_obs * (p_obs / p_ref).ln();
        }
    }
    let degrees_of_freedom = support.len().saturating_sub(1);
    Some(HistogramDivergence {
        chi_square,
        degrees_of_freedom,
        p_value: chi_square_survival(chi_square, degrees_of_freedom),
        kl_divergence: kl_divergence.max(0.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::genomics::{GenomicsHandler, QualityEmbedStrategy};

    /// Deterministic pseudo-random generator for test data.
    fn lcg(seed: &mut u64) -> u64 {
        *seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *seed >> 33
    }

    /// FASTQ whose qualities are drawn from the Phred values `alphabet`.
    fn synth_fastq(records: usize, len: usize, alphabet: &[u8], seed: u64) -> Vec<u8> {
        let mut seed = seed;
        let mut out = Vec::new();
        for r in 0..records {
            out.extend_from_slice(format!("@read{r}\n").as_bytes());
            out.extend((0..len).map(|_| b"ACGT"[(lcg(&mut seed) % 4) as usize]));
            out.extend_from_slice(b"\n+\n");
            out.extend((0..len).map(|_| {
                alphabet[(lcg(&mut seed) % alphabet.len() as u64) as usize] + PHRED_OFFSET
            }));
            out.push(b'\n');
        }
        out
    }

    fn payload(len: usize) -> Vec<u8> {
        let mut seed = 7u64;
        (0..len).map(|_| lcg(&mut seed) as u8).collect()
    }

    const RTA3: &[u8] = &[2, 12, 23, 37];
    const BIN8: &[u8] = &[2, 6, 15, 22, 27, 33, 37, 40];

    fn unbinned() -> Vec<u8> {
        (2..=41).collect()
    }

    #[test]
    fn scheme_names_round_trip() {
        for s in BinningScheme::ALL {
            assert_eq!(BinningScheme::from_name(s.name()), Some(s));
            assert!(s.bins().windows(2).all(|w| w[0] < w[1]));
        }
        assert_eq!(
            BinningScheme::from_name("RTA3"),
            Some(BinningScheme::Rta3FourBin)
        );
        assert_eq!(
            BinningScheme::from_name("8bin"),
            Some(BinningScheme::Illumina8Bin)
        );
        assert_eq!(BinningScheme::from_name("nope"), None);
    }

    #[test]
    fn alphabet_parse() {
        assert_eq!(
            QualityAlphabet::parse("rta3-4bin").unwrap(),
            QualityAlphabet::Scheme(BinningScheme::Rta3FourBin)
        );
        let custom = QualityAlphabet::parse("37, 2,12,12").unwrap();
        assert_eq!(custom, QualityAlphabet::Custom(vec![2, 12, 37]));
        assert_eq!(custom.name(), "custom");
        assert!(QualityAlphabet::parse("2,x").is_err());
        assert!(QualityAlphabet::parse("94").is_err());
        assert!(QualityAlphabet::parse("").is_err());
    }

    #[test]
    fn histogram_counts() {
        let data = b"@r\nACGT\n+\n#FF-\n@r\nAC\n+\n8F\n";
        let h = QualityHistogram::from_fastq(data).unwrap();
        assert_eq!(h.total(), 6);
        assert_eq!(h.counts()[37], 3);
        assert_eq!(h.counts()[2], 1);
        assert_eq!(h.counts()[12], 1);
        assert_eq!(h.counts()[23], 1);
        assert_eq!(h.distinct(), 4);
        assert_eq!(h.invalid(), 0);

        let mut h = QualityHistogram::new();
        h.add_line(b"F \x7f");
        assert_eq!((h.total(), h.invalid()), (1, 2));
    }

    #[test]
    fn rejects_non_fastq() {
        assert!(analyze_fastq_qualities(b"not fastq", &Default::default()).is_err());
    }

    #[test]
    fn clean_binned_files_infer_scheme_and_have_no_off_bin_values() {
        for (alphabet, scheme) in [
            (RTA3, BinningScheme::Rta3FourBin),
            (BIN8, BinningScheme::Illumina8Bin),
        ] {
            let data = synth_fastq(200, 100, alphabet, 1);
            let r = analyze_fastq_qualities(&data, &Default::default()).unwrap();
            assert_eq!(r.records, 200);
            assert_eq!(r.alphabet, Some(QualityAlphabet::Scheme(scheme)));
            assert_eq!(r.alphabet_source, Some(AlphabetSource::Inferred));
            assert_eq!(r.off_bin_count, 0);
            assert_eq!(r.off_bin_fraction, 0.0);
            assert!(r.off_bin_samples.is_empty());
            assert_eq!(r.best_scheme, (scheme, 1.0));
        }
    }

    #[test]
    fn unbinned_file_infers_nothing() {
        let data = synth_fastq(200, 100, &unbinned(), 1);
        let r = analyze_fastq_qualities(&data, &Default::default()).unwrap();
        assert_eq!(r.alphabet, None);
        assert_eq!(r.alphabet_source, None);
        assert_eq!(r.off_bin_count, 0);
        assert!(r.best_scheme.1 < INFER_MIN_COVERAGE);
        assert_eq!(r.histogram.distinct(), 40);
    }

    #[test]
    fn declared_alphabet_overrides_inference() {
        let data = synth_fastq(50, 100, &unbinned(), 1);
        let options = FastqQualityOptions {
            alphabet: Some(QualityAlphabet::Scheme(BinningScheme::Rta3FourBin)),
            max_samples: 3,
            ..Default::default()
        };
        let r = analyze_fastq_qualities(&data, &options).unwrap();
        assert_eq!(r.alphabet_source, Some(AlphabetSource::Declared));
        let expected = r.histogram.total()
            - RTA3
                .iter()
                .map(|&q| r.histogram.counts()[q as usize])
                .sum::<u64>();
        assert_eq!(r.off_bin_count, expected);
        assert_eq!(r.off_bin_samples.len(), 3);
    }

    /// Every quality-strategy depth moves binned characters off their bins,
    /// and the off-bin positions are where the payload was written.
    #[test]
    fn quality_strategy_embedding_is_detected_on_binned_data() {
        let handler = GenomicsHandler::new();
        for alphabet in [RTA3, BIN8] {
            let clean = synth_fastq(500, 100, alphabet, 3);
            for strategy in QualityEmbedStrategy::ALL {
                let mut data = clean.clone();
                let result = handler
                    .embed_fastq_qualities(&mut data, &payload(256), strategy)
                    .unwrap();
                let r = analyze_fastq_qualities(&data, &Default::default()).unwrap();
                assert!(r.alphabet.is_some(), "{strategy:?}: alphabet not inferred");
                assert!(r.off_bin_count > 0, "{strategy:?}: no off-bin values");
                // Only modified characters can be off-bin.  RTA3 bins are at
                // least 10 apart, so every modified character is; on 8-bin
                // data k = 3 can move Q37 onto Q33.
                let modified = result.quality_chars_modified as u64;
                if alphabet == RTA3 {
                    assert_eq!(r.off_bin_count, modified, "{strategy:?}");
                } else {
                    assert!(r.off_bin_count <= modified, "{strategy:?}");
                }
                // The stream is written from the first eligible character on.
                assert_eq!(r.off_bin_samples[0].record, 0);
                assert!(r
                    .off_bin_samples
                    .windows(2)
                    .all(|w| { (w[0].record, w[0].column) < (w[1].record, w[1].column) }));
            }
        }
    }

    #[test]
    fn divergence_against_reference() {
        let handler = GenomicsHandler::new();
        let clean = synth_fastq(500, 100, RTA3, 3);
        let reference = QualityHistogram::from_fastq(&synth_fastq(500, 100, RTA3, 9)).unwrap();

        let same = histogram_divergence(&QualityHistogram::from_fastq(&clean).unwrap(), &reference)
            .unwrap();
        assert_eq!(same.degrees_of_freedom, 3);
        assert!(same.p_value > 0.001, "clean file flagged: {same:?}");

        let mut data = clean.clone();
        handler
            .embed_fastq_qualities(&mut data, &payload(1024), QualityEmbedStrategy::Balanced)
            .unwrap();
        let options = FastqQualityOptions {
            reference: Some(reference.clone()),
            ..Default::default()
        };
        let d = analyze_fastq_qualities(&data, &options)
            .unwrap()
            .divergence
            .unwrap();
        assert!(d.p_value < 1e-6, "embedded file not flagged: {d:?}");
        assert!(d.kl_divergence > same.kl_divergence);

        assert_eq!(
            histogram_divergence(&QualityHistogram::new(), &reference),
            None
        );
    }

    /// The per-alphabet capacity table in docs/FORMATS.md.
    #[test]
    fn eligibility_per_alphabet_matches_docs() {
        let eligible = |q: u8, s: QualityEmbedStrategy| s.is_eligible(q + PHRED_OFFSET);
        for s in QualityEmbedStrategy::ALL {
            // Q2 ('#') never carries bits; every other bin carries k bits,
            // except Q6 at k = 3.
            assert!(!eligible(2, s));
            for &q in RTA3.iter().chain(BIN8).filter(|&&q| q != 2) {
                let expected = !(q == 6 && s == QualityEmbedStrategy::Aggressive);
                assert_eq!(eligible(q, s), expected, "Q{q} {s:?}");
            }
            // Full-range Phred: Q0-Q2 never; Q3-Q6 not at k = 3; Q7-Q41 always.
            for q in 0..=2 {
                assert!(!eligible(q, s));
            }
            for q in 3..=6 {
                assert_eq!(eligible(q, s), s != QualityEmbedStrategy::Aggressive);
            }
            for q in 7..=41 {
                assert!(eligible(q, s));
            }
        }
    }
}
