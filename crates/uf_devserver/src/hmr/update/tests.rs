//! The update payload, its targets, and the door it fetches through.

use camino::Utf8Path;

use super::*;
use crate::hmr::invalidate::{ChangeKind, ReloadReason, UpdateKind};
use crate::resolve::{AccessDenied, resolve_request};

fn target(path: &str, revision: u32) -> Option<String> {
    update_target(Utf8Path::new(path), revision).map(|value| value.to_string())
}

fn fixture() -> (tempfile::TempDir, camino::Utf8PathBuf, FsPolicy) {
    let dir = tempfile::tempdir().expect("temp dir");
    let root =
        camino::Utf8PathBuf::from_path_buf(dir.path().canonicalize().expect("canonical")).unwrap();
    std::fs::create_dir_all(root.join("app")).expect("app dir");
    std::fs::write(root.join("app/Counter.js"), "export const a = 1;\n").expect("module");
    std::fs::write(root.join("app/café.js"), "export const b = 2;\n").expect("module");
    std::fs::write(root.join(".env"), "SECRET=1\n").expect("secret");
    let policy = FsPolicy::with_defaults(&root).expect("policy");
    (dir, root, policy)
}

#[test]
fn an_update_target_is_an_origin_form_path_with_a_cache_busting_key() {
    assert_eq!(
        target("app/Counter.js", 7).as_deref(),
        Some("/app/Counter.js?t=7")
    );
}

#[test]
fn an_update_target_at_revision_zero_still_carries_the_key() {
    assert_eq!(target("a.js", 0).as_deref(), Some("/a.js?t=0"));
}

#[test]
fn an_update_target_percent_encodes_everything_outside_the_unreserved_set() {
    assert_eq!(
        target("app/a file.js", 1).as_deref(),
        Some("/app/a%20file.js?t=1")
    );
    assert_eq!(
        target("app/café.js", 1).as_deref(),
        Some("/app/caf%C3%A9.js?t=1")
    );
}

#[test]
fn an_update_target_keeps_the_unreserved_characters_verbatim() {
    assert_eq!(
        target("a-b._~/c.js", 2).as_deref(),
        Some("/a-b._~/c.js?t=2")
    );
}

#[test]
fn an_update_target_refuses_an_absolute_module_path() {
    assert_eq!(target("/etc/passwd", 1), None);
}

#[test]
fn an_update_target_refuses_an_empty_module_path() {
    assert_eq!(target("", 1), None);
}

#[test]
fn an_update_target_refuses_a_path_with_a_dot_or_traversal_segment() {
    for path in [
        "../.env",
        "../../.env",
        "app/../../.env",
        "app/./main.js",
        "app//main.js",
        "app/",
    ] {
        assert_eq!(target(path, 1), None, "{path} must have no update target");
    }
}

#[test]
fn an_update_target_refuses_a_path_carrying_a_percent_escape() {
    // Encoding the `%` would produce a target that decodes into something still
    // encoded, which `crate::resolve` refuses as double-encoding. Refusing here
    // keeps the two ends agreeing.
    assert_eq!(target("app/50%AB.js", 1), None);
}

#[test]
fn an_update_target_refuses_a_path_longer_than_the_bound() {
    let long = format!("app/{}.js", "a".repeat(MAX_UPDATE_TARGET_BYTES));

    assert_eq!(target(&long, 1), None);
}

#[test]
fn an_update_target_encodes_a_newline_rather_than_carrying_one() {
    let built = target("app/a\nb.js", 1).expect("encodable");

    assert!(built.contains("%0A"));
    assert!(!built.contains('\n'));
}

#[test]
fn an_update_target_encodes_a_question_mark_so_it_cannot_add_a_query() {
    let built = target("app/a?raw.js", 1).expect("encodable");

    assert_eq!(built, "/app/a%3Fraw.js?t=1");
}

#[test]
fn an_update_target_encodes_a_hash_so_it_cannot_add_a_fragment() {
    let built = target("app/a#b.js", 1).expect("encodable");

    assert_eq!(built, "/app/a%23b.js?t=1");
}

#[test]
fn every_built_target_parses_under_the_request_grammar() {
    for path in [
        "a.js",
        "app/Counter.js",
        "app/a file.js",
        "app/café.js",
        "deep/nested/tree/of/modules/index.js",
        "app/a\tb.js",
        "app/a\"b.js",
    ] {
        let built = target(path, 3).expect("encodable");
        assert!(
            RequestTarget::parse(&built).is_ok(),
            "{built} must be origin-form"
        );
    }
}

#[test]
fn a_built_target_fetches_the_module_it_names() {
    let (_dir, _root, policy) = fixture();
    let built = target("app/Counter.js", 1).expect("encodable");

    let file = fetch_update(&policy, &built).expect("the module is servable");

    assert_eq!(file.read().expect("read"), b"export const a = 1;\n");
}

