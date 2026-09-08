use super::*;

#[test]
fn an_ordinary_path_is_borrowed_unchanged() {
    let path = "src/routes/dashboard/_uf.page.js";
    assert!(matches!(safe_path(path), Cow::Borrowed(_)));
    assert_eq!(safe_path(path), path);
}

#[test]
fn an_escape_sequence_in_a_filename_cannot_clear_the_screen() {
    // The acceptance case from ubugeeei-prod/uf#640: a file in a cloned
    // repository whose name is an ANSI sequence.
    let drawn = safe_path("src/\u{1b}[2Jevil.js");
    assert_eq!(drawn, "src/[2Jevil.js");
    assert!(!drawn.contains('\u{1b}'));
}

#[test]
fn a_carriage_return_cannot_rewrite_the_row_already_drawn() {
    let drawn = safe_path("src/a\rerror: everything is fine.js");
    assert!(!drawn.contains('\r'));
    assert_eq!(drawn, "src/aerror: everything is fine.js");
}

#[test]
fn every_control_character_goes_including_the_eight_bit_csi() {
    // `\u{9b}` is CSI as one byte, which a terminal in 8-bit mode obeys
    // exactly as it obeys `ESC [`. `char::is_control` covers C0, DEL and C1,
    // and that is the whole set this drops.
    for ch in ['\0', '\u{7}', '\n', '\r', '\u{1b}', '\u{7f}', '\u{9b}'] {
        let path = format!("src/a{ch}b.js");
        assert_eq!(safe_path(&path), "src/ab.js", "{ch:?} survived");
    }
}

#[test]
fn a_windows_separator_still_renders() {
    // Refusing `\` is right in a *resolver* — `uf_plugin::resolve` does it on
    // every platform — and wrong here: this is a path being drawn, and on
    // Windows that is the separator a reader would type back.
    let path = r"src\routes\page.js";
    assert!(matches!(safe_path(path), Cow::Borrowed(_)));
    assert_eq!(safe_path(path), path);
}

#[test]
fn a_legitimate_non_ascii_filename_still_renders() {
    for path in [
        "src/コンポーネント/ボタン.js",
        "src/composants/entrée.js",
        "src/компоненты/кнопка.js",
        "src/🎉/party.js",
    ] {
        assert!(
            matches!(safe_path(path), Cow::Borrowed(_)),
            "{path} was rewritten",
        );
        assert_eq!(safe_path(path), path);
    }
}

#[test]
fn a_long_path_is_cut_to_the_width_a_frame_draws() {
    let path = format!("{}/page.js", "deeply/".repeat(64));
    let drawn = safe_path(&path);
    assert!(crate::display_width(&drawn) <= MAX_PATH_WIDTH);
    // The end of the path is what a reader needs, so it is the end that
    // survives.
    assert!(drawn.ends_with("/page.js"), "{drawn}");
    assert!(drawn.starts_with("..."), "{drawn}");
}

#[test]
fn a_wide_path_is_measured_in_columns_and_not_in_scalars() {
    // Every scalar here is two columns wide, so a cut counted in `chars` would
    // let this out at twice the width it was given.
    let path = "あ".repeat(MAX_PATH_WIDTH);
    let drawn = safe_path(&path);
    assert!(crate::display_width(&drawn) <= MAX_PATH_WIDTH);
}

#[test]
fn a_path_of_zero_width_scalars_is_bounded_too() {
    // Combining marks measure nothing, so the column ceiling alone would let
    // an unbounded name through. Rule 4 wants the second bound.
    let path = "\u{200d}".repeat(MAX_PATH_SCALARS * 4);
    let drawn = safe_path(&path);
    assert!(drawn.chars().count() <= MAX_PATH_SCALARS + 3);
}

#[test]
fn an_ordinary_message_is_borrowed_unchanged() {
    let message = "`Foo` is not a function";
    assert!(matches!(safe_message(message), Cow::Borrowed(_)));
    assert_eq!(safe_message(message), message);
}

#[test]
fn a_message_quoting_a_module_cannot_steer_the_terminal_either() {
    // An RSC diagnostic names the module *inside* its sentence, so a frame
    // that only prepared its path field would still hand the escape over.
    let drawn = safe_message("client module `src/\u{1b}[2Jevil.js` imports server-only `db`");
    assert!(!drawn.contains('\u{1b}'));
    assert_eq!(
        drawn,
        "client module `src/[2Jevil.js` imports server-only `db`",
    );
}

#[test]
fn a_long_message_keeps_its_start() {
    let message = format!("cannot resolve `{}`", "a".repeat(MAX_MESSAGE_WIDTH * 2));
    let drawn = safe_message(&message);
    assert!(crate::display_width(&drawn) <= MAX_MESSAGE_WIDTH);
    assert!(drawn.starts_with("cannot resolve `a"), "{drawn}");
    assert!(drawn.ends_with("..."), "{drawn}");
}

#[test]
fn pushing_appends_rather_than_replacing() {
    let mut out = String::from("  --> ");
    push_safe_path(&mut out, "src/\rapp.js");
    out.push_str(": ");
    push_safe_message(&mut out, "a \u{1b}[31mred\u{1b}[0m message");
    assert_eq!(out, "  --> src/app.js: a [31mred[0m message");
}
