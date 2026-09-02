//! The grammar that decides whether a declared plugin name may name code on
//! disk at all.
//!
//! Every case below is a way a config could try to reach outside the project.
//! See `docs/security.md` for the failures each one is modelled on.

use camino::Utf8PathBuf;

use crate::builtin::BuiltinPlugin;
use crate::descriptor::PluginSource;
use crate::resolve::{
    BUILTIN_PREFIX, MAX_PLUGIN_NAME_BYTES, PluginPathError, classify_plugin_name,
};

use super::temp_root;

/// A root that does not exist, so the lexical grammar is what is under test.
const ROOT: &str = "/workspace/app";

fn classify(name: &str) -> Result<PluginSource, PluginPathError> {
    classify_plugin_name(name, Utf8PathBuf::from(ROOT).as_path())
}

fn rejected(name: &str) -> PluginPathError {
    classify(name).expect_err(&format!("{name:?} must be refused"))
}

#[test]
fn a_bare_package_specifier_is_a_package() {
    assert_eq!(
        classify("mdx").expect("accepted"),
        PluginSource::Package {
            specifier: "mdx".into()
        }
    );
}

#[test]
fn a_scoped_package_specifier_is_a_package() {
    assert_eq!(
        classify("@uniflowed/plugin-mdx").expect("accepted"),
        PluginSource::Package {
            specifier: "@uniflowed/plugin-mdx".into()
        }
    );
}

#[test]
fn a_package_subpath_is_a_package() {
    assert_eq!(
        classify("@scope/pkg/dist/plugin.js").expect("accepted"),
        PluginSource::Package {
            specifier: "@scope/pkg/dist/plugin.js".into()
        }
    );
}

#[test]
fn a_bare_path_shaped_name_is_still_a_package_and_never_joined_onto_the_root() {
    // Only a leading `./` makes a name a file, exactly as a module resolver
    // reads it. Anything else goes to the package resolver and never becomes a
    // path, so there is one guard rather than two.
    assert!(matches!(
        classify("plugins/metrics.js").expect("accepted"),
        PluginSource::Package { .. }
    ));
}

#[test]
fn a_dot_slash_name_is_a_project_file_under_the_root() {
    assert_eq!(
        classify("./plugins/metrics.js").expect("accepted"),
        PluginSource::ProjectFile {
            path: Utf8PathBuf::from("/workspace/app/plugins/metrics.js"),
        }
    );
}

#[test]
fn a_project_file_keeps_its_nested_directories() {
    assert_eq!(
        classify("./a/b/c/plugin.js").expect("accepted"),
        PluginSource::ProjectFile {
            path: Utf8PathBuf::from("/workspace/app/a/b/c/plugin.js"),
        }
    );
}

#[test]
fn an_absolute_path_is_refused() {
    assert_eq!(
        rejected("/etc/passwd"),
        PluginPathError::Absolute {
            name: "/etc/passwd".into()
        }
    );
}

#[test]
fn a_root_slash_alone_is_refused() {
    assert!(matches!(rejected("/"), PluginPathError::Absolute { .. }));
}

#[test]
fn a_parent_segment_is_refused() {
    assert_eq!(
        rejected("../evil.js"),
        PluginPathError::ParentSegment {
            name: "../evil.js".into(),
            position: 0,
        }
    );
}

#[test]
fn a_parent_segment_after_a_dot_slash_is_refused() {
    assert_eq!(
        rejected("./../../.ssh/id_ed25519"),
        PluginPathError::ParentSegment {
            name: "./../../.ssh/id_ed25519".into(),
            position: 1,
        }
    );
}

#[test]
fn a_parent_segment_buried_deep_in_the_path_is_refused() {
    assert!(matches!(
        rejected("./a/b/c/../../../../../../etc/passwd"),
        PluginPathError::ParentSegment { .. }
    ));
}

#[test]
fn a_parent_segment_inside_a_package_specifier_is_refused() {
    assert!(matches!(
        rejected("@scope/../../../etc/passwd"),
        PluginPathError::ParentSegment { .. }
    ));
}

