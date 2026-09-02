use compact_str::CompactString;
use uf_runtime::RuntimeStandard;

use super::*;

#[test]
fn std_registry_covers_requested_modules() {
    let modules = std_modules();
    let specifiers = modules
        .iter()
        .map(|module| module.specifier.as_str())
        .collect::<Vec<_>>();

    assert!(specifiers.contains(&"@uniflowed/std/vfs"));
    assert!(specifiers.contains(&"@uniflowed/std/fs"));
    assert!(specifiers.contains(&"@uniflowed/std/types"));
    assert!(specifiers.contains(&"@uniflowed/std/pipeline"));
    assert!(specifiers.contains(&"@uniflowed/std/http"));
    assert!(specifiers.contains(&"@uniflowed/std/ws"));
    assert!(specifiers.contains(&"@uniflowed/std/sql"));
    assert!(specifiers.contains(&"@uniflowed/std/collections"));
    assert!(specifiers.contains(&"@uniflowed/std/crypto"));
    assert!(specifiers.contains(&"@uniflowed/std/dotenv"));
    assert!(specifiers.contains(&"@uniflowed/std/math"));
    assert!(specifiers.contains(&"@uniflowed/std/os"));
    assert!(specifiers.contains(&"@uniflowed/std/net"));
    assert!(specifiers.contains(&"@uniflowed/std/dns"));
    assert!(specifiers.contains(&"@uniflowed/std/path"));
    assert!(specifiers.contains(&"@uniflowed/std/stream"));
    assert!(specifiers.contains(&"@uniflowed/std/url"));
    assert!(specifiers.contains(&"@uniflowed/std/wasm"));
    assert!(specifiers.contains(&"@uniflowed/std/glob"));
    assert!(specifiers.contains(&"@uniflowed/std/motion"));
    assert!(specifiers.contains(&"@uniflowed/std/tui"));
    assert!(specifiers.contains(&"@uniflowed/std/cron"));
    assert!(specifiers.contains(&"@uniflowed/std/s3"));
    assert!(specifiers.contains(&"@uniflowed/std/sigv4"));
    assert!(specifiers.contains(&"@uniflowed/std/functions"));
    assert!(specifiers.contains(&"@uniflowed/std/uuid"));
    assert!(specifiers.contains(&"@uniflowed/std/zip"));
    assert!(specifiers.contains(&"@uniflowed/std/import-meta"));
    assert!(specifiers.contains(&"@uniflowed/std/defer"));
    assert!(modules.iter().all(|module| module.wintertc_aligned));
    assert_eq!(std_runtime_standard(), RuntimeStandard::WinterTc);
}

#[test]
fn query_string_round_trips_ordered_pairs() {
    let pairs = parse_query("?name=uf&space=flow+react");

    assert_eq!(pairs[0].key, "name");
    assert_eq!(pairs[1].value, "flow react");
    assert_eq!(stringify_query(&pairs), "name=uf&space=flow+react");
}

#[test]
fn hash_and_equality_are_deterministic() {
    let left = ByteBuffer::from_utf8("uniflowed");
    let right = ByteBuffer::from_bytes(b"uniflowed");

    assert_eq!(left.to_hex(), "756e69666c6f776564");
    assert_eq!(
        fast_hash_str("uniflowed"),
        fast_hash_bytes(right.as_slice())
    );
    assert!(constant_time_equal(left.as_slice(), right.as_slice()));
    assert!(!constant_time_equal(left.as_slice(), b"flow"));
}

