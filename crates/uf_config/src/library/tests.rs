use camino::Utf8Path;

use super::{LibraryConfig, LibraryFormat, LibraryPlan, check};
use crate::{ConfigError, UniflowedConfig};

/// A config with the file-system router off, which is what makes a library.
fn library() -> UniflowedConfig {
    let mut config = UniflowedConfig::default();
    config.app.router.enabled = false;
    config
}

#[test]
fn an_application_has_no_library_plan() {
    assert_eq!(LibraryPlan::resolve(&UniflowedConfig::default()), None);
}

/// The scaffold's own configuration, which is the whole of ubugeeei-prod/uf#268.
///
/// `uf create lib` writes `app.router.enabled: false` and nothing else about
/// the build, and that has to be enough: before this, the same file produced a
/// build that looked for `app.js`.
#[test]
fn turning_the_router_off_is_the_whole_declaration() {
    let plan = LibraryPlan::resolve(&library()).expect("a library");
    assert_eq!(plan.entries(), ["index.js"]);
    assert_eq!(plan.formats(), [LibraryFormat::Es]);
    assert!(plan.external().is_empty());
    assert!(plan.because().contains("app.router.enabled"));
}

#[test]
fn build_lib_says_what_a_library_builds() {
    let mut config = library();
    config.build.lib = Some(LibraryConfig {
        entries: vec!["index.js".into(), "internal/parse.js".into()],
        formats: vec![LibraryFormat::Es, LibraryFormat::Cjs],
        external: vec!["react".into()],
    });
    let plan = LibraryPlan::resolve(&config).expect("a library");
    assert_eq!(plan.entries(), ["index.js", "internal/parse.js"]);
    assert_eq!(plan.formats(), [LibraryFormat::Es, LibraryFormat::Cjs]);
    assert_eq!(plan.external(), ["react"]);
    // The sentence quotes the key the project wrote, so a reader who asked
    // `uf explain build` is sent to the line that decided.
    assert!(plan.because().contains("build.lib"));
}

/// One fact, declared twice, is the failure this refusal exists for.
#[test]
fn a_library_build_in_an_application_is_refused() {
    let mut config = UniflowedConfig::default();
    config.build.lib = Some(LibraryConfig::default());
    let error = check(Utf8Path::new("uf.config.js"), &config).unwrap_err();
    assert!(matches!(
        error,
        ConfigError::LibraryBuildInAnApplication { .. }
    ));
    assert!(error.to_string().contains("app.router.enabled"));
}

#[test]
fn a_library_with_no_entry_is_refused() {
    let mut config = library();
    config.build.lib = Some(LibraryConfig {
        entries: Vec::new(),
        ..LibraryConfig::default()
    });
    let error = check(Utf8Path::new("uf.config.js"), &config).unwrap_err();
    assert!(matches!(error, ConfigError::LibraryWithoutEntries { .. }));
}

#[test]
fn a_library_with_no_format_is_refused() {
    let mut config = library();
    config.build.lib = Some(LibraryConfig {
        formats: Vec::new(),
        ..LibraryConfig::default()
    });
    let error = check(Utf8Path::new("uf.config.js"), &config).unwrap_err();
    assert!(matches!(error, ConfigError::LibraryWithoutFormats { .. }));
}

/// A format uf has not written is named, not skipped.
///
/// The skipped version is the bug one layer up: a build that succeeds and
/// writes nothing the project asked for, found from a consumer rather than
/// from the build.
#[test]
fn a_format_uf_has_not_written_is_refused_by_name() {
    let mut config = library();
    config.build.lib = Some(LibraryConfig {
        formats: vec![LibraryFormat::Es, LibraryFormat::Umd],
        ..LibraryConfig::default()
    });
    let error = check(Utf8Path::new("uf.config.js"), &config).unwrap_err();
    let said = error.to_string();
    assert!(said.contains("umd"), "{said}");
    // And the two that do work, so the message ends somewhere useful.
    assert!(said.contains("es") && said.contains("cjs"), "{said}");
}

#[test]
fn the_two_written_formats_have_the_extensions_a_consumer_resolves() {
    assert_eq!(LibraryFormat::Es.extension(), "js");
    // Not `.js`: the manifest says `"type": "module"`, so a CommonJS file with
    // a `.js` name is a `SyntaxError` in the consumer's `require`.
    assert_eq!(LibraryFormat::Cjs.extension(), "cjs");
    assert!(LibraryFormat::Es.is_implemented());
    assert!(LibraryFormat::Cjs.is_implemented());
    assert!(!LibraryFormat::Umd.is_implemented());
    assert!(!LibraryFormat::Iife.is_implemented());
}

#[test]
fn a_library_that_declared_nothing_passes_the_check() {
    assert!(check(Utf8Path::new("uf.config.js"), &library()).is_ok());
    assert!(check(Utf8Path::new("uf.config.js"), &UniflowedConfig::default()).is_ok());
}
