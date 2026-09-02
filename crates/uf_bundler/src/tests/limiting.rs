//! Every ceiling, and the typed refusal it produces.

use super::fixture::Fixture;
use crate::limits::LimitError;
use crate::{BundleError, BundlerLimits};

#[test]
fn the_default_limits_are_larger_than_the_small_ones() {
    let default = BundlerLimits::default();
    let small = BundlerLimits::small();

    assert!(default.max_modules > small.max_modules);
    assert!(default.max_depth > small.max_depth);
    assert!(default.max_module_bytes > small.max_module_bytes);
    assert!(default.max_chunks > small.max_chunks);
}

#[test]
fn too_many_modules_is_refused() {
    let mut fixture = Fixture::new();
    fixture.limits.max_modules = 3;
    for index in 0..8 {
        fixture.write(
            &format!("m{index}.js"),
            &format!(
                "import \"./m{}.js\";\nexport const a{index} = {index};\n",
                index + 1
            ),
        );
    }
    fixture.write("m8.js", "export const last = 1;\n");
    fixture.entries.push("m0.js".into());

    let error = fixture.try_bundle().expect_err("refused");

    assert!(matches!(
        error,
        BundleError::Limit(LimitError::TooManyModules { limit: 3, .. })
    ));
    fixture.keep();
}

#[test]
fn too_deep_a_graph_is_refused() {
    let mut fixture = Fixture::new();
    fixture.limits.max_depth = 2;
    for index in 0..6 {
        fixture.write(
            &format!("m{index}.js"),
            &format!(
                "import \"./m{}.js\";\nexport const a{index} = {index};\n",
                index + 1
            ),
        );
    }
    fixture.write("m6.js", "export const last = 1;\n");
    fixture.entries.push("m0.js".into());

    let error = fixture.try_bundle().expect_err("refused");

    assert!(matches!(
        error,
        BundleError::Limit(LimitError::GraphTooDeep { limit: 2, .. })
    ));
    fixture.keep();
}

#[test]
fn an_over_large_module_is_refused() {
    let mut fixture = Fixture::new();
    fixture.limits.max_module_bytes = 64;
    fixture.entry(
        "app.js",
        &format!("export const a = \"{}\";\n", "x".repeat(200)),
    );

    let error = fixture.try_bundle().expect_err("refused");

    assert!(matches!(
        error,
        BundleError::Limit(LimitError::ModuleTooLarge { limit: 64, .. })
    ));
    fixture.keep();
}

#[test]
fn too_many_chunks_is_refused() {
    let mut fixture = Fixture::new();
    fixture.limits.max_chunks = 2;
    for index in 0..5 {
        fixture.entry(&format!("e{index}.js"), "export const a = 1;\n");
    }

    let error = fixture.try_bundle().expect_err("refused");

    assert!(matches!(
        error,
        BundleError::Limit(LimitError::TooManyChunks { limit: 2, .. })
    ));
    fixture.keep();
}

#[test]
fn an_entry_outside_the_project_is_refused() {
    let mut fixture = Fixture::new();
    fixture.entries.push("../outside.js".into());

    let error = fixture.try_bundle().expect_err("refused");

    assert!(matches!(error, BundleError::EntryOutsideProject { .. }));
    fixture.keep();
}

#[test]
fn a_module_that_is_not_utf8_is_refused() {
    let fixture = Fixture::new();
    std::fs::write(fixture.path("app.js"), [0xff, 0xfe, 0x00]).expect("write");
    let mut fixture = fixture;
    fixture.entries.push("app.js".into());

    let error = fixture.try_bundle().expect_err("refused");

    assert!(matches!(error, BundleError::NonUtf8 { .. }));
    fixture.keep();
}

#[test]
fn a_missing_entry_is_refused() {
    let mut fixture = Fixture::new();
    fixture.entries.push("gone.js".into());

    let error = fixture.try_bundle().expect_err("refused");

    assert!(matches!(error, BundleError::Read { .. }));
    fixture.keep();
}

#[test]
fn limit_errors_name_the_ceiling_they_broke() {
    let error = LimitError::TooManyModules { count: 5, limit: 3 };

    assert_eq!(
        error.to_string(),
        "build reached 5 modules, over the ceiling of 3"
    );
}

#[test]
fn a_module_exactly_at_the_size_ceiling_is_accepted() {
    let mut fixture = Fixture::new();
    let source = "export const a = 1;\n";
    fixture.limits.max_module_bytes = source.len() as u64;
    fixture.entry("app.js", source);

    let output = fixture.bundle();

    assert_eq!(output.chunks.len(), 1);
    fixture.keep();
}

#[test]
fn a_graph_exactly_at_the_depth_ceiling_is_accepted() {
    let mut fixture = Fixture::new();
    fixture.limits.max_depth = 2;
    fixture.write("deep.js", "export const deep = 1;\n");
    fixture.write(
        "mid.js",
        "import { deep } from \"./deep.js\";\nexport const mid = deep;\n",
    );
    fixture.entry(
        "app.js",
        "import { mid } from \"./mid.js\";\nexport const top = mid;\n",
    );

    let output = fixture.bundle();

    assert_eq!(output.stats.modules_loaded, 3);
    fixture.keep();
}