#[test]
fn a_built_target_for_a_non_ascii_name_fetches_that_file() {
    let (_dir, _root, policy) = fixture();
    let built = target("app/café.js", 1).expect("encodable");

    let file = fetch_update(&policy, &built).expect("the module is servable");

    assert_eq!(file.read().expect("read"), b"export const b = 2;\n");
}

#[test]
fn an_hmr_fetch_for_a_traversing_path_is_refused_exactly_like_a_plain_request() {
    let (_dir, root, policy) = fixture();

    let over_hmr = fetch_update(&policy, "/../../.env").unwrap_err();
    let over_the_plain_path = resolve_request(&root, "/../../.env").unwrap_err();

    assert_eq!(over_hmr, over_the_plain_path);
    assert_eq!(over_hmr, AccessDenied::Escape);
}

#[test]
fn an_hmr_fetch_for_a_denied_file_is_refused_exactly_like_a_plain_request() {
    let (_dir, root, policy) = fixture();

    let over_hmr = fetch_update(&policy, "/.env").unwrap_err();
    let over_the_plain_path = resolve_request(&root, "/.env").unwrap_err();

    assert_eq!(over_hmr, over_the_plain_path);
}

#[test]
fn an_hmr_fetch_refuses_the_filesystem_prefix() {
    let (_dir, _root, policy) = fixture();

    assert_eq!(
        fetch_update(&policy, "/@fs/etc/passwd").unwrap_err(),
        AccessDenied::FilesystemPrefix
    );
}

#[test]
fn an_hmr_fetch_refuses_a_double_encoded_path() {
    let (_dir, _root, policy) = fixture();

    assert_eq!(
        fetch_update(&policy, "/%252e%252e/.env").unwrap_err(),
        AccessDenied::DoubleEncoded
    );
}

#[test]
fn an_hmr_fetch_refuses_a_target_that_is_not_origin_form() {
    let (_dir, _root, policy) = fixture();

    assert!(matches!(
        fetch_update(&policy, "http://evil.test/.env").unwrap_err(),
        AccessDenied::InvalidTarget(_)
    ));
}

#[test]
fn an_hmr_fetch_refuses_a_null_byte() {
    let (_dir, _root, policy) = fixture();

    assert!(fetch_update(&policy, "/app/Counter.js%00.env").is_err());
}

#[test]
fn update_roles_apply_dependencies_before_boundaries() {
    assert!(UpdateRole::Dependency.apply_order() < UpdateRole::Boundary.apply_order());
    assert_eq!(UpdateRole::Boundary.as_str(), "boundary");
    assert_eq!(UpdateRole::Dependency.as_str(), "dependency");
}

fn sample_update() -> HmrUpdate {
    HmrUpdate {
        id: 4,
        path: CompactString::const_new("app/Counter.js"),
        change: ChangeKind::Modified,
        kind: UpdateKind::Hot,
        reason: None,
        modules: vec![UpdateModule {
            path: CompactString::const_new("app/Counter.js"),
            url: CompactString::const_new("/app/Counter.js?t=2"),
            role: UpdateRole::Boundary,
        }],
        routes: Vec::new(),
        elapsed_micros: 812,
    }
}

#[test]
fn an_update_serializes_with_camel_case_keys_and_kebab_case_values() {
    let json = serde_json::to_string(&sample_update()).expect("serializes");

    assert!(json.contains("\"elapsedMicros\":812"));
    assert!(json.contains("\"kind\":\"hot\""));
    assert!(json.contains("\"change\":\"modified\""));
    assert!(json.contains("\"role\":\"boundary\""));
}

#[test]
fn an_update_without_a_reason_omits_the_field() {
    let json = serde_json::to_string(&sample_update()).expect("serializes");

    assert!(!json.contains("reason"));
}

#[test]
fn an_update_with_a_reason_names_it() {
    let mut update = sample_update();
    update.kind = UpdateKind::FullReload;
    update.reason = Some(ReloadReason::NoAcceptingBoundary);

    let json = serde_json::to_string(&update).expect("serializes");

    assert!(json.contains("\"reason\":\"no-accepting-boundary\""));
}

#[test]
fn an_update_round_trips_through_json() {
    let update = sample_update();
    let json = serde_json::to_string(&update).expect("serializes");
    let parsed: HmrUpdate = serde_json::from_str(&json).expect("deserializes");

    assert_eq!(parsed, update);
}

#[test]
fn an_inert_update_reports_itself_as_inert() {
    let mut update = sample_update();
    update.kind = UpdateKind::Inert;
    update.modules.clear();

    assert!(update.is_inert());
    assert_eq!(update.module_count(), 0);
}

#[test]
fn the_module_bound_is_smaller_than_the_target_grammar_bound() {
    const {
        assert!(MAX_UPDATE_TARGET_BYTES < crate::target::MAX_TARGET_BYTES);
        assert!(MAX_UPDATE_MODULES > 0);
    }
}
