use super::*;

#[test]
fn the_name_changes_when_the_bytes_do() {
    // The whole reason the name carries a digest: it is served with a
    // far-future cache header, so a changed file that kept its name is a file
    // no reader ever sees again.
    let before = hashed_name("hero", b"one", &[], "png");
    let after = hashed_name("hero", b"two", &[], "png");

    assert_ne!(before, after);
}

#[test]
fn the_name_changes_when_a_parameter_does() {
    // Same source, different width: two different files, so two different
    // names. Without the parameters in the digest the second write would land
    // on the first file and every reference to either would get one of them.
    let narrow = hashed_name_with_suffix("hero", b"same", &[&640u32.to_le_bytes()], "-640w", "jpg");
    let wide = hashed_name_with_suffix("hero", b"same", &[&1280u32.to_le_bytes()], "-1280w", "jpg");

    assert_ne!(digest_of(&narrow), digest_of(&wide));
}

#[test]
fn the_same_input_always_gives_the_same_name() {
    // A rebuild that changes nothing must not invalidate a CDN.
    let once = hashed_name("hero", b"bytes", &[b"a", b"b"], "png");
    let twice = hashed_name("hero", b"bytes", &[b"a", b"b"], "png");

    assert_eq!(once, twice);
}

#[test]
fn components_are_framed_rather_than_joined() {
    // `("a", "bc")` and `("ab", "c")` are different inputs and must hash
    // differently. A separator-joined construction makes them identical the
    // moment a value contains the separator, and these values come from a
    // dependency's file names.
    let left = hashed_name("hero", b"x", &[b"a", b"bc"], "png");
    let right = hashed_name("hero", b"x", &[b"ab", b"c"], "png");

    assert_ne!(left, right);
}

#[test]
fn the_stem_is_reduced_to_something_a_url_can_carry() {
    let name = hashed_name("../../etc/pass wd", b"x", &[], "png");

    assert!(!name.contains('/'), "{name}");
    assert!(!name.contains(' '), "{name}");
    assert!(!name.starts_with('.'), "{name}");
}

#[test]
fn a_stem_with_nothing_usable_in_it_still_gets_a_name() {
    // A file named entirely in a script the reducer drops must not produce
    // `.1a2b3c4d.png`, which is a hidden file most static servers refuse.
    let name = hashed_name("……", b"x", &[], "png");

    assert!(name.starts_with("asset."), "{name}");
}

#[test]
fn the_digest_is_the_documented_width() {
    let name = hashed_name("hero", b"x", &[], "png");

    assert_eq!(digest_of(&name).len(), NAME_DIGITS);
    assert!(
        digest_of(&name).chars().all(|c| c.is_ascii_hexdigit()),
        "{name}"
    );
}

/// The hexadecimal part of `stem.<digest><suffix>.extension`.
fn digest_of(name: &str) -> String {
    name.split('.')
        .nth(1)
        .expect("a name has a digest segment")
        .chars()
        .take_while(char::is_ascii_hexdigit)
        .collect()
}
