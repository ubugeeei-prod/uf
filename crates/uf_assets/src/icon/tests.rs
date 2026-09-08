use camino::Utf8PathBuf;

use super::{IconError, IconRequest, SpriteRequest, icon, sprite};

fn temp() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    (dir, path)
}

const STAR: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="m12 2 3 7h7l-6 4 2 7-6-4-6 4 2-7-6-4h7z"/></svg>"##;

fn write(dir: &Utf8PathBuf, name: &str, body: &str) -> Utf8PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, body).unwrap();
    path
}

#[test]
fn an_svg_becomes_a_symbol_carrying_its_geometry() {
    let (_guard, dir) = temp();
    let path = write(&dir, "star.svg", STAR);
    let asset = icon(&IconRequest {
        source: &path,
        name: "star",
    })
    .unwrap();

    assert_eq!(asset.name, "star");
    assert_eq!(asset.view_box, "0 0 24 24");
    assert_eq!((asset.width, asset.height), (24.0, 24.0));
    assert!(asset.id.starts_with("uf-icon-star-"), "{}", asset.id);
    assert!(
        asset.symbol.starts_with("<symbol id=\""),
        "{}",
        asset.symbol
    );
    assert!(asset.symbol.contains("<path d=\"m12 2"), "{}", asset.symbol);
    // The root `<svg>` element itself is gone: a `<symbol>` replaces it, and
    // an `<svg>` inside a sprite would be a second document.
    assert!(!asset.symbol.contains("<svg"), "{}", asset.symbol);
}

