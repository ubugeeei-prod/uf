use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use compact_str::CompactString;
use uf_runtime::RuntimeStandard;

use super::*;

/// This checkout, found by a file rather than by counting `..`.
fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the crate is inside the repository")
}

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
    // The six of ubugeeei-prod/uf#711 plus helpers from #710's next tranche,
    // named here as well as held to
    // `packages/std` by `crates/uf_lib`: this list is what a reader checks
    // first, and a shipped module missing from it is the drift #710 is about.
    for shipped in [
        "@uniflowed/std/errors",
        "@uniflowed/std/sync",
        "@uniflowed/std/context",
        "@uniflowed/std/bytes",
        "@uniflowed/std/heap",
        "@uniflowed/std/hex",
        "@uniflowed/std/base32",
        "@uniflowed/std/slices",
        "@uniflowed/std/list",
        "@uniflowed/std/csv",
        "@uniflowed/std/binary",
        "@uniflowed/std/path",
        "@uniflowed/std/glob",
        "@uniflowed/std/hash",
        "@uniflowed/std/time",
        "@uniflowed/std/io",
        "@uniflowed/std/bufio",
    ] {
        let module = modules
            .iter()
            .find(|module| module.specifier == shipped)
            .unwrap_or_else(|| panic!("the registry names {shipped}"));
        assert_eq!(module.status, StdStatus::Ships, "{shipped}");
    }
    assert_eq!(std_runtime_standard(), RuntimeStandard::WinterTc);
}

/// One entry per specifier, and every one of them under this package.
///
/// The table is grouped by [`StdStatus`] rather than sorted, so a duplicate is
/// a hundred lines away from its twin and reads as a different entry. Two rows
/// for one specifier would let `uf inspect` count it twice and let a status
/// change land on the copy nobody reads.
#[test]
fn every_std_specifier_appears_once_and_is_under_the_package() {
    let modules = std_modules();
    let mut seen = BTreeSet::new();

    for module in modules.iter() {
        assert!(
            module.specifier == "@uniflowed/std" || module.specifier.starts_with("@uniflowed/std/"),
            "{} is not a @uniflowed/std specifier",
            module.specifier
        );
        assert!(
            seen.insert(module.specifier.clone()),
            "{} is in the registry twice",
            module.specifier
        );
    }
    assert_eq!(seen.len(), modules.len());
}

/// Only a module that ships claims to have been read.
///
/// `wintertcAligned` was `true` on every entry because one constructor set it,
/// which made it a claim about `@uniflowed/std/net` — whose stated surface is
/// `TcpListener` and `UdpSocket` — as loudly as about the six modules somebody
/// wrote. The flag now means "this file was read and imports no host", and
/// `a_std_module_that_claims_wintertc_alignment_names_no_host` in
/// `crates/uf_lib` does the reading. This is the half of it that needs no
/// checkout: a module with no file cannot have been read.
#[test]
fn only_a_shipping_std_module_claims_wintertc_alignment() {
    for module in std_modules() {
        assert_eq!(
            module.wintertc_aligned,
            module.status == StdStatus::Ships,
            "{} claims wintertcAligned={} at status {:?}",
            module.specifier,
            module.wintertc_aligned,
            module.status
        );
    }
}

/// The root is the only declaration surface, and it names its exports
/// elsewhere.
///
/// `uf_lib::builtin_modules()` carries `@uniflowed/std`'s forty-odd names and
/// `the_registry_names_exactly_what_each_package_exports` holds that list to
/// the file. Repeating it here would be a second place with the same names in
/// it, which is exactly how this table came to say `["modules", "wintertc",
/// "native"]` about a module that exports forty-two things, one of which is
/// not `native`.
#[test]
fn the_root_declaration_surface_names_no_exports_here() {
    let modules = std_modules();
    let declared: Vec<&StdModule> = modules
        .iter()
        .filter(|module| module.status == StdStatus::Declared)
        .collect();

    assert_eq!(declared.len(), 1);
    assert_eq!(declared[0].specifier, "@uniflowed/std");
    assert!(declared[0].exports.is_empty());
}

/// Nothing claims a native binding while there is no binding to cross.
///
/// `nativeBinding` was `true` on all forty-five entries and had never been
/// looked at: the workspace has no N-API crate and no `wasm-bindgen`, so
/// "native" for a `@uniflowed/std` module is not a build flag, it is a layer
/// that does not exist. ubugeeei-prod/uf#710 measured what one would buy and
/// found `bytes.equal` on sixteen bytes *faster* in JavaScript than Node's own
/// C++ `Buffer#equals`, because comparing sixteen bytes costs less than
/// crossing into C++ to do it.
///
/// So the flag is held to the workspace rather than to intent. This fails the
/// day somebody sets it without adding the layer — and the day the layer
/// arrives it stops applying, which is when the argument for the first native
/// module has to be made in the open, with the boundary cost in it and a host
/// matrix beside it. N-API runs on Node and Bun and not on Deno or the edge,
/// which is red line 6.
#[test]
fn no_std_module_claims_a_binding_this_workspace_does_not_have() {
    let root = repository_root();
    let mut manifests = vec![root.join("Cargo.toml")];
    for entry in std::fs::read_dir(root.join("crates")).expect("crates/ is readable") {
        let path = entry.expect("a readable directory entry").path();
        let manifest = path.join("Cargo.toml");
        if manifest.is_file() {
            manifests.push(manifest);
        }
    }
    assert!(
        manifests.len() > 10,
        "the walk found almost nothing, so it is not checking anything: {}",
        manifests.len()
    );

    let bindings: Vec<String> = manifests
        .iter()
        .filter(|manifest| {
            let text = std::fs::read_to_string(manifest).unwrap_or_default();
            text.lines().any(|line| {
                let name = line.split('=').next().unwrap_or_default().trim();
                name == "napi" || name == "napi-build" || name == "wasm-bindgen"
            })
        })
        .map(|manifest| manifest.display().to_string())
        .collect();

    if bindings.is_empty() {
        for module in std_modules() {
            assert!(
                !module.native_binding,
                "{} claims a native binding, and this workspace has none to cross",
                module.specifier
            );
        }
    } else {
        // Not a failure: it is the day the argument becomes possible. What is
        // not allowed is the flag going true quietly beside it.
        panic!(
            "this workspace now has a binding crate ({}), so ubugeeei-prod/uf#710's \
             native question can be reopened — with a measurement and a host matrix. \
             Update this test when it is.",
            bindings.join(", ")
        );
    }
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
    assert_eq!(join_path(&["app/", "/$page.js"]), "app/$page.js");
    assert_eq!(normalize_path("app/./server/../$page.js"), "app/$page.js");
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
    assert!(glob.matches("app/$page.js"));
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