#[test]
fn a_dotdot_that_is_only_part_of_a_name_is_allowed() {
    // `..foo` is a legal file name; only a whole `..` segment walks upwards.
    assert!(classify("./a/..foo.js").is_ok());
    assert!(classify("./a/foo...js").is_ok());
}

#[test]
fn a_backslash_is_a_separator_on_every_platform() {
    // The pnpm tarball traversal and the Vite-era dev-server bypass were both
    // "the deny check treated `\` as an ordinary character off Windows".
    assert_eq!(
        rejected("..\\..\\evil.js"),
        PluginPathError::BackslashSeparator {
            name: "..\\..\\evil.js".into(),
            offset: 2,
        }
    );
}

#[test]
fn a_unc_path_is_refused() {
    assert!(matches!(
        rejected("\\\\server\\share\\evil.js"),
        PluginPathError::BackslashSeparator { offset: 0, .. }
    ));
}

#[test]
fn a_windows_drive_path_is_refused() {
    assert_eq!(
        rejected("C:/Windows/System32/evil.js"),
        PluginPathError::UrlScheme {
            name: "C:/Windows/System32/evil.js".into(),
            scheme: "C".into(),
        }
    );
}

#[test]
fn a_file_url_is_refused() {
    assert!(matches!(
        rejected("file:///etc/passwd"),
        PluginPathError::UrlScheme { .. }
    ));
}

#[test]
fn a_remote_url_is_refused() {
    for name in [
        "http://example.test/plugin.js",
        "https://example.test/plugin.js",
        "data:text/javascript,alert(1)",
        "node:fs",
    ] {
        assert!(
            matches!(rejected(name), PluginPathError::UrlScheme { .. }),
            "{name}"
        );
    }
}

#[test]
fn a_protocol_relative_url_is_refused() {
    assert!(matches!(
        rejected("//example.test/plugin.js"),
        PluginPathError::Absolute { .. }
    ));
}

#[test]
fn a_home_relative_path_is_refused() {
    assert_eq!(
        rejected("~/.ssh/id_ed25519"),
        PluginPathError::HomeRelative {
            name: "~/.ssh/id_ed25519".into()
        }
    );
}

#[test]
fn an_empty_name_is_refused() {
    assert_eq!(rejected(""), PluginPathError::Empty);
}

#[test]
fn an_oversized_name_is_refused() {
    let name = "a".repeat(MAX_PLUGIN_NAME_BYTES + 1);

    assert_eq!(
        rejected(&name),
        PluginPathError::TooLong {
            bytes: MAX_PLUGIN_NAME_BYTES + 1,
            limit: MAX_PLUGIN_NAME_BYTES,
        }
    );
}

#[test]
fn a_name_at_exactly_the_ceiling_is_accepted() {
    let name = "a".repeat(MAX_PLUGIN_NAME_BYTES);

    assert!(classify(&name).is_ok());
}

#[test]
fn a_nul_byte_is_refused() {
    assert_eq!(
        rejected("plugin\0.js"),
        PluginPathError::ControlByte {
            name: "plugin\0.js".into(),
            offset: 6,
        }
    );
}

#[test]
fn a_newline_or_tab_is_refused() {
    assert!(matches!(
        rejected("plugin\n.js"),
        PluginPathError::ControlByte { offset: 6, .. }
    ));
    assert!(matches!(
        rejected("\tplugin"),
        PluginPathError::ControlByte { offset: 0, .. }
    ));
}

#[test]
fn a_delete_byte_is_refused() {
    assert!(matches!(
        rejected("plugin\u{7f}"),
        PluginPathError::ControlByte { .. }
    ));
}

#[test]
fn the_builtin_prefix_is_reserved() {
    assert_eq!(
        rejected("uf:flow"),
        PluginPathError::ReservedPrefix {
            name: "uf:flow".into(),
            prefix: BUILTIN_PREFIX,
        }
    );
    for plugin in BuiltinPlugin::ALL {
        assert!(
            matches!(
                rejected(plugin.name()),
                PluginPathError::ReservedPrefix { .. }
            ),
            "{}",
            plugin.name()
        );
    }
}

