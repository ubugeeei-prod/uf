//! Display width, alignment, and the allocation-free text helpers.

use super::*;

#[test]
fn ascii_is_one_column_per_character() {
    assert_eq!(display_width("uf build"), 8);
    assert_eq!(char_width('a'), 1);
    assert_eq!(char_width(' '), 1);
}

#[test]
fn the_empty_string_is_zero_columns() {
    assert_eq!(display_width(""), 0);
}

#[test]
fn cjk_ideographs_are_two_columns() {
    assert_eq!(char_width('漢'), 2);
    assert_eq!(display_width("漢字"), 4);
    assert_eq!(display_width("日本語のパス.js"), 12 + 3);
}

#[test]
fn hangul_and_kana_are_two_columns() {
    assert_eq!(char_width('한'), 2);
    assert_eq!(char_width('ア'), 2);
    assert_eq!(char_width('ｱ'), 1, "halfwidth kana stays narrow");
}

#[test]
fn fullwidth_forms_are_two_columns() {
    assert_eq!(display_width("ＡＢ"), 4);
    assert_eq!(char_width('　'), 2, "ideographic space");
}

#[test]
fn combining_marks_add_no_columns() {
    assert_eq!(display_width("e\u{0301}"), 1);
    assert_eq!(display_width("a\u{0300}\u{0301}\u{0302}"), 1);
    assert_eq!(char_width('\u{0301}'), 0);
}

#[test]
fn devanagari_and_thai_marks_add_no_columns() {
    assert_eq!(display_width("क\u{094d}ष"), 2);
    assert_eq!(display_width("ก\u{0e34}"), 1);
}

#[test]
fn control_characters_add_no_columns() {
    assert_eq!(display_width("a\u{7}b"), 2);
    assert_eq!(char_width('\n'), 0);
    assert_eq!(char_width('\t'), 0);
    assert_eq!(char_width('\0'), 0);
}

#[test]
fn emoji_are_two_columns() {
    assert_eq!(char_width('🚀'), 2);
    assert_eq!(display_width("🚀🚀"), 4);
    assert_eq!(char_width('✓'), 1, "dingbats stay narrow");
}

#[test]
fn a_variation_selector_widens_a_narrow_base() {
    assert_eq!(display_width("\u{2764}"), 1);
    assert_eq!(display_width("\u{2764}\u{fe0f}"), 2);
}

#[test]
fn a_keycap_sequence_is_two_columns() {
    assert_eq!(display_width("1\u{fe0f}\u{20e3}"), 2);
}

#[test]
fn a_zero_width_joiner_sequence_stays_two_columns() {
    assert_eq!(display_width("\u{1f469}\u{200d}\u{1f4bb}"), 2);
    assert_eq!(
        display_width("\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f466}"),
        2
    );
}

#[test]
fn a_regional_indicator_pair_is_two_columns() {
    assert_eq!(display_width("\u{1f1ef}\u{1f1f5}"), 2);
}

#[test]
fn ansi_sequences_do_not_count_toward_width() {
    assert_eq!(display_width("\x1b[31mred\x1b[0m"), 3);
    assert_eq!(display_width("\x1b[1;38;5;75mstyled\x1b[0m"), 6);
    assert_eq!(display_width("\x1b]8;;https://example.com\x07link"), 4);
}

#[test]
fn a_truncated_ansi_sequence_does_not_panic() {
    assert_eq!(display_width("\x1b"), 0);
    assert_eq!(display_width("\x1b["), 0);
    assert_eq!(display_width("\x1b[31"), 0);
}

#[test]
fn truncation_never_splits_a_character() {
    assert_eq!(truncate_to_width("漢字です", 3), "漢");
    assert_eq!(truncate_to_width("漢字です", 4), "漢字");
    assert_eq!(truncate_to_width("abc", 10), "abc");
    assert_eq!(truncate_to_width("abc", 0), "");
}

