use super::*;
use crate::size::{BudgetMetric, ByteSize};

fn write(root: &Utf8Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("has parent")).expect("creates dirs");
    fs::write(path, contents).expect("writes");
}

fn temp_root() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf-8 temp dir");
    (dir, root)
}

#[test]
fn classifies_assets_by_extension() {
    assert_eq!(
        AssetKind::from_path(Utf8Path::new("a/app.js")),
        AssetKind::JavaScript
    );
    assert_eq!(
        AssetKind::from_path(Utf8Path::new("a/app.mjs")),
        AssetKind::JavaScript
    );
    assert_eq!(
        AssetKind::from_path(Utf8Path::new("a/app.css")),
        AssetKind::Stylesheet
    );
    assert_eq!(
        AssetKind::from_path(Utf8Path::new("a/index.html")),
        AssetKind::Html
    );
    assert_eq!(
        AssetKind::from_path(Utf8Path::new("a/data.json")),
        AssetKind::Json
    );
    assert_eq!(
        AssetKind::from_path(Utf8Path::new("a/font.woff2")),
        AssetKind::Other
    );
}

#[test]
fn source_maps_win_over_the_extension_they_end_in() {
    assert_eq!(
        AssetKind::from_path(Utf8Path::new("a/app.js.map")),
        AssetKind::SourceMap
    );
    assert_eq!(
        AssetKind::from_path(Utf8Path::new("a/app.css.map")),
        AssetKind::SourceMap
    );
    assert!(!AssetKind::SourceMap.is_downloaded());
    assert!(AssetKind::JavaScript.is_downloaded());
}

#[test]
fn collects_and_sorts_assets_from_the_output_directory() {
    let (_dir, root) = temp_root();
    write(&root, "assets/b.js", "const b = 2;\n");
    write(&root, "assets/a.js", "const a = 1;\n");
    write(&root, "index.html", "<!doctype html>\n");

    let assets = collect_assets(&root, &ReportOptions::default()).expect("collects");

    let paths = assets.iter().map(|a| a.path.as_str()).collect::<Vec<_>>();
    assert_eq!(paths, ["assets/a.js", "assets/b.js", "index.html"]);
}

#[test]
fn skips_build_metadata_by_default() {
    let (_dir, root) = temp_root();
    write(&root, "uf-build-manifest.json", "{}\n");
    write(&root, "uf-rsc-manifest.json", "{}\n");
    write(&root, "uf-bundle-report.json", "{}\n");
    write(&root, "app.js", "const a = 1;\n");

    let assets = collect_assets(&root, &ReportOptions::default()).expect("collects");

    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0].path, "app.js");
}

#[test]
fn a_missing_output_directory_reports_no_assets() {
    let (_dir, root) = temp_root();

    let assets =
        collect_assets(&root.join("does-not-exist"), &ReportOptions::default()).expect("ok");

    assert!(assets.is_empty());
}

#[cfg(unix)]
#[test]
fn symlinks_are_neither_measured_nor_followed() {
    let (_dir, root) = temp_root();
    write(&root, "app.js", "const a = 1;\n");
    std::os::unix::fs::symlink("/etc/passwd", root.join("leak.js")).expect("symlink");
    std::os::unix::fs::symlink(root.as_std_path(), root.join("loop")).expect("symlink");

    let assets = collect_assets(&root, &ReportOptions::default()).expect("collects");

    let paths = assets.iter().map(|a| a.path.as_str()).collect::<Vec<_>>();
    assert_eq!(paths, ["app.js"]);
}

#[test]
fn deep_trees_terminate_instead_of_recursing() {
    let (_dir, root) = temp_root();
    let mut nested = String::new();
    for _ in 0..(MAX_ASSET_DEPTH + 8) {
        nested.push_str("d/");
    }
    write(&root, &format!("{nested}deep.js"), "const a = 1;\n");
    write(&root, "shallow.js", "const b = 2;\n");

    let assets = collect_assets(&root, &ReportOptions::default()).expect("collects");

    assert!(assets.iter().any(|a| a.path == "shallow.js"));
    assert!(assets.iter().all(|a| a.path != format!("{nested}deep.js")));
}

#[test]
fn route_weight_splits_initial_from_lazy_javascript() {
    let (_dir, root) = temp_root();
    write(&root, "entry.js", &"const a = 1;\n".repeat(20));
    write(&root, "lazy.js", &"const b = 2;\n".repeat(40));
    write(&root, "app.css", &".a { color: red }\n".repeat(10));

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

    let route = &report.routes[0];
    assert!(route.initial_js.raw.bytes() > 0);
    assert!(route.lazy_js.raw.bytes() > route.initial_js.raw.bytes());
    // Stylesheets count toward the route total but never toward initial JS.
    assert!(route.total.raw.bytes() > route.initial_js.raw.bytes() + route.lazy_js.raw.bytes());
    assert_eq!(route.assets.len(), 3);
}

