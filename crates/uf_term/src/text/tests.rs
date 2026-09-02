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