#[test]
fn padding_uses_display_width_not_byte_length() {
    let mut out = String::new();
    push_padded(&mut out, "漢字", 6, Align::Left);
    assert_eq!(out, "漢字  ");
    assert_eq!(display_width(&out), 6);

    out.clear();
    push_padded(&mut out, "ab", 5, Align::Right);
    assert_eq!(out, "   ab");

    out.clear();
    push_padded(&mut out, "ab", 6, Align::Center);
    assert_eq!(out, "  ab  ");
}

#[test]
fn padding_never_truncates_an_oversized_cell() {
    let mut out = String::new();
    push_padded(&mut out, "a-very-long-cell", 4, Align::Left);
    assert_eq!(out, "a-very-long-cell");
}

#[test]
fn padded_columns_line_up_across_scripts() {
    let width = 12;
    for text in ["src/app.js", "src/日本.js", "src/🚀.js", "e\u{0301}.js"] {
        let mut out = String::new();
        push_padded(&mut out, text, width, Align::Left);
        assert_eq!(display_width(&out), width, "{text}");
    }
}

#[test]
fn decimal_rendering_matches_the_standard_formatter() {
    for value in [0usize, 1, 9, 10, 99, 100, 1_234, usize::MAX] {
        let mut out = String::new();
        push_usize(&mut out, value);
        assert_eq!(out, value.to_string());
    }
}

#[test]
fn digit_counting_matches_the_rendered_length() {
    for value in [0usize, 9, 10, 999, 1_000, 123_456] {
        assert_eq!(decimal_digits(value), value.to_string().len());
    }
}

#[test]
fn repeat_helpers_write_exact_counts() {
    let mut out = String::new();
    push_repeat(&mut out, '-', 4);
    push_repeat_str(&mut out, "ab", 2);
    push_spaces(&mut out, 2);
    assert_eq!(out, "----abab  ");

    out.clear();
    push_repeat(&mut out, '-', 0);
    assert!(out.is_empty());
}

#[test]
fn char_boundaries_floor_to_the_start_of_a_scalar() {
    let text = "漢字";
    assert_eq!(floor_char_boundary(text, 0), 0);
    assert_eq!(floor_char_boundary(text, 1), 0);
    assert_eq!(floor_char_boundary(text, 2), 0);
    assert_eq!(floor_char_boundary(text, 3), 3);
    assert_eq!(floor_char_boundary(text, 99), text.len());
}

#[test]
fn width_tables_are_sorted_and_disjoint() {
    for table in [ZERO_WIDTH, WIDE] {
        for pair in table.windows(2) {
            assert!(pair[0].0 <= pair[0].1, "range {:?} is inverted", pair[0]);
            assert!(
                pair[0].1 < pair[1].0,
                "ranges {:?} and {:?} overlap",
                pair[0],
                pair[1]
            );
        }
    }
}

#[test]
fn truncation_keeps_whole_escape_sequences() {
    let mut out = String::new();
    push_truncated(&mut out, "\x1b[31mred and long\x1b[0m", 3);
    // The colour opened, three columns survived, and the reset was written
    // because the rest of the line was dropped inside it.
    assert_eq!(out, "\x1b[31mred\x1b[0m");
}

#[test]
fn truncation_charges_no_width_to_styling() {
    let mut out = String::new();
    push_truncated(&mut out, "\x1b[1m\x1b[32mok\x1b[0m", 8);
    assert_eq!(out, "\x1b[1m\x1b[32mok\x1b[0m");
}

#[test]
fn truncation_measures_wide_characters_in_columns() {
    let mut out = String::new();
    push_truncated(&mut out, "日本語", 4);
    assert_eq!(out, "日本");

    out.clear();
    // Three columns cannot hold two double-width scalars, and half of one is
    // not a character a terminal can draw.
    push_truncated(&mut out, "日本語", 3);
    assert_eq!(out, "日");
}

#[test]
fn truncation_of_plain_text_appends_no_reset() {
    let mut out = String::new();
    push_truncated(&mut out, "abcdef", 3);
    assert_eq!(out, "abc");
}

