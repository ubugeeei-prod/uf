use super::*;

mod decision;

/// A throwaway project root with a few files in it.
struct Project {
    _dir: tempfile::TempDir,
    root: Utf8PathBuf,
}

impl Project {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        // The temp directory is itself behind a symlink on macOS, so canonicalize
        // once here; every assertion below then compares real paths to real paths.
        let root = Utf8PathBuf::from_path_buf(dir.path().canonicalize().unwrap()).unwrap();
        Self { _dir: dir, root }
    }

    fn write(&self, relative: &str, contents: &str) -> Utf8PathBuf {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, contents).unwrap();
        path
    }

    fn resolve(&self, target: &str) -> Result<ResolvedFile, AccessDenied> {
        resolve_request(&self.root, target)
    }

    fn body(&self, target: &str) -> String {
        String::from_utf8(self.resolve(target).unwrap().read().unwrap()).unwrap()
    }
}

// --- the positive path ------------------------------------------------------

#[test]
fn serves_an_ordinary_file() {
    let project = Project::new();
    project.write("app/main.js", "export default 1;\n");
    assert_eq!(project.body("/app/main.js"), "export default 1;\n");
}

#[test]
fn serves_the_directory_index_for_the_root() {
    let project = Project::new();
    project.write("index.html", "<!doctype html>\n");
    assert_eq!(project.body("/"), "<!doctype html>\n");
}

#[test]
fn serves_a_file_whose_name_contains_a_space() {
    let project = Project::new();
    project.write("a file.js", "spaced\n");
    assert_eq!(project.body("/a%20file.js"), "spaced\n");
}

#[test]
fn serves_a_file_with_a_non_ascii_name() {
    let project = Project::new();
    project.write("café/naïve.js", "unicode\n");
    assert_eq!(project.body("/caf%C3%A9/na%C3%AFve.js"), "unicode\n");
}

#[test]
fn serves_a_file_through_a_legitimate_import_query() {
    let project = Project::new();
    project.write("app/main.js", "export default 1;\n");
    let resolved = project.resolve("/app/main.js?import&t=1700000000").unwrap();
    assert_eq!(resolved.loader(), Loader::Module);
    assert_eq!(resolved.read().unwrap(), b"export default 1;\n");
}

#[test]
fn carries_the_selected_loader_onto_the_resolved_file() {
    let project = Project::new();
    project.write("app/logo.svg", "<svg/>\n");
    assert_eq!(
        project.resolve("/app/logo.svg?raw").unwrap().loader(),
        Loader::Raw
    );
}

#[test]
fn reports_the_media_type_from_the_canonical_extension() {
    let project = Project::new();
    project.write("app/main.js", "x\n");
    project.write("app/data.json", "{}\n");
    project.write("app/blob.unknown", "x\n");
    assert_eq!(
        project
            .resolve("/app/main.js")
            .unwrap()
            .media_type()
            .as_str(),
        "text/javascript; charset=utf-8"
    );
    assert_eq!(
        project
            .resolve("/app/data.json")
            .unwrap()
            .media_type()
            .as_str(),
        "application/json"
    );
    assert_eq!(
        project
            .resolve("/app/blob.unknown")
            .unwrap()
            .media_type()
            .as_str(),
        "application/octet-stream"
    );
}

#[test]
fn collapses_redundant_separators_and_dot_segments() {
    let project = Project::new();
    project.write("app/main.js", "ok\n");
    assert_eq!(project.body("/app//main.js"), "ok\n");
    assert_eq!(project.body("/./app/./main.js"), "ok\n");
    assert_eq!(project.body("/app/nested/../main.js"), "ok\n");
}

#[test]
fn reports_the_length_from_the_open_handle() {
    let project = Project::new();
    project.write("app/main.js", "1234567890");
    let resolved = project.resolve("/app/main.js").unwrap();
    assert_eq!(resolved.len(), 10);
    assert!(!resolved.is_empty());
}

#[test]
fn an_empty_file_is_reported_as_empty() {
    let project = Project::new();
    project.write("app/empty.js", "");
    assert!(project.resolve("/app/empty.js").unwrap().is_empty());
}

#[test]
fn the_checked_path_displays_the_canonical_path() {
    let project = Project::new();
    let written = project.write("app/main.js", "ok\n");
    let resolved = project.resolve("/app/main.js").unwrap();
    assert_eq!(resolved.checked_path().to_string(), written.to_string());
}

// --- percent decoding -------------------------------------------------------

#[test]
fn decodes_exactly_once() {
    let project = Project::new();
    project.write("a b.js", "ok\n");
    assert_eq!(project.body("/a%20b.js"), "ok\n");
}

#[test]
fn rejects_a_double_encoded_path() {
    // `%252e` decodes once to `%2e`. Decoding again is what turns a checked
    // string into a different opened path.
    let project = Project::new();
    assert_eq!(
        project.resolve("/%252e%252e/.env").unwrap_err(),
        AccessDenied::DoubleEncoded
    );
    assert_eq!(
        project.resolve("/app/%25%36%31.js").unwrap_err(),
        AccessDenied::DoubleEncoded
    );
}

