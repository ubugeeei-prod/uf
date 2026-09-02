//! Specifier resolution, and the guards it refuses input with.

use camino::{Utf8Path, Utf8PathBuf};

use super::fixture::Fixture;
use crate::BundlerLimits;
use crate::limits::LimitError;
use crate::resolve::{Resolution, ResolveError, Resolver};

fn resolver(fixture: &Fixture) -> Resolver {
    Resolver::new(fixture.root.clone(), BundlerLimits::small())
}

fn importer() -> &'static Utf8Path {
    Utf8Path::new("app/page.js")
}

#[test]
fn a_relative_specifier_resolves_to_a_sibling() {
    let fixture = Fixture::new();
    fixture.write("app/util.js", "export const a = 1;\n");

    let resolved = resolver(&fixture)
        .resolve(importer(), "./util.js")
        .expect("resolves");

    assert_eq!(
        resolved,
        Resolution::Module(Utf8PathBuf::from("app/util.js"))
    );
    fixture.keep();
}

#[test]
fn a_relative_specifier_without_an_extension_finds_the_js_file() {
    let fixture = Fixture::new();
    fixture.write("app/util.js", "export const a = 1;\n");

    let resolved = resolver(&fixture)
        .resolve(importer(), "./util")
        .expect("resolves");

    assert_eq!(
        resolved,
        Resolution::Module(Utf8PathBuf::from("app/util.js"))
    );
    fixture.keep();
}

#[test]
fn a_directory_specifier_finds_its_index() {
    let fixture = Fixture::new();
    fixture.write("app/util/index.js", "export const a = 1;\n");

    let resolved = resolver(&fixture)
        .resolve(importer(), "./util")
        .expect("resolves");

    assert_eq!(
        resolved,
        Resolution::Module(Utf8PathBuf::from("app/util/index.js"))
    );
    fixture.keep();
}

#[test]
fn a_parent_specifier_resolves_upwards_inside_the_project() {
    let fixture = Fixture::new();
    fixture.write("server/actions.js", "export const a = 1;\n");

    let resolved = resolver(&fixture)
        .resolve(importer(), "../server/actions.js")
        .expect("resolves");

    assert_eq!(
        resolved,
        Resolution::Module(Utf8PathBuf::from("server/actions.js"))
    );
    fixture.keep();
}

#[test]
fn a_specifier_that_climbs_out_of_the_project_is_refused() {
    let fixture = Fixture::new();

    let error = resolver(&fixture)
        .resolve(importer(), "../../../etc/passwd")
        .expect_err("refused");

    assert!(matches!(error, ResolveError::EscapesProjectRoot { .. }));
    fixture.keep();
}

#[test]
fn a_missing_relative_specifier_is_an_error() {
    let fixture = Fixture::new();

    let error = resolver(&fixture)
        .resolve(importer(), "./gone.js")
        .expect_err("refused");

    assert!(matches!(error, ResolveError::NotFound { .. }));
    fixture.keep();
}

#[test]
fn an_empty_specifier_is_refused() {
    let fixture = Fixture::new();

    let error = resolver(&fixture)
        .resolve(importer(), "")
        .expect_err("refused");

    assert!(matches!(error, ResolveError::Empty { .. }));
    fixture.keep();
}

#[test]
fn a_specifier_holding_a_control_byte_is_refused() {
    let fixture = Fixture::new();

    let error = resolver(&fixture)
        .resolve(importer(), "./a\u{0}.js")
        .expect_err("refused");

    assert!(matches!(error, ResolveError::ControlByte { offset: 3, .. }));
    fixture.keep();
}

#[test]
fn a_backslash_is_a_separator_on_every_platform_and_is_refused() {
    let fixture = Fixture::new();

    let error = resolver(&fixture)
        .resolve(importer(), "..\\..\\secrets.js")
        .expect_err("refused");

    assert!(matches!(
        error,
        ResolveError::BackslashSeparator { offset: 2, .. }
    ));
    fixture.keep();
}

#[test]
fn an_absolute_specifier_is_refused() {
    let fixture = Fixture::new();

    let error = resolver(&fixture)
        .resolve(importer(), "/etc/passwd")
        .expect_err("refused");

    assert!(matches!(error, ResolveError::Absolute { .. }));
    fixture.keep();
}

#[test]
fn a_home_relative_specifier_is_refused() {
    let fixture = Fixture::new();

    let error = resolver(&fixture)
        .resolve(importer(), "~/.ssh/id_ed25519")
        .expect_err("refused");

    assert!(matches!(error, ResolveError::HomeRelative { .. }));
    fixture.keep();
}