#[test]
fn parses_json_and_toml_and_detects_yaml_shape() {
    let json = parse_json(r#"{ "name": "uf" }"#).unwrap();
    let toml = parse_toml("name = \"uf\"").unwrap();

    assert_eq!(json.value["name"], "uf");
    assert_eq!(
        minify_json(r#"{ "name": "uf" }"#).unwrap(),
        r#"{"name":"uf"}"#
    );
    assert_eq!(toml.value["name"].as_str(), Some("uf"));
    assert_eq!(detect_yaml("- name: uf"), YamlDocumentKind::Sequence);
    assert_eq!(detect_yaml("name: uf"), YamlDocumentKind::Mapping);
}

#[test]
fn diagnostics_and_pipeline_contracts_are_lightweight() {
    let pipeline = LazyPipeline::new("docs")
        .then(PipelineStep::Map)
        .then(PipelineStep::Collect);
    let event = debug_event("uf:std", "ready");

    assert_eq!(VirtualPath::new("app\\page.js").path, "app/page.js");
    assert_eq!(pipeline.steps.len(), 2);
    assert!(colorize("ok", AnsiStyle::Green, true).starts_with("\x1b[32m"));
    assert_eq!(colorize("ok", AnsiStyle::Green, false), "ok");
    assert_eq!(event.channel, "uf:std");
}

#[test]
fn import_meta_and_defer_are_explicit_contracts() {
    let meta = ImportMeta::new("file:///repo/app.js").with_file("/repo", "/repo/app.js");
    let task = DeferredTask::new("render-post-response", DeferPhase::PostResponse);

    assert_eq!(meta.dirname.as_deref(), Some("/repo"));
    assert_eq!(task.phase, DeferPhase::PostResponse);
}

#[test]
fn collections_math_path_and_dotenv_helpers_are_deterministic() {
    let items = [1, 2, 3, 4, 5];
    let chunks = chunk(&items, 2);
    let env = parse_dotenv("UF_ENV=dev\nQUOTED=\"flow\"\n# ignored");

    assert_eq!(chunks.len(), 3);
    assert_eq!(clamp(12, 0, 10), 10);
    assert_eq!(lerp(10.0, 20.0, 0.25), 12.5);
    assert_eq!(join_path(&["app/", "/_uf.page.js"]), "app/_uf.page.js");
    assert_eq!(
        normalize_path("app/./server/../_uf.page.js"),
        "app/_uf.page.js"
    );
    assert_eq!(env[0].key, "UF_ENV");
    assert_eq!(env[1].value, "flow");
}

#[test]
fn platform_cloud_and_motion_contracts_are_typed() {
    let os = OsInfo::new(OsFamily::MacOs, "aarch64", 10);
    let dns = DnsQuery::new("uniflowed.dev", DnsRecordType::Aaaa);
    let stream = StreamDescriptor::new(StreamKind::Transform);
    let url = parse_url("https://setup.uniflowed.dev/install.sh").unwrap();
    let wasm = WasmModulePlan::new("ox-content");
    let glob = GlobPattern::new("app/*.js");
    let motion = MotionTransition::new(120, MotionEase::Spring);
    let cron = parse_cron("0 * * * *").unwrap();
    let s3 = S3ObjectRequest::new("uf-releases", "uf@0.1.0.tar.gz");
    let sigv4 = SigV4Scope::new("ap-northeast-1", "s3");
    let function = FunctionDescriptor::new(
        "docs-render",
        FunctionRuntime::Worker,
        "server/functions/docs.js",
    );

    assert_eq!(os.family, OsFamily::MacOs);
    assert_eq!(dns.record_type, DnsRecordType::Aaaa);
    assert!(stream.backpressure);
    assert_eq!(url.host, "setup.uniflowed.dev");
    assert!(wasm.ahead_of_time);
    assert!(glob.matches("app/_uf.page.js"));
    assert!(motion.respects_reduced_motion);
    assert_eq!(cron.minute, "0");
    assert!(s3.sigv4);
    assert_eq!(sigv4.service, "s3");
    assert_eq!(function.runtime, FunctionRuntime::Worker);
}

#[test]
fn terminal_capabilities_are_high_fidelity_by_default() {
    let capabilities = terminal_capabilities(120, 36).with_inline_images();

    assert_eq!(capabilities.columns, 120);
    assert_eq!(capabilities.rows, 36);
    assert_eq!(capabilities.color_depth, TerminalColorDepth::TrueColor);
    assert!(capabilities.high_fidelity());
    assert!(capabilities.mouse);
    assert!(capabilities.inline_images);
}

#[test]
fn crypto_uuid_and_zip_contracts_are_native_ready() {
    let digest = digest_bytes(DigestAlgorithm::FastHash, b"uf");
    let sha256 = digest_bytes(DigestAlgorithm::Sha256, b"");
    let uuid = parse_uuid("018f7c9a-7cb4-7a10-a7aa-1df490512a88").unwrap();
    let entry = ZipEntry {
        path: CompactString::const_new("app.js"),
        compression: ZipCompression::Deflate,
        size: 42,
    };

    assert_eq!(digest.len(), 16);
    assert_eq!(
        sha256,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(uuid.version, Some(7));
    assert_eq!(entry.compression, ZipCompression::Deflate);
}
