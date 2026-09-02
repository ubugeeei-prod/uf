//! Byte sizes, human-readable size parsing, and real compressed measurement.

use std::io::Write;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Gzip level used for every reported measurement.
///
/// Fixed so numbers are comparable across runs and machines. Level 9 is what a
/// CDN serves for static assets.
pub const GZIP_LEVEL: u32 = 9;

/// Brotli quality used for every reported measurement.
pub const BROTLI_QUALITY: u32 = 11;

/// Brotli window size (log2) used for every reported measurement.
pub const BROTLI_WINDOW: u32 = 22;

/// Largest asset `uf` will measure, in bytes.
///
/// Compression is CPU-bound and asset paths come from a build directory that a
/// dependency can write into, so the work is bounded rather than trusted.
pub const MAX_ASSET_BYTES: u64 = 512 * 1024 * 1024;

/// A byte count parsed from configuration.
///
/// Wraps `u64` so a budget cannot be confused with a raw count at a call site,
/// and so parsing lives in exactly one place.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct ByteSize(u64);

impl<'de> Deserialize<'de> for ByteSize {
    /// Accept both a raw byte count and a human-readable string.
    ///
    /// `uf.config.js` is written by people, so `initialJs: "180kb"` has to work;
    /// the report reads back its own numbers, so `180000` has to work too.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl serde::de::Visitor<'_> for Visitor {
            type Value = ByteSize;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a byte count or a size such as \"180kb\"")
            }

            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<ByteSize, E> {
                Ok(ByteSize(value))
            }

            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<ByteSize, E> {
                u64::try_from(value)
                    .map(ByteSize)
                    .map_err(|_| E::custom("a size cannot be negative"))
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<ByteSize, E> {
                parse_byte_size(value).map_err(E::custom)
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

impl ByteSize {
    /// Build a size from a raw byte count.
    #[must_use]
    pub const fn from_bytes(bytes: u64) -> Self {
        Self(bytes)
    }

    /// The raw byte count.
    #[must_use]
    pub const fn bytes(self) -> u64 {
        self.0
    }

    /// Difference against a smaller size, saturating at zero.
    #[must_use]
    pub const fn saturating_sub(self, other: Self) -> Self {
        Self(self.0.saturating_sub(other.0))
    }
}

impl std::fmt::Display for ByteSize {
    /// Render as the largest unit that keeps the value at or above one, using
    /// decimal units because that is how download sizes are reported.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        const UNITS: [(u64, &str); 4] = [
            (1_000_000_000, "GB"),
            (1_000_000, "MB"),
            (1_000, "kB"),
            (1, "B"),
        ];

        for (scale, suffix) in UNITS {
            if self.0 >= scale {
                if scale == 1 {
                    return write!(f, "{} {suffix}", self.0);
                }
                let whole = self.0 / scale;
                let fraction = (self.0 % scale) * 100 / scale;
                return write!(f, "{whole}.{fraction:02} {suffix}");
            }
        }

        write!(f, "0 B")
    }
}

/// Failures while reading a size out of `uf.config.js`.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ByteSizeParseError {
    /// The input held no digits.
    #[error("size `{input}` has no numeric part")]
    Empty {
        /// The rejected input, truncated for safety.
        input: String,
    },
    /// The input carried a unit that is not recognized.
    #[error("size `{input}` uses unknown unit `{unit}`")]
    UnknownUnit {
        /// The rejected input, truncated for safety.
        input: String,
        /// The unrecognized unit.
        unit: String,
    },
    /// The input held characters that are neither digits, a decimal point, nor a unit.
    #[error("size `{input}` contains `{character}`, which is not valid in a size")]
    InvalidCharacter {
        /// The rejected input, truncated for safety.
        input: String,
        /// The offending character.
        character: char,
    },
    /// The value does not fit in a `u64` byte count.
    #[error("size `{input}` does not fit in a 64-bit byte count")]
    Overflow {
        /// The rejected input, truncated for safety.
        input: String,
    },
    /// More than one decimal point appeared.
    #[error("size `{input}` has more than one decimal point")]
    RepeatedDecimalPoint {
        /// The rejected input, truncated for safety.
        input: String,
    },
}

/// Longest size string `uf` will look at.
const MAX_SIZE_INPUT_BYTES: usize = 64;

/// Truncate untrusted text before it lands in an error message.
fn quote(input: &str) -> String {
    input.chars().take(32).collect()
}