#[test]
fn a_url_specifier_is_refused() {
    let fixture = Fixture::new();

    let error = resolver(&fixture)
        .resolve(importer(), "https://cdn.example.com/pkg.js")
        .expect_err("refused");

    assert!(matches!(error, ResolveError::UrlScheme { .. }));
    fixture.keep();
}

#[test]
fn a_windows_drive_specifier_is_refused() {
    let fixture = Fixture::new();

    let error = resolver(&fixture)
        .resolve(importer(), "C:/Windows/system32")
        .expect_err("refused");

    assert!(matches!(error, ResolveError::UrlScheme { .. }));
    fixture.keep();
}

#[test]
fn a_node_builtin_is_external() {
    let fixture = Fixture::new();

    let resolved = resolver(&fixture)
        .resolve(importer(), "node:fs")
        .expect("resolves");

    assert_eq!(resolved, Resolution::External("node:fs".into()));
    fixture.keep();
}

#[test]
fn an_over_long_specifier_is_refused() {
    let fixture = Fixture::new();
    let specifier = format!(
        "./{}",
        "a".repeat(BundlerLimits::small().max_specifier_bytes)
    );

    let error = resolver(&fixture)
        .resolve(importer(), &specifier)
        .expect_err("refused");

    assert!(matches!(
        error,
        ResolveError::Limit(LimitError::SpecifierTooLong { .. })
    ));
    fixture.keep();
}

#[test]
fn an_uninstalled_package_is_external() {
    let fixture = Fixture::new();

    let resolved = resolver(&fixture)
        .resolve(importer(), "react")
        .expect("resolves");

    assert_eq!(resolved, Resolution::External("react".into()));
    fixture.keep();
}

#[test]
fn a_package_without_a_manifest_resolves_to_its_index() {
    let fixture = Fixture::new();
    fixture.write("node_modules/tiny/index.js", "export const a = 1;\n");

    let resolved = resolver(&fixture)
        .resolve(importer(), "tiny")
        .expect("resolves");

    assert_eq!(
        resolved,
        Resolution::Module(Utf8PathBuf::from("node_modules/tiny/index.js"))
    );
    fixture.keep();
}

#[test]
fn a_package_main_field_is_honoured() {
    let fixture = Fixture::new();
    fixture.write(
        "node_modules/tiny/package.json",
        "{\"name\":\"tiny\",\"main\":\"lib/entry.js\"}",
    );
    fixture.write("node_modules/tiny/lib/entry.js", "export const a = 1;\n");

    let resolved = resolver(&fixture)
        .resolve(importer(), "tiny")
        .expect("resolves");

    assert_eq!(
        resolved,
        Resolution::Module(Utf8PathBuf::from("node_modules/tiny/lib/entry.js"))
    );
    fixture.keep();
}

#[test]
fn a_scoped_package_resolves() {
    let fixture = Fixture::new();
    fixture.write("node_modules/@scope/pkg/index.js", "export const a = 1;\n");

    let resolved = resolver(&fixture)
        .resolve(importer(), "@scope/pkg")
        .expect("resolves");

    assert_eq!(
        resolved,
        Resolution::Module(Utf8PathBuf::from("node_modules/@scope/pkg/index.js"))
    );
    fixture.keep();
}

#[test]
fn a_uniflowed_specifier_maps_to_a_core_subpath() {
    let fixture = Fixture::new();
    fixture.write(
        "node_modules/@uniflowed/core/package.json",
        "{\"name\":\"@uniflowed/core\",\"exports\":{\"./react\":\"./react.js\"}}",
    );
    fixture.write(
        "node_modules/@uniflowed/core/react.js",
        "export const a = 1;\n",
    );

    let resolved = resolver(&fixture)
        .resolve(importer(), "@uniflowed/react")
        .expect("resolves");

    assert_eq!(
        resolved,
        Resolution::Module(Utf8PathBuf::from("node_modules/@uniflowed/core/react.js"))
    );
    fixture.keep();
}