#[test]
fn source_maps_are_measured_but_excluded_from_totals() {
    let (_dir, root) = temp_root();
    write(&root, "app.js", "const a = 1;\n");
    write(&root, "app.js.map", &"x".repeat(4096));

    let assets = collect_assets(&root, &ReportOptions::default()).expect("collects");
    let report = build_report(assets, &[]);

    assert_eq!(report.assets.len(), 2);
    let app_js = report
        .assets
        .iter()
        .find(|a| a.path == "app.js")
        .expect("app.js");
    assert_eq!(report.total, app_js.size);
}

#[test]
fn unknown_route_assets_are_ignored_rather_than_guessed() {
    let report = build_report(
        Vec::new(),
        &[(
            CompactString::const_new("/"),
            vec![CompactString::const_new("missing.js")],
            Vec::new(),
        )],
    );

    assert_eq!(report.routes[0].initial_js, AssetSize::zero());
    assert_eq!(report.routes[0].total, AssetSize::zero());
}

#[test]
fn an_asset_shared_by_entry_and_lazy_is_counted_once_in_the_total() {
    let (_dir, root) = temp_root();
    write(&root, "shared.js", &"const a = 1;\n".repeat(20));

    let assets = collect_assets(&root, &ReportOptions::default()).expect("collects");
    let shared = assets[0].size;
    let report = build_report(
        assets,
        &[(
            CompactString::const_new("/"),
            vec![CompactString::const_new("shared.js")],
            vec![CompactString::const_new("shared.js")],
        )],
    );

    assert_eq!(report.routes[0].total, shared);
    assert_eq!(report.routes[0].assets.len(), 1);
}

#[test]
fn routes_are_sorted_by_path() {
    let report = build_report(
        Vec::new(),
        &[
            (CompactString::const_new("/z"), Vec::new(), Vec::new()),
            (CompactString::const_new("/a"), Vec::new(), Vec::new()),
        ],
    );

    assert_eq!(report.routes[0].path, "/a");
    assert_eq!(report.routes[1].path, "/z");
}

#[test]
fn largest_assets_are_ordered_by_the_requested_metric() {
    let (_dir, root) = temp_root();
    write(&root, "small.js", "const a = 1;\n");
    write(&root, "large.js", &"const b = 2;\n".repeat(200));

    let assets = collect_assets(&root, &ReportOptions::default()).expect("collects");
    let report = build_report(assets, &[]);

    let ordered = report.largest_assets(BudgetMetric::Gzip);
    assert_eq!(ordered[0].path, "large.js");
    assert_eq!(ordered[1].path, "small.js");
}

#[test]
fn totals_group_by_asset_kind() {
    let (_dir, root) = temp_root();
    write(&root, "a.js", "const a = 1;\n");
    write(&root, "b.js", "const b = 2;\n");
    write(&root, "a.css", ".a{}\n");

    let assets = collect_assets(&root, &ReportOptions::default()).expect("collects");
    let report = build_report(assets, &[]);
    let totals = report.total_by_kind();

    assert_eq!(totals.len(), 2);
    assert!(totals.contains_key(&AssetKind::JavaScript));
    assert!(totals.contains_key(&AssetKind::Stylesheet));
}

#[test]
fn the_written_report_is_deterministic_and_carries_its_compression_settings() {
    let (_dir, root) = temp_root();
    write(&root, "app.js", "const a = 1;\n");

    let assets = collect_assets(&root, &ReportOptions::default()).expect("collects");
    let report = build_report(assets, &[]);
    let path = write_report(&root, &report).expect("writes");
    let first = fs::read_to_string(&path).expect("reads");
    let second_path = write_report(&root, &report).expect("writes");
    let second = fs::read_to_string(&second_path).expect("reads");

    assert_eq!(first, second);
    assert!(first.contains("\"gzipLevel\": 9"), "{first}");
    assert!(first.contains("\"brotliQuality\": 11"), "{first}");
    assert!(first.contains("\"initialJs\"") || report.routes.is_empty());
    assert!(first.ends_with('\n'));
}

#[test]
fn report_json_uses_camel_case_keys() {
    let report = build_report(
        Vec::new(),
        &[(CompactString::const_new("/"), Vec::new(), Vec::new())],
    );

    let json = serde_json::to_string(&report).expect("serializes");

    assert!(json.contains("\"initialJs\""), "{json}");
    assert!(json.contains("\"lazyJs\""), "{json}");
    assert!(!json.contains("initial_js"), "{json}");
}

#[test]
fn an_empty_build_reports_zero() {
    let report = BundleReport::default();

    assert_eq!(report.total, AssetSize::zero());
    assert_eq!(
        report.total.get(BudgetMetric::Gzip),
        ByteSize::from_bytes(0)
    );
}