/// Parse a human-readable size such as `180kb`, `1.5 MB`, `200KiB`, or `4096`.
///
/// Hand-written rather than regex-backed: this reads project configuration, and
/// a backtracking matcher on untrusted input is a denial-of-service class all by
/// itself. The scan is a single forward pass with no allocation on the happy
/// path until the unit lookup.
///
/// Bare numbers are bytes. Units are case-insensitive. Both decimal (`kB`, `MB`,
/// `GB`) and binary (`KiB`, `MiB`, `GiB`) units are accepted, and a lone `k`,
/// `m`, or `g` is read as the decimal unit, matching how bundlers report sizes.
pub fn parse_byte_size(input: &str) -> Result<ByteSize, ByteSizeParseError> {
    if input.len() > MAX_SIZE_INPUT_BYTES {
        return Err(ByteSizeParseError::Overflow {
            input: quote(input),
        });
    }

    let trimmed = input.trim();
    let bytes = trimmed.as_bytes();

    let mut index = 0;
    let mut whole: u64 = 0;
    let mut fraction: u64 = 0;
    let mut fraction_scale: u64 = 1;
    let mut seen_digit = false;
    let mut seen_point = false;

    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b'0'..=b'9' => {
                seen_digit = true;
                let digit = u64::from(byte - b'0');
                if seen_point {
                    // Digits past the precision we can use cannot change the
                    // result, so stop accumulating instead of overflowing.
                    if fraction_scale <= 1_000_000_000_000_000_000 {
                        fraction = fraction * 10 + digit;
                        fraction_scale *= 10;
                    }
                } else {
                    whole = whole
                        .checked_mul(10)
                        .and_then(|n| n.checked_add(digit))
                        .ok_or_else(|| ByteSizeParseError::Overflow {
                            input: quote(input),
                        })?;
                }
                index += 1;
            }
            b'.' => {
                if seen_point {
                    return Err(ByteSizeParseError::RepeatedDecimalPoint {
                        input: quote(input),
                    });
                }
                seen_point = true;
                index += 1;
            }
            b'_' => index += 1,
            _ => break,
        }
    }

    if !seen_digit {
        return Err(ByteSizeParseError::Empty {
            input: quote(input),
        });
    }

    let unit_text = trimmed[index..].trim();
    for character in unit_text.chars() {
        if !character.is_ascii_alphabetic() {
            return Err(ByteSizeParseError::InvalidCharacter {
                input: quote(input),
                character,
            });
        }
    }

    let multiplier = unit_multiplier(unit_text).ok_or_else(|| ByteSizeParseError::UnknownUnit {
        input: quote(input),
        unit: quote(unit_text),
    })?;

    let scaled_whole =
        whole
            .checked_mul(multiplier)
            .ok_or_else(|| ByteSizeParseError::Overflow {
                input: quote(input),
            })?;
    // `fraction / fraction_scale` is in [0, 1), so this cannot overflow once the
    // whole part has been checked.
    let scaled_fraction = multiplier / fraction_scale * fraction
        + (multiplier % fraction_scale) * fraction / fraction_scale;

    scaled_whole
        .checked_add(scaled_fraction)
        .map(ByteSize::from_bytes)
        .ok_or_else(|| ByteSizeParseError::Overflow {
            input: quote(input),
        })
}

/// Byte multiplier for a size unit, or [`None`] when the unit is unknown.
fn unit_multiplier(unit: &str) -> Option<u64> {
    const KB: u64 = 1_000;
    const MB: u64 = 1_000_000;
    const GB: u64 = 1_000_000_000;
    const KIB: u64 = 1 << 10;
    const MIB: u64 = 1 << 20;
    const GIB: u64 = 1 << 30;

    // Case-insensitive without allocating for the common ASCII inputs.
    let mut lowered = [0u8; 8];
    if unit.len() > lowered.len() {
        return None;
    }
    for (slot, byte) in lowered.iter_mut().zip(unit.as_bytes()) {
        *slot = byte.to_ascii_lowercase();
    }
    let lowered = &lowered[..unit.len()];

    match lowered {
        b"" | b"b" => Some(1),
        b"k" | b"kb" => Some(KB),
        b"m" | b"mb" => Some(MB),
        b"g" | b"gb" => Some(GB),
        b"kib" => Some(KIB),
        b"mib" => Some(MIB),
        b"gib" => Some(GIB),
        _ => None,
    }
}