#[test]
fn a_self_closing_svg_is_an_empty_symbol_rather_than_a_parse_error() {
    let (_guard, dir) = temp();
    let path = write(&dir, "empty.svg", r#"<svg viewBox="0 0 4 4" fill="red" />"#);
    let asset = icon(&IconRequest {
        source: &path,
        name: "empty",
    })
    .unwrap();
    assert_eq!(asset.view_box, "0 0 4 4");
    assert!(asset.symbol.contains("fill=\"red\""), "{}", asset.symbol);
    assert!(asset.symbol.ends_with("></symbol>"), "{}", asset.symbol);
}

#[test]
fn a_filled_icon_does_not_inherit_a_fill_it_never_declared() {
    // The bug an unconditional `fill="none"` on the symbol would cause: every
    // icon that fills rather than strokes would render as nothing at all.
    let (_guard, dir) = temp();
    let path = write(
        &dir,
        "dot.svg",
        r#"<svg viewBox="0 0 8 8"><circle cx="4" cy="4" r="3"/></svg>"#,
    );
    let asset = icon(&IconRequest {
        source: &path,
        name: "dot",
    })
    .unwrap();
    assert!(!asset.symbol.contains("fill="), "{}", asset.symbol);
}

#[test]
fn a_width_and_height_stand_in_for_a_missing_viewbox() {
    let (_guard, dir) = temp();
    let path = write(
        &dir,
        "box.svg",
        r#"<svg width="16" height="16"><rect width="16" height="16"/></svg>"#,
    );
    let asset = icon(&IconRequest {
        source: &path,
        name: "box",
    })
    .unwrap();
    assert_eq!(asset.view_box, "0 0 16 16");
}

#[test]
fn an_svg_with_no_dimensions_at_all_is_a_message_rather_than_a_guess() {
    let (_guard, dir) = temp();
    let path = write(&dir, "flat.svg", r#"<svg><rect/></svg>"#);
    let error = icon(&IconRequest {
        source: &path,
        name: "flat",
    })
    .unwrap_err();
    assert!(matches!(error, IconError::Malformed { .. }), "{error}");
    assert!(error.to_string().contains("viewBox"), "{error}");
}

#[test]
fn editing_an_icon_changes_its_symbol_id() {
    // The sprite is content-hashed, so a stale symbol under a name that has
    // not changed would be served forever.
    let (_guard, dir) = temp();
    let first = icon(&IconRequest {
        source: &write(&dir, "star.svg", STAR),
        name: "star",
    })
    .unwrap();
    let second = icon(&IconRequest {
        source: &write(
            &dir,
            "star.svg",
            &STAR.replace("stroke-width=\"2\"", "stroke-width=\"1\""),
        ),
        name: "star",
    })
    .unwrap();
    assert_ne!(first.id, second.id);
}

#[test]
fn two_icons_that_define_the_same_id_do_not_collide_in_one_sprite() {
    // The normal case for anything with a gradient, and the bug a hand-rolled
    // sprite always has: the second `id="a"` wins for both.
    let (_guard, dir) = temp();
    let gradient = |stop: &str| {
        format!(
            r##"<svg viewBox="0 0 24 24"><defs><linearGradient id="a"><stop stop-color="{stop}"/></linearGradient></defs><rect width="24" height="24" fill="url(#a)"/></svg>"##
        )
    };
    let one = icon(&IconRequest {
        source: &write(&dir, "one.svg", &gradient("#f00")),
        name: "one",
    })
    .unwrap();
    let two = icon(&IconRequest {
        source: &write(&dir, "two.svg", &gradient("#00f")),
        name: "two",
    })
    .unwrap();

    assert!(
        one.symbol.contains(&format!("id=\"{}-a\"", one.id)),
        "{}",
        one.symbol
    );
    assert!(
        one.symbol.contains(&format!("url(#{}-a)", one.id)),
        "{}",
        one.symbol
    );
    assert!(
        !two.symbol.contains(&format!("{}-a", one.id)),
        "{}",
        two.symbol
    );
}

#[test]
fn a_sprite_holds_exactly_the_icons_the_build_reached() {
    let (_guard, dir) = temp();
    let star = icon(&IconRequest {
        source: &write(&dir, "star.svg", STAR),
        name: "star",
    })
    .unwrap();
    let out = dir.join("out");
    let built = sprite(&SpriteRequest {
        icons: std::slice::from_ref(&star),
        out_dir: &out,
    })
    .unwrap();

    assert_eq!(built.symbols, 1);
    assert!(built.markup.contains(&star.id), "{}", built.markup);
    assert!(built.markup.contains("aria-hidden"), "{}", built.markup);
    let written = std::fs::read_to_string(out.join(&built.file)).unwrap();
    assert_eq!(written, built.markup);
    assert_eq!(built.bytes, written.len() as u64);
}

#[test]
fn a_sprite_is_the_same_bytes_whatever_order_the_bundler_resolved_in() {
    let (_guard, dir) = temp();
    let star = icon(&IconRequest {
        source: &write(&dir, "star.svg", STAR),
        name: "star",
    })
    .unwrap();
    let dot = icon(&IconRequest {
        source: &write(
            &dir,
            "dot.svg",
            r#"<svg viewBox="0 0 8 8"><circle cx="4" cy="4" r="3"/></svg>"#,
        ),
        name: "dot",
    })
    .unwrap();
    let out = dir.join("out");

    let forwards = sprite(&SpriteRequest {
        icons: &[star.clone(), dot.clone()],
        out_dir: &out,
    })
    .unwrap();
    let backwards = sprite(&SpriteRequest {
        icons: &[dot, star],
        out_dir: &out,
    })
    .unwrap();
    assert_eq!(forwards.file, backwards.file);
}

#[test]
fn a_script_in_an_icon_is_refused_rather_than_stripped() {
    // Stripped markup renders differently from the file in the repository and
    // nobody finds out until it looks wrong.
    let (_guard, dir) = temp();
    for (name, body, expected) in [
        (
            "script.svg",
            r#"<svg viewBox="0 0 1 1"><script>alert(1)</script></svg>"#,
            "<script>",
        ),
        (
            "foreign.svg",
            r#"<svg viewBox="0 0 1 1"><foreignObject><p>hi</p></foreignObject></svg>"#,
            "<foreignObject>",
        ),
        (
            "handler.svg",
            r#"<svg viewBox="0 0 1 1"><rect onload="alert(1)"/></svg>"#,
            "onload",
        ),
        (
            "url.svg",
            r#"<svg viewBox="0 0 1 1"><a href="javascript:alert(1)"><rect/></a></svg>"#,
            "javascript: URL",
        ),
        (
            "remote.svg",
            r#"<svg viewBox="0 0 1 1"><use href="https://example.com/x.svg#a"/></svg>"#,
            "https://example.com/x.svg#a",
        ),
    ] {
        let error = icon(&IconRequest {
            source: &write(&dir, name, body),
            name: "x",
        })
        .unwrap_err();
        assert!(
            matches!(error, IconError::Refused { .. }),
            "{name}: {error}"
        );
        assert!(error.to_string().contains(expected), "{name}: {error}");
    }
}

#[test]
fn an_internal_reference_is_not_mistaken_for_an_external_one() {
    let (_guard, dir) = temp();
    let path = write(
        &dir,
        "linked.svg",
        r##"<svg viewBox="0 0 24 24"><defs><path id="p" d="M0 0h1v1z"/></defs><use href="#p"/></svg>"##,
    );
    let asset = icon(&IconRequest {
        source: &path,
        name: "linked",
    })
    .unwrap();
    assert!(
        asset.symbol.contains(&format!("href=\"#{}-p\"", asset.id)),
        "{}",
        asset.symbol
    );
}

#[test]
fn a_file_that_is_not_an_svg_is_a_message_naming_it() {
    let (_guard, dir) = temp();
    let path = write(&dir, "nope.svg", "just some text");
    let error = icon(&IconRequest {
        source: &path,
        name: "nope",
    })
    .unwrap_err();
    assert!(error.to_string().contains("no <svg> element"), "{error}");
}

#[test]
fn an_icon_with_a_multibyte_character_in_it_is_read_rather_than_panicked_over() {
    // The scan for `on…=` walked every byte offset and sliced the string at
    // `index + 3` before checking whether the window matched, so any icon
    // carrying a non-ASCII character — a `<title>` in a language with an
    // accent, which is exactly what a labelled icon set ships — sliced the
    // string in the middle of a code point and panicked. A panic in the asset
    // service takes the whole build's asset pass with it.
    let (_guard, dir) = temp();
    let path = write(
        &dir,
        "café.svg",
        r#"<svg viewBox="0 0 8 8"><title>café — préféré</title><circle cx="4" cy="4" r="3"/></svg>"#,
    );

    let asset = icon(&IconRequest {
        source: &path,
        name: "cafe",
    })
    .unwrap();

    assert_eq!(asset.view_box, "0 0 8 8");
    assert!(asset.symbol.contains("café"), "{}", asset.symbol);
}

#[test]
fn an_event_handler_is_still_refused_when_the_file_is_not_ascii() {
    // The other half of the fix: making the scan boundary-safe must not make
    // it blind. This file has both a multi-byte character and an `onload`.
    let (_guard, dir) = temp();
    let path = write(
        &dir,
        "trap.svg",
        r#"<svg viewBox="0 0 8 8"><title>café</title><circle onload="steal()" r="3"/></svg>"#,
    );

    let error = icon(&IconRequest {
        source: &path,
        name: "trap",
    })
    .unwrap_err();

    assert!(
        matches!(&error, IconError::Refused { found, .. } if found.contains("onload")),
        "{error}"
    );
}
