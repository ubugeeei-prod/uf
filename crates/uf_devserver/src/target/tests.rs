use super::*;

fn parse(raw: &str) -> Result<RequestTarget<'_>, TargetError> {
    RequestTarget::parse(raw)
}

#[test]
fn accepts_a_plain_origin_form_path() {
    let target = parse("/app/main.js").unwrap();
    assert_eq!(target.path(), "/app/main.js");
    assert_eq!(target.query(), None);
    assert_eq!(target.loader(), Loader::Module);
}

#[test]
fn accepts_a_root_target() {
    assert_eq!(parse("/").unwrap().path(), "/");
}

#[test]
fn splits_the_query_off_the_path() {
    let target = parse("/app/main.js?import&t=1700000000").unwrap();
    assert_eq!(target.path(), "/app/main.js");
    assert_eq!(target.query(), Some("import&t=1700000000"));
}

#[test]
fn keeps_the_path_percent_encoded() {
    // Decoding is `resolve`'s job and happens exactly once, there.
    assert_eq!(parse("/caf%C3%A9.js").unwrap().path(), "/caf%C3%A9.js");
}

#[test]
fn rejects_an_empty_target() {
    assert_eq!(parse("").unwrap_err(), TargetError::Empty);
}

#[test]
fn rejects_a_target_over_the_byte_ceiling() {
    let raw = format!("/{}", "a".repeat(MAX_TARGET_BYTES));
    assert!(matches!(
        parse(&raw).unwrap_err(),
        TargetError::TooLong { .. }
    ));
}

#[test]
fn rejects_the_asterisk_form() {
    assert_eq!(parse("*").unwrap_err(), TargetError::AsteriskForm);
}

#[test]
fn rejects_an_absolute_form_target() {
    // CVE-2025-32395: an absolute-form target that a server "fixes up" into a
    // path is a path the access check never saw.
    assert_eq!(
        parse("http://evil.test/.env").unwrap_err(),
        TargetError::NotOriginForm
    );
    assert_eq!(
        parse("https://evil.test/.env").unwrap_err(),
        TargetError::NotOriginForm
    );
}

#[test]
fn rejects_an_authority_form_target() {
    assert_eq!(
        parse("evil.test:443").unwrap_err(),
        TargetError::NotOriginForm
    );
}

#[test]
fn rejects_a_network_path_reference() {
    assert_eq!(
        parse("//evil.test/.env").unwrap_err(),
        TargetError::NetworkPathReference
    );
}

#[test]
fn rejects_a_bare_relative_target() {
    assert_eq!(parse(".env").unwrap_err(), TargetError::NotOriginForm);
}

#[test]
fn rejects_an_embedded_nul() {
    assert!(matches!(
        parse("/.env\0.js").unwrap_err(),
        TargetError::ForbiddenByte { byte: 0, .. }
    ));
}

#[test]
fn rejects_control_characters() {
    for raw in ["/a\rb", "/a\nb", "/a\tb", "/a\x0bb", "/a\x7fb"] {
        assert!(
            matches!(parse(raw).unwrap_err(), TargetError::ForbiddenByte { .. }),
            "{raw:?} was accepted"
        );
    }
}

#[test]
fn rejects_a_raw_space() {
    assert!(matches!(
        parse("/a file.js").unwrap_err(),
        TargetError::ForbiddenByte { byte: 0x20, .. }
    ));
}

#[test]
fn rejects_a_raw_non_ascii_byte() {
    assert!(matches!(
        parse("/café.js").unwrap_err(),
        TargetError::ForbiddenByte { .. }
    ));
}

#[test]
fn rejects_a_fragment_rather_than_stripping_it() {
    // Stripping is a repair, and repairs are how the checked string stops
    // describing the opened path.
    assert_eq!(
        parse("/app.js#/../.env").unwrap_err(),
        TargetError::Fragment
    );
}

#[test]
fn treats_a_backslash_as_an_ordinary_target_byte() {
    // The separator decision belongs to `resolve`, on the decoded path. The
    // grammar gate must not reject it, or the CVE-2025-62522 case never reaches
    // the normalizer that proves it is handled.
    assert_eq!(parse("/.env\\").unwrap().path(), "/.env\\");
}

#[test]
fn defaults_to_the_module_loader() {
    assert_eq!(parse("/a.js").unwrap().loader(), Loader::Module);
    assert_eq!(parse("/a.js?").unwrap().loader(), Loader::Module);
    assert_eq!(parse("/a.js?t=1").unwrap().loader(), Loader::Module);
}

#[test]
fn selects_each_loader_by_exact_key() {
    assert_eq!(parse("/a.js?raw").unwrap().loader(), Loader::Raw);
    assert_eq!(parse("/a.js?inline").unwrap().loader(), Loader::Inline);
    assert_eq!(parse("/a.js?url").unwrap().loader(), Loader::Url);
    assert_eq!(parse("/a.js?worker").unwrap().loader(), Loader::Worker);
}

#[test]
fn treats_import_as_neutral() {
    assert_eq!(parse("/a.js?import").unwrap().loader(), Loader::Module);
    assert_eq!(parse("/a.js?import&raw").unwrap().loader(), Loader::Raw);
}

#[test]
fn does_not_match_a_loader_by_suffix() {
    // CVE-2025-30208 in one assertion: `raw??` is not the key `raw`, and a
    // closed-table lookup cannot be talked into thinking otherwise.
    assert_eq!(parse("/a.js?raw??").unwrap().loader(), Loader::Module);
    assert_eq!(parse("/a.js?rawx").unwrap().loader(), Loader::Module);
    assert_eq!(parse("/a.js?xraw").unwrap().loader(), Loader::Module);
    assert_eq!(parse("/a.js?raw.js").unwrap().loader(), Loader::Module);
    assert_eq!(
        parse("/a.js?import&raw??").unwrap().loader(),
        Loader::Module
    );
}

#[test]
fn does_not_percent_decode_the_query_to_find_a_loader() {
    // Decoding the query would put a second, differently-spelled decision in
    // front of the one `resolve` makes.
    assert_eq!(parse("/a.js?%72aw").unwrap().loader(), Loader::Module);
}

#[test]
fn repeating_one_loader_key_is_not_a_conflict() {
    assert_eq!(parse("/a.js?raw&raw").unwrap().loader(), Loader::Raw);
}

#[test]
fn rejects_two_different_loader_keys() {
    assert_eq!(
        parse("/a.js?raw&inline").unwrap_err(),
        TargetError::ConflictingLoaders {
            first: "raw",
            second: "inline",
        }
    );
}

#[test]
fn ignores_a_loader_key_that_carries_a_value() {
    assert_eq!(parse("/a.js?raw=1").unwrap().loader(), Loader::Raw);
}

#[test]
fn loader_names_are_stable() {
    assert_eq!(Loader::Module.as_str(), "module");
    assert_eq!(Loader::Raw.as_str(), "raw");
    assert_eq!(Loader::Inline.as_str(), "inline");
    assert_eq!(Loader::Url.as_str(), "url");
    assert_eq!(Loader::Worker.as_str(), "worker");
}

#[test]
fn parsing_is_idempotent_on_its_own_output() {
    for raw in ["/a.js", "/a/b/c.js?import", "/%20.js", "/a.js?raw"] {
        let once = parse(raw).unwrap();
        let twice = parse(raw).unwrap();
        assert_eq!(once, twice);
    }
}