/// Raw and compressed sizes for one asset.
///
/// Compressed figures come from actually compressing the bytes, never from an
/// estimate — a budget that lies is worse than no budget.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetSize {
    /// Size on disk.
    pub raw: ByteSize,
    /// Size after gzip at [`GZIP_LEVEL`].
    pub gzip: ByteSize,
    /// Size after brotli at [`BROTLI_QUALITY`].
    pub brotli: ByteSize,
}

impl AssetSize {
    /// The size under one metric.
    #[must_use]
    pub const fn get(self, metric: BudgetMetric) -> ByteSize {
        match metric {
            BudgetMetric::Raw => self.raw,
            BudgetMetric::Gzip => self.gzip,
            BudgetMetric::Brotli => self.brotli,
        }
    }

    /// Component-wise sum, saturating rather than wrapping.
    #[must_use]
    pub const fn saturating_add(self, other: Self) -> Self {
        Self {
            raw: ByteSize(self.raw.0.saturating_add(other.raw.0)),
            gzip: ByteSize(self.gzip.0.saturating_add(other.gzip.0)),
            brotli: ByteSize(self.brotli.0.saturating_add(other.brotli.0)),
        }
    }

    /// The zero size, used as a fold identity.
    #[must_use]
    pub const fn zero() -> Self {
        Self {
            raw: ByteSize(0),
            gzip: ByteSize(0),
            brotli: ByteSize(0),
        }
    }
}

/// Which measurement a budget applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BudgetMetric {
    /// Bytes on disk.
    Raw,
    /// Bytes after gzip. The default, because it is what users download.
    #[default]
    Gzip,
    /// Bytes after brotli.
    Brotli,
}

impl BudgetMetric {
    /// Stable identifier used in reports and error messages.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Gzip => "gzip",
            Self::Brotli => "brotli",
        }
    }
}

/// Failures while measuring an asset.
#[derive(Debug, Error)]
pub enum MeasureError {
    /// The asset was larger than [`MAX_ASSET_BYTES`].
    #[error("asset is {actual} bytes, above the {limit} byte measurement limit")]
    TooLarge {
        /// Observed size.
        actual: u64,
        /// Configured limit.
        limit: u64,
    },
    /// Compression failed.
    #[error("failed to compress asset: {0}")]
    Compress(#[source] std::io::Error),
}

/// Measure `contents` raw, gzipped, and brotli-compressed.
pub fn measure(contents: &[u8]) -> Result<AssetSize, MeasureError> {
    let raw = contents.len() as u64;
    if raw > MAX_ASSET_BYTES {
        return Err(MeasureError::TooLarge {
            actual: raw,
            limit: MAX_ASSET_BYTES,
        });
    }

    Ok(AssetSize {
        raw: ByteSize::from_bytes(raw),
        gzip: ByteSize::from_bytes(gzip_size(contents)?),
        brotli: ByteSize::from_bytes(brotli_size(contents)?),
    })
}

fn gzip_size(contents: &[u8]) -> Result<u64, MeasureError> {
    let mut encoder = flate2::write::GzEncoder::new(
        CountingSink::default(),
        flate2::Compression::new(GZIP_LEVEL),
    );
    encoder
        .write_all(contents)
        .map_err(MeasureError::Compress)?;
    Ok(encoder.finish().map_err(MeasureError::Compress)?.written)
}

fn brotli_size(contents: &[u8]) -> Result<u64, MeasureError> {
    let mut sink = CountingSink::default();
    {
        let mut encoder = brotli::CompressorWriter::new(
            &mut sink,
            BROTLI_BUFFER_BYTES,
            BROTLI_QUALITY,
            BROTLI_WINDOW,
        );
        encoder
            .write_all(contents)
            .map_err(MeasureError::Compress)?;
        encoder.flush().map_err(MeasureError::Compress)?;
    }
    Ok(sink.written)
}

/// Brotli's internal buffer. Large enough to keep syscall-free writes cheap
/// without holding a copy of the asset.
const BROTLI_BUFFER_BYTES: usize = 4096;

/// A sink that counts bytes instead of keeping them.
///
/// Compressed output is never needed, only its length, so nothing is retained
/// and a large asset costs no extra memory.
#[derive(Debug, Default)]
struct CountingSink {
    written: u64,
}

impl Write for CountingSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.written += buf.len() as u64;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(input: &str) -> u64 {
        parse_byte_size(input).expect("parses").bytes()
    }

