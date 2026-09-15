//! Which template sources are pinned, and which are refused, by name.

use super::*;

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

fn refusal(template: &str, integrity: Option<&str>) -> String {
    RemoteTemplate::parse(template, integrity)
        .expect_err("refused")
        .to_string()
}

#[test]
fn a_built_in_template_name_is_not_a_remote_source() {
    for word in ["react", "monorepo", "raect"] {
        assert_eq!(RemoteTemplate::parse(word, None).expect("parsed"), None);
    }
}

#[test]
fn github_shorthand_pinned_to_a_commit_is_a_git_source() {
    assert_eq!(
        RemoteTemplate::parse(&format!("github:acme/starter#{COMMIT}"), None).expect("parsed"),
        Some(RemoteTemplate::Git {
            url: "https://github.com/acme/starter.git".to_owned(),
            commit: COMMIT.to_owned(),
        })
    );
}

#[test]
fn a_git_source_without_a_full_commit_is_refused_saying_why() {
    assert!(refusal("github:acme/starter", None).contains("not pinned"));
    assert!(refusal("github:acme/starter#main", None).contains("branch or a tag"));
    assert!(refusal("github:acme/starter#0123abc", None).contains("short commit id"));
    assert!(
        refusal("git+https://example.com/starter.git#v1.2.0", None).contains("branch or a tag")
    );
}

#[test]
fn a_transport_that_is_not_https_or_file_is_refused() {
    for template in [
        format!("git+ssh://git@example.com/starter.git#{COMMIT}"),
        format!("git+http://example.com/starter.git#{COMMIT}"),
        "http://example.com/starter.tar.gz".to_owned(),
    ] {
        assert!(
            refusal(&template, Some(&format!("sha256:{}", "a".repeat(64)))).contains("scheme")
                || refusal(&template, None).contains("scheme"),
            "{template}"
        );
    }
}

#[test]
fn a_tarball_is_fetched_only_with_its_integrity() {
    assert!(refusal("https://example.com/starter.tar.gz", None).contains("--integrity"));
    let sha256 = format!("sha256:{}", "A".repeat(64));
    assert_eq!(
        RemoteTemplate::parse("https://example.com/starter.tgz", Some(&sha256)).expect("parsed"),
        Some(RemoteTemplate::Tarball {
            url: "https://example.com/starter.tgz".to_owned(),
            integrity: Digest::Sha256Hex("a".repeat(64)),
        })
    );
    let sha512 = format!("sha512-{}==", "b".repeat(86));
    assert!(matches!(
        RemoteTemplate::parse("https://example.com/starter.tar.gz", Some(&sha512)),
        Ok(Some(RemoteTemplate::Tarball {
            integrity: Digest::Sha512Base64(_),
            ..
        }))
    ));
}

#[test]
fn an_integrity_uf_cannot_check_or_does_not_need_is_refused() {
    assert!(refusal("https://example.com/starter.tar.gz", Some("md5-abc")).contains("integrity"));
    assert!(
        refusal(
            &format!("github:acme/starter#{COMMIT}"),
            Some(&format!("sha256:{}", "a".repeat(64)))
        )
        .contains("commit id is the pin")
    );
    assert!(refusal("react", Some("sha256:00")).contains("names no tarball"));
}

#[test]
fn a_url_that_is_neither_a_repository_nor_a_tarball_is_refused() {
    assert!(refusal("https://example.com/starter.zip", None).contains("neither"));
}

#[test]
fn the_copy_refuses_a_link_and_writes_nothing() {
    let from = Staging::new().expect("a staging directory");
    let to = Staging::new().expect("a second one");
    fs::write(from.path().join("a.js"), "export {};\n").expect("a file");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/etc/passwd", from.path().join("link")).expect("a link");
        let error = copy_into(from.path(), to.path(), false).expect_err("a link is refused");
        assert!(error.to_string().contains("symbolic link"), "{error}");
        assert!(
            !to.path().join("a.js").exists(),
            "the copy wrote a file anyway"
        );
    }
}

#[test]
fn the_copy_leaves_git_behind_and_refuses_to_overwrite_without_force() {
    let from = Staging::new().expect("a staging directory");
    let to = Staging::new().expect("a second one");
    fs::create_dir_all(from.path().join(".git")).expect(".git");
    fs::write(from.path().join(".git/HEAD"), "ref\n").expect("HEAD");
    fs::create_dir_all(from.path().join("app")).expect("app");
    fs::write(from.path().join("app/$page.js"), "export {};\n").expect("a page");

    let written = copy_into(from.path(), to.path(), false).expect("copied");

    assert_eq!(written, [to.path().join("app/$page.js")]);
    assert!(!to.path().join(".git").exists());
    let error = copy_into(from.path(), to.path(), false).expect_err("an overwrite is refused");
    assert!(error.to_string().contains("--force"), "{error}");
    copy_into(from.path(), to.path(), true).expect("--force overwrites");
}

#[test]
fn install_scripts_the_manifest_declares_are_named() {
    let root = Staging::new().expect("a staging directory");
    fs::write(
        root.path().join("package.json"),
        r#"{ "scripts": { "build": "uf build", "postinstall": "node setup.js" } }"#,
    )
    .expect("a manifest");

    assert_eq!(declared_install_scripts(root.path()), ["postinstall"]);
}