#[test]
fn a_uniflowed_core_subpath_resolves_directly() {
    let fixture = Fixture::new();
    fixture.write(
        "node_modules/@uniflowed/core/package.json",
        "{\"name\":\"@uniflowed/core\",\"exports\":{\"./react\":\"./react.js\"}}",
    );
    fixture.write(
        "node_modules/@uniflowed/core/react.js",
        "export const a = 1;\n",
    );

    let resolved = resolver(&fixture)
        .resolve(importer(), "@uniflowed/core/react")
        .expect("resolves");

    assert_eq!(
        resolved,
        Resolution::Module(Utf8PathBuf::from("node_modules/@uniflowed/core/react.js"))
    );
    fixture.keep();
}

#[test]
fn a_nested_node_modules_wins_over_the_root_one() {
    let fixture = Fixture::new();
    fixture.write("node_modules/pkg/index.js", "export const a = \"root\";\n");
    fixture.write(
        "app/node_modules/pkg/index.js",
        "export const a = \"nested\";\n",
    );

    let resolved = resolver(&fixture)
        .resolve(importer(), "pkg")
        .expect("resolves");

    assert_eq!(
        resolved,
        Resolution::Module(Utf8PathBuf::from("app/node_modules/pkg/index.js"))
    );
    fixture.keep();
}

#[test]
fn a_manifest_over_the_ceiling_is_refused() {
    let fixture = Fixture::new();
    let padding = "x".repeat(BundlerLimits::small().max_manifest_bytes as usize);
    fixture.write(
        "node_modules/pkg/package.json",
        &format!("{{\"name\":\"pkg\",\"description\":\"{padding}\"}}"),
    );
    fixture.write("node_modules/pkg/index.js", "export const a = 1;\n");

    let error = resolver(&fixture)
        .resolve(importer(), "pkg")
        .expect_err("refused");

    assert!(matches!(
        error,
        ResolveError::Limit(LimitError::ManifestTooLarge { .. })
    ));
    fixture.keep();
}

#[test]
fn a_manifest_with_a_prototype_key_is_ignored() {
    let fixture = Fixture::new();
    fixture.write(
        "node_modules/pkg/package.json",
        "{\"name\":\"pkg\",\"exports\":{\"__proto__\":{\"polluted\":true},\".\":\"./danger.js\"}}",
    );
    fixture.write("node_modules/pkg/danger.js", "export const a = 1;\n");
    fixture.write("node_modules/pkg/index.js", "export const a = 2;\n");

    let resolved = resolver(&fixture)
        .resolve(importer(), "pkg")
        .expect("resolves");

    // The poisoned map is dropped entirely, so resolution falls back to
    // `index.js` rather than trusting anything the manifest said.
    assert_eq!(
        resolved,
        Resolution::Module(Utf8PathBuf::from("node_modules/pkg/index.js"))
    );
    fixture.keep();
}

#[test]
fn a_symlinked_module_is_refused() {
    let fixture = Fixture::new();
    fixture.write("app/real.js", "export const a = 1;\n");
    let link = fixture.path("app/link.js");
    #[cfg(unix)]
    std::os::unix::fs::symlink(fixture.path("app/real.js"), &link).expect("symlink");
    #[cfg(not(unix))]
    return;

    let error = resolver(&fixture)
        .resolve(importer(), "./link.js")
        .expect_err("refused");

    assert!(matches!(error, ResolveError::Symlink { .. }));
    fixture.keep();
}

#[test]
fn a_manifest_says_whether_a_package_is_side_effect_free() {
    let fixture = Fixture::new();
    fixture.write(
        "node_modules/pkg/package.json",
        "{\"name\":\"pkg\",\"sideEffects\":false}",
    );
    fixture.write("node_modules/pkg/index.js", "export const a = 1;\n");
    let mut resolver = resolver(&fixture);

    let manifest = resolver
        .manifest_for(Utf8Path::new("node_modules/pkg/index.js"))
        .expect("manifest reads");

    assert_eq!(
        manifest.side_effects,
        crate::resolve::SideEffectsField::None
    );
    fixture.keep();
}

#[test]
fn a_package_subpath_resolves_through_the_exports_map() {
    let fixture = Fixture::new();
    fixture.write(
        "node_modules/pkg/package.json",
        "{\"name\":\"pkg\",\"exports\":{\".\":\"./index.js\",\"./deep\":\"./src/deep.js\"}}",
    );
    fixture.write("node_modules/pkg/src/deep.js", "export const a = 1;\n");

    let resolved = resolver(&fixture)
        .resolve(importer(), "pkg/deep")
        .expect("resolves");

    assert_eq!(
        resolved,
        Resolution::Module(Utf8PathBuf::from("node_modules/pkg/src/deep.js"))
    );
    fixture.keep();
}