    #[test]
    fn parses_bare_byte_counts() {
        assert_eq!(bytes("0"), 0);
        assert_eq!(bytes("4096"), 4096);
        assert_eq!(bytes("4096b"), 4096);
        assert_eq!(bytes("4096 B"), 4096);
    }

    #[test]
    fn parses_decimal_units() {
        assert_eq!(bytes("180kb"), 180_000);
        assert_eq!(bytes("180 kB"), 180_000);
        assert_eq!(bytes("180K"), 180_000);
        assert_eq!(bytes("1mb"), 1_000_000);
        assert_eq!(bytes("1gb"), 1_000_000_000);
    }

    #[test]
    fn parses_binary_units() {
        assert_eq!(bytes("200KiB"), 204_800);
        assert_eq!(bytes("1MiB"), 1_048_576);
        assert_eq!(bytes("1GiB"), 1_073_741_824);
    }

    #[test]
    fn parses_fractional_sizes() {
        assert_eq!(bytes("1.5 MB"), 1_500_000);
        assert_eq!(bytes("0.5kb"), 500);
        assert_eq!(bytes("2.25MiB"), 2_359_296);
    }

    #[test]
    fn parsing_is_case_insensitive() {
        assert_eq!(bytes("1KB"), bytes("1kb"));
        assert_eq!(bytes("1MiB"), bytes("1mib"));
        assert_eq!(bytes("1Gb"), bytes("1gB"));
    }

    #[test]
    fn underscores_are_digit_separators() {
        assert_eq!(bytes("1_000_000"), 1_000_000);
        assert_eq!(bytes("1_0kb"), 10_000);
    }

    #[test]
    fn surrounding_whitespace_is_ignored() {
        assert_eq!(bytes("  180kb  "), 180_000);
        assert_eq!(bytes("180  kb"), 180_000);
    }

    #[test]
    fn rejects_input_without_digits() {
        assert!(matches!(
            parse_byte_size("kb"),
            Err(ByteSizeParseError::Empty { .. })
        ));
        assert!(matches!(
            parse_byte_size(""),
            Err(ByteSizeParseError::Empty { .. })
        ));
        assert!(matches!(
            parse_byte_size("   "),
            Err(ByteSizeParseError::Empty { .. })
        ));
    }

    #[test]
    fn rejects_unknown_units() {
        assert!(matches!(
            parse_byte_size("10tb"),
            Err(ByteSizeParseError::UnknownUnit { .. })
        ));
        assert!(matches!(
            parse_byte_size("10bytes"),
            Err(ByteSizeParseError::UnknownUnit { .. })
        ));
    }

    #[test]
    fn rejects_non_size_characters() {
        // Scientific notation is not a size: the scan stops at `e`, and the
        // digit inside the unit is what makes it invalid.
        assert!(matches!(
            parse_byte_size("1e9kb"),
            Err(ByteSizeParseError::InvalidCharacter { character: '9', .. })
        ));
        assert!(matches!(
            parse_byte_size("10k!"),
            Err(ByteSizeParseError::InvalidCharacter { character: '!', .. })
        ));
        assert!(matches!(
            parse_byte_size("-1kb"),
            Err(ByteSizeParseError::Empty { .. })
        ));
        assert!(matches!(
            parse_byte_size("10 kb extra"),
            Err(ByteSizeParseError::InvalidCharacter { character: ' ', .. })
        ));
    }

    #[test]
    fn rejects_repeated_decimal_points() {
        assert!(matches!(
            parse_byte_size("1.2.3kb"),
            Err(ByteSizeParseError::RepeatedDecimalPoint { .. })
        ));
    }

    #[test]
    fn rejects_values_that_do_not_fit_in_u64() {
        assert!(matches!(
            parse_byte_size("99999999999999999999999"),
            Err(ByteSizeParseError::Overflow { .. })
        ));
        assert!(matches!(
            parse_byte_size("18446744073709551615gb"),
            Err(ByteSizeParseError::Overflow { .. })
        ));
    }

    #[test]
    fn accepts_the_largest_representable_byte_count() {
        assert_eq!(bytes("18446744073709551615"), u64::MAX);
    }

    #[test]
    fn rejects_absurdly_long_input_without_scanning_it() {
        let input = "1".repeat(10_000);
        assert!(matches!(
            parse_byte_size(&input),
            Err(ByteSizeParseError::Overflow { .. })
        ));
    }

    #[test]
    fn error_messages_truncate_untrusted_input() {
        let input = format!("{}tb", "9".repeat(40));
        let error = parse_byte_size(&input).unwrap_err();

        assert!(error.to_string().len() < 120, "{error}");
    }