#[test]
fn truncation_to_nothing_still_closes_what_it_opened() {
    let mut out = String::new();
    push_truncated(&mut out, "\x1b[31mred\x1b[0m", 0);
    assert_eq!(out, "\x1b[31m\x1b[0m");
}

/// A filename that tries to clear the screen is drawn as text.
///
/// `uf` is run against a clone, and nothing stops a file in that clone from
/// being named with an escape sequence in it. The path then reaches a terminal
/// inside a diagnostic — the same shape `uf_pm::progress` already refuses for a
/// package name out of a registry. See ubugeeei-prod/uf#640.
#[test]
fn a_path_cannot_move_the_cursor() {
    // Clear the screen, then home the cursor.
    assert_eq!(safe_path("src/\x1b[2J\x1b[Hevil.js"), "src/[2J[Hevil.js");
    // A carriage return redraws the row that was already written, which is how
    // a name overwrites the diagnostic above it.
    assert_eq!(
        safe_path("src/a.js\rerror: nothing is wrong"),
        "src/a.jserror: nothing is wrong"
    );
    // A newline ends the line the reporter is composing.
    assert_eq!(safe_path("src/a\n.js"), "src/a.js");
    // The one-byte C1 CSI, which is `is_control` and is not `\x1b`.
    assert_eq!(safe_path("src/\u{9b}31m.js"), "src/31m.js");
    // And the bell, which is not visible and is still not a filename.
    assert_eq!(safe_path("src/\u{7}a.js"), "src/a.js");
}

/// What a path is allowed to contain, which is nearly everything.
///
/// A sanitiser that dropped what it did not recognise would report the wrong
/// name for a real file, and the name is the whole point of the diagnostic.
#[test]
fn a_path_keeps_every_character_that_is_not_a_control() {
    // The Windows separator is a separator, not an escape.
    assert_eq!(
        safe_path(r"src\components\Button.js"),
        r"src\components\Button.js"
    );
    // A filename in another script is a filename.
    assert_eq!(safe_path("src/コンポーネント.js"), "src/コンポーネント.js");
    assert_eq!(safe_path("src/Ünïcödé-Ω.js"), "src/Ünïcödé-Ω.js");
    assert_eq!(safe_path("src/emoji-🎉.js"), "src/emoji-🎉.js");
    // Including the characters an escape sequence is spelled with, once the
    // escape that starts it is gone. `[2J` on its own is four printable
    // characters and a legal filename.
    assert_eq!(safe_path("src/[2J.js"), "src/[2J.js");
}

/// A path too wide to draw is elided from the left, so the file it names
/// survives.
#[test]
fn a_path_wider_than_the_cap_is_elided_from_its_head() {
    let deep = format!("{}Button.js", "nested/".repeat(40));
    let drawn = safe_path(&deep);
    assert_eq!(display_width(&drawn), MAX_PATH_WIDTH);
    assert!(drawn.starts_with('\u{2026}'), "{drawn}");
    // The tail is what identifies the file, so the tail is what is kept.
    assert!(drawn.ends_with("Button.js"), "{drawn}");

    // A path exactly at the cap is not elided.
    let exact = "a".repeat(MAX_PATH_WIDTH);
    assert_eq!(safe_path(&exact), exact);

    // The cap is columns rather than scalars, so a wide filename cannot
    // overflow the row by counting itself twice. It is a ceiling and not a
    // target: a double-width scalar cannot land on an odd column left over
    // after the ellipsis, so the drawn width here is one under the cap rather
    // than on it. Under is the safe direction — over is what wraps the row.
    let wide = "日".repeat(MAX_PATH_WIDTH);
    let drawn = safe_path(&wide);
    assert!(display_width(&drawn) <= MAX_PATH_WIDTH, "{drawn}");
    assert!(display_width(&drawn) >= MAX_PATH_WIDTH - 1, "{drawn}");

    // And the control characters are removed before the width is measured, so
    // a name padded with escapes is not elided for a width nobody can see.
    let padded = format!("{}src/a.js", "\x1b".repeat(500));
    assert_eq!(safe_path(&padded), "src/a.js");
}