#[test]
fn an_empty_segment_is_refused() {
    assert_eq!(
        rejected("./a//b.js"),
        PluginPathError::EmptySegment {
            name: "./a//b.js".into(),
            position: 2,
        }
    );
}

#[test]
fn a_trailing_slash_is_refused() {
    assert!(matches!(
        rejected("./plugins/"),
        PluginPathError::EmptySegment { .. }
    ));
}

#[test]
fn a_dot_segment_away_from_the_front_is_refused() {
    assert_eq!(
        rejected("./a/./b.js"),
        PluginPathError::CurrentSegment {
            name: "./a/./b.js".into(),
            position: 2,
        }
    );
}

#[test]
fn a_lone_dot_is_accepted_as_a_package_name() {
    // Position zero is the `./` form; a bare `.` is not a path and goes to the
    // package resolver, which will simply not find it.
    assert!(matches!(
        classify(".").expect("accepted"),
        PluginSource::Package { .. }
    ));
}

#[test]
fn a_non_ascii_package_name_is_accepted() {
    assert!(classify("@scope/plugin-café").is_ok());
    assert!(classify("./plugins/日本語.js").is_ok());
}

#[test]
fn a_symlink_pointing_out_of_the_project_is_refused() {
    if cfg!(not(unix)) {
        return;
    }
    #[cfg(unix)]
    {
        let outside = tempfile::tempdir().expect("temp dir");
        let project = tempfile::tempdir().expect("temp dir");
        let outside_root = temp_root(&outside);
        let root = temp_root(&project);
        std::fs::write(outside_root.join("evil.js"), "// @flow\n").expect("write");
        std::os::unix::fs::symlink(outside_root.join("evil.js"), root.join("plugin.js"))
            .expect("symlink");

        let error = classify_plugin_name("./plugin.js", &root)
            .expect_err("a symlink out of the project is refused");

        assert!(
            matches!(error, PluginPathError::OutsideRoot { .. }),
            "{error:?}"
        );
    }
}

#[test]
fn a_symlink_that_stays_inside_the_project_is_accepted() {
    if cfg!(not(unix)) {
        return;
    }
    #[cfg(unix)]
    {
        let project = tempfile::tempdir().expect("temp dir");
        let root = temp_root(&project);
        std::fs::create_dir_all(root.join("real")).expect("dir");
        std::fs::write(root.join("real/plugin.js"), "// @flow\n").expect("write");
        std::os::unix::fs::symlink(root.join("real/plugin.js"), root.join("plugin.js"))
            .expect("symlink");

        assert!(classify_plugin_name("./plugin.js", &root).is_ok());
    }
}

#[test]
fn a_sibling_directory_sharing_a_prefix_is_not_inside_the_root() {
    // Containment is decided on path components, so `/workspace/app-evil` is
    // not treated as living under `/workspace/app`.
    let root = Utf8PathBuf::from("/workspace/app");
    let sibling = Utf8PathBuf::from("/workspace/app-evil/plugin.js");

    assert!(!sibling.starts_with(&root));
}

#[test]
fn a_real_file_inside_the_project_resolves_to_its_checked_path() {
    let project = tempfile::tempdir().expect("temp dir");
    let root = temp_root(&project);
    std::fs::create_dir_all(root.join("plugins")).expect("dir");
    std::fs::write(root.join("plugins/metrics.js"), "// @flow\n").expect("write");

    assert_eq!(
        classify_plugin_name("./plugins/metrics.js", &root).expect("accepted"),
        PluginSource::ProjectFile {
            path: root.join("plugins/metrics.js"),
        }
    );
}

#[test]
fn path_errors_say_what_was_wrong() {
    assert_eq!(
        rejected("/etc/passwd").to_string(),
        "plugin name \"/etc/passwd\" is an absolute path"
    );
    assert_eq!(
        rejected("~/x").to_string(),
        "plugin name \"~/x\" is home-relative"
    );
    assert_eq!(PluginPathError::Empty.to_string(), "plugin name is empty");
}
