//! `package.json#exports`: subpaths, conditions and refusals.

use serde_json::json;

use crate::BundlerLimits;
use crate::limits::LimitError;
use crate::resolve::resolve_exports;

#[test]
fn an_exports_target_that_climbs_out_of_the_package_resolves_to_nothing() {
    let limits = BundlerLimits::small();
    let exports = json!({ "./secret": "../../../etc/passwd" });

    let target = resolve_exports(&exports, "./secret", "pkg", &limits).expect("no limit hit");

    assert_eq!(target, None);
}

#[test]
fn an_absolute_exports_target_resolves_to_nothing() {
    let limits = BundlerLimits::small();
    let exports = json!({ "./secret": "/etc/passwd" });

    assert_eq!(
        resolve_exports(&exports, "./secret", "pkg", &limits).expect("no limit hit"),
        None
    );
}

#[test]
fn a_null_exports_target_hides_the_subpath() {
    let limits = BundlerLimits::small();
    let exports = json!({ "./hidden": serde_json::Value::Null });

    assert_eq!(
        resolve_exports(&exports, "./hidden", "pkg", &limits).expect("no limit hit"),
        None
    );
}

#[test]
fn the_uf_condition_is_preferred_over_import() {
    let limits = BundlerLimits::small();
    let exports = json!({ ".": { "uf": "./uf.js", "import": "./esm.js", "default": "./any.js" } });

    assert_eq!(
        resolve_exports(&exports, ".", "pkg", &limits).expect("no limit hit"),
        Some(String::from("uf.js"))
    );
}

#[test]
fn the_import_condition_is_preferred_over_default() {
    let limits = BundlerLimits::small();
    let exports =
        json!({ ".": { "require": "./cjs.js", "import": "./esm.js", "default": "./any.js" } });

    assert_eq!(
        resolve_exports(&exports, ".", "pkg", &limits).expect("no limit hit"),
        Some(String::from("esm.js"))
    );
}

#[test]
fn nested_conditions_resolve() {
    let limits = BundlerLimits::small();
    let exports = json!({ ".": { "import": { "uf": "./uf.js", "default": "./esm.js" } } });

    assert_eq!(
        resolve_exports(&exports, ".", "pkg", &limits).expect("no limit hit"),
        Some(String::from("uf.js"))
    );
}

#[test]
fn a_fallback_array_takes_the_first_usable_target() {
    let limits = BundlerLimits::small();
    let exports = json!({ ".": [serde_json::Value::Null, "./second.js"] });

    assert_eq!(
        resolve_exports(&exports, ".", "pkg", &limits).expect("no limit hit"),
        Some(String::from("second.js"))
    );
}

#[test]
fn a_wildcard_subpath_substitutes_the_captured_text() {
    let limits = BundlerLimits::small();
    let exports = json!({ "./*": "./src/*.js" });

    assert_eq!(
        resolve_exports(&exports, "./deep/thing", "pkg", &limits).expect("no limit hit"),
        Some(String::from("src/deep/thing.js"))
    );
}

#[test]
fn an_exact_subpath_wins_over_a_wildcard() {
    let limits = BundlerLimits::small();
    let exports = json!({ "./*": "./src/*.js", "./one": "./special.js" });

    assert_eq!(
        resolve_exports(&exports, "./one", "pkg", &limits).expect("no limit hit"),
        Some(String::from("special.js"))
    );
}

#[test]
fn a_bare_string_exports_map_only_answers_the_package_root() {
    let limits = BundlerLimits::small();
    let exports = json!("./index.js");

    assert_eq!(
        resolve_exports(&exports, ".", "pkg", &limits).expect("no limit hit"),
        Some(String::from("index.js"))
    );
    assert_eq!(
        resolve_exports(&exports, "./deep", "pkg", &limits).expect("no limit hit"),
        None
    );
}

#[test]
fn an_exports_map_nested_past_the_ceiling_is_refused() {
    let limits = BundlerLimits::small();
    let mut exports = json!("./deep.js");
    for _ in 0..limits.max_exports_depth + 2 {
        exports = json!({ "import": exports });
    }

    let error = resolve_exports(&exports, ".", "pkg", &limits).expect_err("refused");

    assert!(matches!(error, LimitError::ExportsTooDeep { .. }));
}