    #[test]
    fn renders_sizes_in_the_largest_fitting_unit() {
        assert_eq!(ByteSize::from_bytes(512).to_string(), "512 B");
        assert_eq!(ByteSize::from_bytes(1_500).to_string(), "1.50 kB");
        assert_eq!(ByteSize::from_bytes(1_500_000).to_string(), "1.50 MB");
        assert_eq!(ByteSize::from_bytes(2_000_000_000).to_string(), "2.00 GB");
        assert_eq!(ByteSize::from_bytes(0).to_string(), "0 B");
    }

    #[test]
    fn measures_real_compressed_sizes() {
        let contents = "export const value = 1;\n".repeat(500);
        let size = measure(contents.as_bytes()).expect("measures");

        assert_eq!(size.raw.bytes(), contents.len() as u64);
        assert!(
            size.gzip.bytes() < size.raw.bytes(),
            "gzip {} should beat raw {}",
            size.gzip,
            size.raw
        );
        assert!(
            size.brotli.bytes() <= size.gzip.bytes(),
            "brotli {} should beat gzip {} on repetitive input",
            size.brotli,
            size.gzip
        );
    }

    #[test]
    fn measures_empty_and_incompressible_input() {
        let empty = measure(b"").expect("measures");
        assert_eq!(empty.raw.bytes(), 0);

        // Compressing random-ish bytes can grow them; the measurement must
        // report that honestly rather than clamping.
        let noise: Vec<u8> = (0..4096u32)
            .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
            .collect();
        let size = measure(&noise).expect("measures");
        assert_eq!(size.raw.bytes(), 4096);
        assert!(size.gzip.bytes() > 0);
    }

    #[test]
    fn measurement_is_deterministic() {
        let contents = b"const a = 1; const b = 2; const c = 3;".repeat(64);
        let first = measure(&contents).expect("measures");
        let second = measure(&contents).expect("measures");

        assert_eq!(first, second);
    }

    #[test]
    fn metric_selection_reads_the_matching_field() {
        let size = AssetSize {
            raw: ByteSize::from_bytes(300),
            gzip: ByteSize::from_bytes(200),
            brotli: ByteSize::from_bytes(100),
        };

        assert_eq!(size.get(BudgetMetric::Raw).bytes(), 300);
        assert_eq!(size.get(BudgetMetric::Gzip).bytes(), 200);
        assert_eq!(size.get(BudgetMetric::Brotli).bytes(), 100);
    }

    #[test]
    fn sizes_add_without_wrapping() {
        let huge = AssetSize {
            raw: ByteSize::from_bytes(u64::MAX),
            gzip: ByteSize::from_bytes(u64::MAX),
            brotli: ByteSize::from_bytes(u64::MAX),
        };

        let total = huge.saturating_add(huge);

        assert_eq!(total.raw.bytes(), u64::MAX);
    }

    #[test]
    fn gzip_metric_is_the_default() {
        assert_eq!(BudgetMetric::default(), BudgetMetric::Gzip);
    }

    #[test]
    fn deserializes_from_a_human_readable_string() {
        let size: ByteSize = serde_json::from_str("\"180kb\"").expect("deserializes");

        assert_eq!(size.bytes(), 180_000);
    }

    #[test]
    fn deserializes_from_a_raw_byte_count() {
        let size: ByteSize = serde_json::from_str("180000").expect("deserializes");

        assert_eq!(size.bytes(), 180_000);
    }

    #[test]
    fn rejects_a_negative_size() {
        let error = serde_json::from_str::<ByteSize>("-1").expect_err("rejects");

        assert!(error.to_string().contains("negative"), "{error}");
    }

    #[test]
    fn surfaces_the_parse_error_when_deserializing_a_bad_string() {
        let error = serde_json::from_str::<ByteSize>("\"10tb\"").expect_err("rejects");

        assert!(error.to_string().contains("unknown unit"), "{error}");
    }

    #[test]
    fn serializes_back_to_a_plain_byte_count() {
        let json = serde_json::to_string(&ByteSize::from_bytes(180_000)).expect("serializes");

        assert_eq!(json, "180000");
    }

    #[test]
    fn round_trips_through_json() {
        let original = ByteSize::from_bytes(1_234_567);
        let json = serde_json::to_string(&original).expect("serializes");
        let parsed: ByteSize = serde_json::from_str(&json).expect("deserializes");

        assert_eq!(original, parsed);
    }
}