#[test]
fn a_literal_percent_that_is_not_an_escape_is_allowed_through() {
    let project = Project::new();
    project.write("100%.js", "ok\n");
    assert_eq!(project.body("/100%25.js"), "ok\n");
}

#[test]
fn rejects_a_truncated_percent_escape() {
    let project = Project::new();
    assert!(matches!(
        project.resolve("/a%2").unwrap_err(),
        AccessDenied::InvalidPercentEncoding { .. }
    ));
    assert!(matches!(
        project.resolve("/a%zz.js").unwrap_err(),
        AccessDenied::InvalidPercentEncoding { .. }
    ));
    assert!(matches!(
        project.resolve("/a%").unwrap_err(),
        AccessDenied::InvalidPercentEncoding { .. }
    ));
}

#[test]
fn rejects_an_encoded_nul_byte() {
    // Poisoned NUL: a suffix check sees `.js`, `open(2)` sees `.env`.
    let project = Project::new();
    assert_eq!(
        project.resolve("/.env%00.js").unwrap_err(),
        AccessDenied::ForbiddenByte { byte: 0 }
    );
}

#[test]
fn rejects_encoded_control_characters() {
    let project = Project::new();
    for target in ["/a%0d%0ab.js", "/a%09b.js", "/a%7f.js"] {
        assert!(
            matches!(
                project.resolve(target).unwrap_err(),
                AccessDenied::ForbiddenByte { .. }
            ),
            "{target} was accepted"
        );
    }
}

#[test]
fn rejects_a_path_that_is_not_utf8_once_decoded() {
    let project = Project::new();
    assert_eq!(
        project.resolve("/%ff%fe.js").unwrap_err(),
        AccessDenied::NonUtf8
    );
}

#[test]
fn rejects_an_encoded_lone_surrogate() {
    let project = Project::new();
    assert_eq!(
        project.resolve("/%ed%a0%80.js").unwrap_err(),
        AccessDenied::NonUtf8
    );
}

// --- normalization ----------------------------------------------------------

#[test]
fn rejects_a_traversal_above_the_root() {
    let project = Project::new();
    for target in [
        "/../.env",
        "/../../etc/passwd",
        "/app/../../.env",
        "/..%2f..%2f.env",
        "/%2e%2e/%2e%2e/.env",
        "/..%5c..%5c.env",
    ] {
        assert_eq!(
            project.resolve(target).unwrap_err(),
            AccessDenied::Escape,
            "{target} was not treated as an escape"
        );
    }
}

#[test]
fn does_not_clamp_a_traversal_back_to_the_root() {
    // Clamping `..` to the root is a repair; the request asked for something
    // outside, and the answer is a refusal, not a substitution.
    let project = Project::new();
    project.write("index.html", "<!doctype html>\n");
    assert_eq!(
        project.resolve("/../index.html").unwrap_err(),
        AccessDenied::Escape
    );
}

#[test]
fn treats_a_backslash_as_a_separator_on_every_platform() {
    // CVE-2025-62522. `app\main.js` must reach the same file `app/main.js` does,
    // and `\` must never survive into a file name the deny list cannot see.
    let project = Project::new();
    project.write("app/main.js", "ok\n");
    assert_eq!(project.body("/app%5Cmain.js"), "ok\n");
    assert_eq!(project.body("/app/main.js%5C"), "ok\n");
}

#[test]
fn four_dots_are_a_literal_segment_not_a_traversal() {
    // `/....//` is only a traversal to a normalizer that rewrites it into one.
    let project = Project::new();
    project.write("..../inside.js", "inside\n");
    project.write(".env", "SECRET=1\n");
    // It reaches the literal `....` directory, and never the parent.
    assert_eq!(project.body("/....//inside.js"), "inside\n");
    assert_eq!(
        project.resolve("/....//nope.js").unwrap_err(),
        AccessDenied::NotFound
    );
    assert!(matches!(
        project.resolve("/....//.env").unwrap_err(),
        AccessDenied::Denied(PolicyDenial::DeniedByPattern { .. })
    ));
}

#[test]
fn rejects_the_filesystem_prefix() {
    let project = Project::new();
    for target in [
        "/@fs/etc/passwd",
        "/@fs/",
        "/./@fs/etc/passwd",
        "/app/../@fs/etc/passwd",
        "/@fs%2Fetc%2Fpasswd",
    ] {
        assert_eq!(
            project.resolve(target).unwrap_err(),
            AccessDenied::FilesystemPrefix,
            "{target} was not refused"
        );
    }
}

#[test]
fn rejects_a_path_deeper_than_the_segment_ceiling() {
    let project = Project::new();
    let deep = format!("/{}", "a/".repeat(MAX_PATH_SEGMENTS + 2));
    assert_eq!(project.resolve(&deep).unwrap_err(), AccessDenied::TooDeep);
}
