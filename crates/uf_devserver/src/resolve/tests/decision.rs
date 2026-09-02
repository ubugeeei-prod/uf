//! The access decision itself: deny patterns, allow roots, and symlinks.

use super::*;

#[test]
fn denies_a_dotenv_file_however_it_is_spelled() {
    let project = Project::new();
    project.write(".env", "SECRET=1\n");
    for target in [
        "/.env",
        "/.env?raw",
        "/.env?raw??",
        "/.env?import&raw??",
        "/.env?inline",
        "/.env%5C",
        "/.env/",
        "/./.env",
        "/app/../.env",
    ] {
        assert!(
            matches!(
                project.resolve(target).unwrap_err(),
                AccessDenied::Denied(PolicyDenial::DeniedByPattern { .. })
            ),
            "{target} was not denied"
        );
    }
}

#[test]
fn a_denied_name_answers_the_same_whether_or_not_it_exists() {
    // Otherwise the deny list is an existence oracle for the project's secrets.
    let present = Project::new();
    present.write(".env", "SECRET=1\n");
    let absent = Project::new();
    assert_eq!(
        present.resolve("/.env").unwrap_err(),
        absent.resolve("/.env").unwrap_err()
    );
}

#[test]
fn denies_git_metadata() {
    let project = Project::new();
    project.write(".git/config", "[core]\n");
    assert!(matches!(
        project.resolve("/.git/config").unwrap_err(),
        AccessDenied::Denied(PolicyDenial::DeniedByPattern { .. })
    ));
}

#[test]
fn denies_key_material() {
    let project = Project::new();
    project.write("certs/server.pem", "-----BEGIN\n");
    assert!(matches!(
        project.resolve("/certs/server.pem").unwrap_err(),
        AccessDenied::Denied(PolicyDenial::DeniedByPattern { .. })
    ));
}

#[test]
fn denies_the_dev_server_state_directory() {
    let project = Project::new();
    project.write(".uf/dev-server.json", "{}\n");
    assert!(matches!(
        project.resolve("/.uf/dev-server.json").unwrap_err(),
        AccessDenied::Denied(PolicyDenial::DeniedByPattern { .. })
    ));
}

#[test]
#[cfg(unix)]
fn denies_a_symlink_that_points_out_of_the_root() {
    // Lexical normalization alone cannot see this: every segment is innocent,
    // and only `canonicalize` reveals where the path lands.
    let project = Project::new();
    std::os::unix::fs::symlink("/etc/passwd", project.root.join("passwd.js")).unwrap();
    assert!(matches!(
        project.resolve("/passwd.js").unwrap_err(),
        AccessDenied::Denied(PolicyDenial::OutsideAllowedRoots { .. })
    ));
}

#[test]
#[cfg(unix)]
fn denies_a_symlinked_directory_that_points_out_of_the_root() {
    let outside = Project::new();
    outside.write("secret.js", "leaked\n");
    let project = Project::new();
    std::os::unix::fs::symlink(outside.root.as_std_path(), project.root.join("escape")).unwrap();
    assert!(matches!(
        project.resolve("/escape/secret.js").unwrap_err(),
        AccessDenied::Denied(PolicyDenial::OutsideAllowedRoots { .. })
    ));
}

#[test]
#[cfg(unix)]
fn allows_a_symlink_that_stays_inside_the_root() {
    let project = Project::new();
    project.write("app/main.js", "ok\n");
    std::os::unix::fs::symlink("app/main.js", project.root.join("alias.js")).unwrap();
    assert_eq!(project.body("/alias.js"), "ok\n");
}

#[test]
#[cfg(unix)]
fn denies_a_symlink_whose_target_matches_the_deny_list() {
    // The decision runs on the canonical path, so the deny list follows the
    // link rather than being fooled by the innocent-looking link name.
    let project = Project::new();
    project.write(".env", "SECRET=1\n");
    std::os::unix::fs::symlink(".env", project.root.join("config.js")).unwrap();
    assert!(matches!(
        project.resolve("/config.js").unwrap_err(),
        AccessDenied::Denied(PolicyDenial::DeniedByPattern { .. })
    ));
}

#[test]
fn a_missing_file_is_not_found() {
    let project = Project::new();
    assert_eq!(
        project.resolve("/app/missing.js").unwrap_err(),
        AccessDenied::NotFound
    );
}

#[test]
fn a_directory_is_not_a_regular_file() {
    let project = Project::new();
    project.write("app/main.js", "ok\n");
    assert!(matches!(
        project.resolve("/app").unwrap_err(),
        AccessDenied::NotARegularFile { .. }
    ));
}

#[test]
fn a_path_through_a_file_is_not_found() {
    let project = Project::new();
    project.write("app/main.js", "ok\n");
    assert_eq!(
        project.resolve("/app/main.js/extra").unwrap_err(),
        AccessDenied::NotFound
    );
}

#[test]
fn an_invalid_target_never_reaches_the_filesystem() {
    let project = Project::new();
    project.write("index.html", "<!doctype html>\n");
    assert!(matches!(
        project.resolve("http://evil.test/index.html").unwrap_err(),
        AccessDenied::InvalidTarget(TargetError::NotOriginForm)
    ));
    assert!(matches!(
        project.resolve("*").unwrap_err(),
        AccessDenied::InvalidTarget(TargetError::AsteriskForm)
    ));
}

#[test]
fn a_prepared_policy_and_the_convenience_entry_point_agree() {
    let project = Project::new();
    project.write("app/main.js", "ok\n");
    project.write(".env", "SECRET=1\n");
    let policy = FsPolicy::with_defaults(&project.root).unwrap();
    for target in ["/app/main.js", "/.env", "/../.env", "/@fs/etc/passwd"] {
        let parsed = RequestTarget::parse(target).unwrap();
        let direct = resolve_with_policy(&policy, &parsed);
        let convenience = project.resolve(target);
        assert_eq!(
            direct.is_ok(),
            convenience.is_ok(),
            "{target} disagreed between entry points"
        );
        if let (Err(left), Err(right)) = (direct, convenience) {
            assert_eq!(left, right, "{target} produced different refusals");
        }
    }
}
