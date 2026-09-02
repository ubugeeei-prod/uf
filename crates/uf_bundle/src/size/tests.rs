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
