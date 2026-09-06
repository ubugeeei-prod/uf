use super::*;
use base64::Engine as _;
use camino::{Utf8Path, Utf8PathBuf};
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

/// The packer, end to end: a build directory in, a module a bundler can read
/// out, with the bytes intact.
///
/// The round trip is the assertion that matters. A generated module is only
/// correct if the runtime that loads it gets back exactly the bytes that were
/// on disk, and the two escaping passes between here and there — base64, then
/// JSON inside a JavaScript string literal — are each a place where a quote or
/// a backslash could be lost. So the test parses the module the way the
/// runtime will, rather than checking that the text looks about right.
#[test]
fn packs_a_build_directory_into_a_module_that_reads_back_byte_for_byte() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf-8");
    std::fs::create_dir_all(root.join("assets")).expect("mkdir");

    // A document, a chunk, and something that is not text at all — the last
    // one is why the payload is base64 and not a string: these bytes are not
    // valid UTF-8 and would not survive being treated as characters.
    let html = "<!doctype html><p>a \"quoted\" \\ backslash</p>\n";
    let png = [
        0x89u8, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0xff, 0xfe, 0x00,
    ];
    std::fs::write(root.join("index.html"), html).expect("write");
    std::fs::write(root.join("assets/app-a1b2.js"), "export const a = 1;\n").expect("write");
    std::fs::write(root.join("assets/logo.png"), png).expect("write");
    // Build metadata, which the size report skips and the binary must skip too.
    std::fs::write(root.join("uf-build-manifest.json"), "{}").expect("write");

    let dest = root.join(".uf/assets.js");
    let embedded = write_embedded_assets(&root, &ReportOptions::default(), &dest).expect("packs");

    assert_eq!(embedded.files, 3, "the manifest is metadata, not an asset");
    assert_eq!(
        embedded.bytes,
        (html.len() + "export const a = 1;\n".len() + png.len()) as u64
    );

    let module = std::fs::read_to_string(&dest).expect("reads");
    assert!(
        !module.contains("uf-build-manifest.json"),
        "build metadata must not travel inside the binary:\n{module}"
    );

    // Parse it the way the runtime does: pull the JSON string literal back out
    // of the module and hand it to a JSON parser.
    let literal = module
        .split_once("JSON.parse(")
        .and_then(|(_, rest)| rest.rsplit_once(");"))
        .map(|(literal, _)| literal.trim().to_owned())
        .unwrap_or_else(|| panic!("the module must be one `JSON.parse` call:\n{module}"));
    let json: String = serde_json::from_str(&literal).expect("the literal is a JSON string");
    let payload: serde_json::Value = serde_json::from_str(&json).expect("and it holds JSON");

    let decode = |path: &str| -> Vec<u8> {
        let body = payload[path]["body"]
            .as_str()
            .unwrap_or_else(|| panic!("no {path} in {payload}"));
        base64::engine::general_purpose::STANDARD
            .decode(body)
            .expect("decodes")
    };
    assert_eq!(decode("index.html"), html.as_bytes());
    assert_eq!(decode("assets/logo.png"), png);
    assert_eq!(
        payload["index.html"]["type"],
        serde_json::json!("text/html; charset=utf-8")
    );
    assert_eq!(
        payload["assets/app-a1b2.js"]["type"],
        serde_json::json!("text/javascript; charset=utf-8")
    );
    assert_eq!(
        payload["assets/logo.png"]["type"],
        serde_json::json!("image/png")
    );
}

/// A name the caller adds to the exclusion list does not travel.
///
/// This is how `uf build --compile` keeps the executable it is about to write
/// out of the executable it is writing. Without it, a project whose Vite
/// config turns `emptyOutDir` off compiles the previous run's binary into this
/// run's, and the file doubles in size on every build.
#[test]
fn an_excluded_name_stays_out_of_the_payload() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf-8");
    std::fs::write(root.join("index.html"), "<!doctype html>").expect("write");
    std::fs::write(root.join("docs"), "a previous run's 60 MB binary").expect("write");

    let options = ReportOptions {
        excluded: vec![CompactString::const_new("docs")],
    };
    let dest = root.join(".uf/assets.js");
    let embedded = write_embedded_assets(&root, &options, &dest).expect("packs");

    assert_eq!(embedded.files, 1);
    let module = std::fs::read_to_string(&dest).expect("reads");
    assert!(!module.contains("previous run"), "{module}");
}

/// Every extension the build can emit has a type, and the unknown ones are
/// named as unknown rather than guessed at.
#[test]
fn a_content_type_is_decided_by_extension_and_never_guessed() {
    for (path, expected) in [
        ("a/b/index.html", "text/html; charset=utf-8"),
        ("assets/app.JS", "text/javascript; charset=utf-8"),
        ("assets/app.js.map", "application/json; charset=utf-8"),
        ("brand/mark.svg", "image/svg+xml"),
        ("brand/Inter.woff2", "font/woff2"),
        ("data/model.bin", "application/octet-stream"),
        ("LICENSE", "application/octet-stream"),
    ] {
        assert_eq!(
            content_type(Utf8Path::new(path)),
            expected,
            "wrong type for {path}"
        );
    }
}
