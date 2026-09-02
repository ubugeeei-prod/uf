use super::*;

fn glob(pattern: &str) -> DenyGlob {
    DenyGlob::compile(pattern).unwrap()
}

fn denies(pattern: &str, path: &str) -> bool {
    glob(pattern).matches(Utf8Path::new(path))
}

#[test]
fn a_literal_pattern_matches_only_itself() {
    assert!(denies("index.html", "index.html"));
    assert!(!denies("index.html", "index.htm"));
    assert!(!denies("index.html", "app/index.html.bak"));
}

#[test]
fn a_star_does_not_cross_a_separator() {
    assert!(denies("*.pem", "server.pem"));
    assert!(!denies("app/*.pem", "app/nested/server.pem"));
}

#[test]
fn an_unrooted_pattern_reaches_every_segment() {
    // `.env*` must deny `config/.env.local`, not only a root-level `.env`.
    assert!(denies(".env*", ".env"));
    assert!(denies(".env*", "config/.env.local"));
    assert!(denies("*.pem", "certs/deep/server.pem"));
}

#[test]
fn a_rooted_pattern_is_anchored() {
    assert!(denies("app/secret.js", "app/secret.js"));
    assert!(!denies("app/secret.js", "vendor/app/secret.js"));
}

#[test]
fn double_star_spans_any_number_of_segments() {
    assert!(denies("**/.git/**", ".git/config"));
    assert!(denies("**/.git/**", "a/b/.git/objects/ab/cd"));
    assert!(denies("**/.git/**", "a/.git"));
    assert!(!denies("**/.git/**", "a/git/config"));
}

#[test]
fn a_question_mark_matches_one_character() {
    assert!(denies("a?c.js", "abc.js"));
    assert!(!denies("a?c.js", "ac.js"));
    assert!(!denies("a?c.js", "abbc.js"));
}

#[test]
fn a_question_mark_matches_one_non_ascii_character() {
    // Byte-wise matching would split the two-byte `é` and quietly fail to deny.
    assert!(denies("caf?.key", "café.key"));
}

#[test]
fn a_backslash_in_a_pattern_is_a_separator() {
    assert!(denies("**\\.git\\**", "a/.git/config"));
}

#[test]
fn a_pathological_pattern_terminates() {
    // A backtracking matcher would go exponential here. This one is O(n*m).
    let pattern = "*".repeat(40) + "b";
    let text = "a".repeat(4000);
    assert!(!denies(&pattern, &text));
}

#[test]
fn rejects_a_pattern_over_the_byte_ceiling() {
    let pattern = "a".repeat(MAX_PATTERN_BYTES + 1);
    assert!(matches!(
        DenyGlob::compile(&pattern).unwrap_err(),
        PolicyError::PatternTooLong { .. }
    ));
}

fn project() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().canonicalize().unwrap()).unwrap();
    (dir, root)
}

#[test]
fn the_project_root_is_always_the_first_allow_root() {
    let (_guard, root) = project();
    let policy = FsPolicy::with_defaults(&root).unwrap();
    assert_eq!(policy.roots(), &[root]);
}

#[test]
fn allows_a_file_inside_the_root() {
    let (_guard, root) = project();
    std::fs::write(root.join("index.html"), "<!doctype html>").unwrap();
    let policy = FsPolicy::with_defaults(&root).unwrap();
    assert_eq!(policy.decide(&root.join("index.html")), Ok(()));
}

#[test]
fn denies_a_path_outside_every_root() {
    let (_guard, root) = project();
    let policy = FsPolicy::with_defaults(&root).unwrap();
    assert!(matches!(
        policy.decide(Utf8Path::new("/etc/passwd")).unwrap_err(),
        PolicyDenial::OutsideAllowedRoots { .. }
    ));
}

#[test]
fn root_containment_compares_whole_components() {
    // A textual prefix test would let a sibling directory whose name merely
    // starts with the root's name pass as "inside" it.
    let (_guard, root) = project();
    let policy = FsPolicy::with_defaults(&root).unwrap();
    let sibling = Utf8PathBuf::from(format!("{root}-secrets/key.txt"));
    assert!(matches!(
        policy.decide(&sibling).unwrap_err(),
        PolicyDenial::OutsideAllowedRoots { .. }
    ));
}

