use super::*;
use camino::Utf8PathBuf;
use compact_str::CompactString;

/// The whole point of the crate, end to end: measure a build directory,
/// attribute assets to a route, and fail the declared budget.
#[test]
fn measures_a_build_and_reports_every_budget_it_breaks() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf-8");
    std::fs::write(root.join("entry.js"), "export const a = 1;\n".repeat(400)).expect("write");
    std::fs::write(root.join("lazy.js"), "export const b = 2;\n".repeat(200)).expect("write");
    std::fs::write(root.join("app.css"), ".a { color: red }\n".repeat(50)).expect("write");

    let assets = collect_assets(&root, &ReportOptions::default()).expect("collects");
    let report = build_report(
        assets,
        &[(
            CompactString::const_new("/"),
            vec![
                CompactString::const_new("entry.js"),
                CompactString::const_new("app.css"),
            ],
            vec![CompactString::const_new("lazy.js")],
        )],
    );

    let generous = BundleBudgets {
        total: Some(SizeBudget::new(ByteSize::from_bytes(1_000_000))),
        ..BundleBudgets::default()
    };
    assert!(evaluate(&report, &generous).is_within_budget());

    let strict = BundleBudgets {
        total: Some(SizeBudget::new(ByteSize::from_bytes(64))),
        initial_js: Some(SizeBudget::new(ByteSize::from_bytes(16))),
        ..BundleBudgets::default()
    };
    let outcome = evaluate(&report, &strict);

    assert_eq!(outcome.violations.len(), 2);
    assert!(outcome.violations.iter().all(|v| v.overage.bytes() > 0));

    let written = write_report(&root, &report).expect("writes");
    assert!(written.exists());
}

#[test]
fn a_budget_string_from_config_round_trips_into_an_enforced_ceiling() {
    let budget = SizeBudget::new(parse_byte_size("180 kB").expect("parses"));

    assert_eq!(budget.max.bytes(), 180_000);
    assert_eq!(budget.metric, BudgetMetric::Gzip);
    assert!(budget.admits(AssetSize {
        raw: ByteSize::from_bytes(600_000),
        gzip: ByteSize::from_bytes(180_000),
        brotli: ByteSize::from_bytes(150_000),
    }));
    assert!(!budget.admits(AssetSize {
        raw: ByteSize::from_bytes(600_000),
        gzip: ByteSize::from_bytes(180_001),
        brotli: ByteSize::from_bytes(150_000),
    }));
}