#[test]
fn deny_wins_over_a_path_inside_the_root() {
    let (_guard, root) = project();
    let policy = FsPolicy::with_defaults(&root).unwrap();
    assert!(matches!(
        policy.decide(&root.join(".env")).unwrap_err(),
        PolicyDenial::DeniedByPattern { .. }
    ));
}

#[test]
fn deny_wins_over_an_explicitly_allowed_extra_root() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().canonicalize().unwrap()).unwrap();
    let extra = root.join("shared");
    std::fs::create_dir(&extra).unwrap();
    std::fs::write(extra.join(".env"), "SECRET=1").unwrap();

    let policy = FsPolicy::new(&root, ["shared"], Vec::<&str>::new()).unwrap();
    assert!(matches!(
        policy.decide(&extra.join(".env")).unwrap_err(),
        PolicyDenial::DeniedByPattern { .. }
    ));
}

#[test]
fn an_extra_allow_root_widens_the_allowed_set() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().canonicalize().unwrap()).unwrap();
    let project_root = root.join("app");
    let shared = root.join("shared");
    std::fs::create_dir(&project_root).unwrap();
    std::fs::create_dir(&shared).unwrap();

    let strict = FsPolicy::with_defaults(&project_root).unwrap();
    assert!(strict.decide(&shared.join("lib.js")).is_err());

    let relaxed = FsPolicy::new(&project_root, ["../shared"], Vec::<&str>::new()).unwrap();
    assert_eq!(relaxed.decide(&shared.join("lib.js")), Ok(()));
}

#[test]
fn an_unresolvable_allow_root_is_a_startup_error() {
    let (_guard, root) = project();
    assert!(matches!(
        FsPolicy::new(&root, ["does-not-exist"], Vec::<&str>::new()).unwrap_err(),
        PolicyError::UnresolvableRoot { .. }
    ));
}

#[test]
fn rejects_more_patterns_than_the_ceiling() {
    let (_guard, root) = project();
    let patterns: Vec<String> = (0..=MAX_DENY_PATTERNS).map(|n| format!("p{n}")).collect();
    assert!(matches!(
        FsPolicy::new(&root, Vec::<&str>::new(), patterns).unwrap_err(),
        PolicyError::TooManyPatterns { .. }
    ));
}

#[test]
fn the_default_deny_list_covers_the_documented_classes() {
    let (_guard, root) = project();
    let policy = FsPolicy::with_defaults(&root).unwrap();
    for relative in [
        ".env",
        ".env.local",
        ".env.production",
        "config/.env",
        ".git/config",
        "vendor/.git/HEAD",
        "certs/server.pem",
        "certs/server.key",
        "certs/server.crt",
        ".uf/dev-server.json",
    ] {
        assert!(
            policy.deny_pattern_for(Utf8Path::new(relative)).is_some(),
            "{relative} was not denied"
        );
    }
}

#[test]
fn the_default_deny_list_leaves_ordinary_files_alone() {
    let (_guard, root) = project();
    let policy = FsPolicy::with_defaults(&root).unwrap();
    for relative in [
        "index.html",
        "app/main.js",
        "src/components/Button.js",
        "assets/logo.svg",
        "environment.js",
        "docs/git.md",
    ] {
        assert_eq!(
            policy.deny_pattern_for(Utf8Path::new(relative)),
            None,
            "{relative} was denied"
        );
    }
}

#[test]
fn a_project_can_only_add_to_the_deny_list() {
    let (_guard, root) = project();
    let policy = FsPolicy::new(&root, Vec::<&str>::new(), ["*.secret"]).unwrap();
    assert!(policy.deny_pattern_for(Utf8Path::new("a.secret")).is_some());
    // The built-ins survive a project-supplied list.
    assert!(policy.deny_pattern_for(Utf8Path::new(".env")).is_some());
    let patterns: Vec<&str> = policy.deny_patterns().collect();
    assert_eq!(patterns.len(), DEFAULT_DENY.len() + 1);
    assert_eq!(patterns.last(), Some(&"*.secret"));
}

#[test]
fn a_duplicate_of_a_built_in_pattern_is_not_stored_twice() {
    let (_guard, root) = project();
    let policy = FsPolicy::new(&root, Vec::<&str>::new(), [".env*"]).unwrap();
    assert_eq!(policy.deny_patterns().count(), DEFAULT_DENY.len());
}
