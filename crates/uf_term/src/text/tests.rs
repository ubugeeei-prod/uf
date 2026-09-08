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

/// A diagnostic message gets the treatment a path does.
///
/// A message quotes module paths and import specifiers, which came out of the
/// same clone the filename did. See ubugeeei-prod/uf#659.
#[test]
fn a_message_cannot_move_the_cursor() {
    assert_eq!(
        safe_message("cannot resolve \x1b[2Jgotcha"),
        "cannot resolve [2Jgotcha"
    );
    assert_eq!(safe_message("a\rb"), "ab");
    assert_eq!(safe_message("a\nb"), "ab");
    // A message is a sentence, so the prose survives whole.
    assert_eq!(
        safe_message("Cannot get `x.y` because property `y` is missing."),
        "Cannot get `x.y` because property `y` is missing."
    );
    assert_eq!(safe_message("型が合いません"), "型が合いません");
}

/// A message longer than the cap is cut from the right, because a sentence is
/// read from its start — the opposite end from a path, which is identified by
/// its tail.
#[test]
fn a_message_wider_than_the_cap_is_cut_from_its_tail() {
    let long = "x".repeat(MAX_MESSAGE_WIDTH * 2);
    let drawn = safe_message(&long);
    assert_eq!(display_width(&drawn), MAX_MESSAGE_WIDTH);
    assert!(drawn.ends_with('\u{2026}'), "{drawn}");
    assert!(
        drawn.starts_with("xxx"),
        "the start of the sentence is what survives"
    );

    // Exactly at the cap is not cut.
    let exact = "y".repeat(MAX_MESSAGE_WIDTH);
    assert_eq!(safe_message(&exact), exact);

    // And the control characters go before the width is measured, so a message
    // padded with escapes is not cut for a width nobody can see.
    let padded = format!("{}short", "\x1b".repeat(2_000));
    assert_eq!(safe_message(&padded), "short");
}

/// The cap is the *rendered* width, so an emoji-presentation sequence counts.
///
/// `char_width` gives a variation selector zero and the narrow scalar before it
/// one, which sums to half of what a terminal draws: `a\u{fe0f}` repeated 120
/// times summed to 120 — exactly the cap — and rendered as 240 columns. A name
/// built out of those could wrap the row the bound exists to hold whatever the
/// bound said. See ubugeeei-prod/uf#671.
#[test]
fn an_emoji_presentation_sequence_counts_the_columns_it_draws() {
    let path = "a\u{fe0f}".repeat(120);
    let drawn = safe_path(&path);

    assert!(
        display_width(&drawn) <= MAX_PATH_WIDTH,
        "{}",
        display_width(&drawn)
    );
    assert!(drawn.starts_with('\u{2026}'), "it was elided: {drawn:?}");
    // The same sequence inside the cap is left whole.
    let short = "a\u{fe0f}".repeat(10);
    assert_eq!(safe_path(&short), short);
}

/// A joined cluster is one cluster, and is not charged per scalar.
///
/// The other half of the same mistake, in the other direction: summing
/// `char_width` charges every scalar of a zero-width-joiner sequence, so a
/// path of them would be elided long before it filled the row.
#[test]
fn a_zero_width_joiner_sequence_is_charged_once() {
    // A single rendered glyph made of three scalars joined by two joiners.
    let family = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}";
    let path = format!("src/{}.js", family.repeat(20));
    let drawn = safe_path(&path);

    // Well inside the cap once the joins are counted, so nothing is elided.
    assert_eq!(display_width(&drawn), display_width(&path));
    assert!(!drawn.starts_with('\u{2026}'), "{drawn:?}");
}

/// And a message is measured the same way.
#[test]
fn a_message_counts_the_columns_it_draws_too() {
    let message = "a\u{fe0f}".repeat(MAX_MESSAGE_WIDTH);
    let drawn = safe_message(&message);

    assert!(
        display_width(&drawn) <= MAX_MESSAGE_WIDTH,
        "{}",
        display_width(&drawn)
    );
    assert!(drawn.ends_with('\u{2026}'), "{drawn:?}");
}

/// The per-scalar costs are [`display_width`], split up — checked exhaustively.
///
/// The caps read the costs forward and stop at whatever boundary the budget
/// runs out on, so any place the two rules disagree is a place a cap is wrong
/// about the row. Rather than trust that the second copy of the state machine
/// matches the first by eye, every sequence of up to four scalars drawn from
/// the interesting alphabet — narrow, wide, a variation selector, a joiner —
/// is checked to sum to exactly what the string renders as.
#[test]
fn the_costs_sum_to_what_the_string_renders_as() {
    const ALPHABET: [char; 5] = ['a', '漢', VS16, ZWJ, '\u{1f469}'];

    let mut sequences: Vec<Vec<char>> = vec![Vec::new()];
    for _ in 0..4 {
        let mut longer = Vec::new();
        for sequence in &sequences {
            for &ch in &ALPHABET {
                let mut next = sequence.clone();
                next.push(ch);
                longer.push(next);
            }
        }
        sequences.extend(longer);
    }

    for sequence in &sequences {
        let rendered: String = sequence.iter().collect();
        assert_eq!(
            columns(sequence)
                .iter()
                .map(|column| column.cost)
                .sum::<usize>(),
            display_width(&rendered),
            "{rendered:?}"
        );
    }
}

/// And what the cap keeps is under the cap, whatever it had to cut through.
///
/// Dropping from the front can stop in the middle of a cluster, where the
/// costs were counted against scalars that are no longer there. The one that
/// would be unsafe — keeping a joined scalar whose joiner was dropped, which
/// the model charged nothing and the terminal charges in full — cannot happen,
/// because dropping a zero-cost scalar never brings the total under the budget
/// and so never ends the loop. This holds the property rather than the reason.
#[test]
fn every_cut_leaves_a_path_inside_the_cap() {
    const ALPHABET: [char; 5] = ['a', '漢', VS16, ZWJ, '\u{1f469}'];

    // Long enough that the cap always bites — every scalar draws at most two
    // columns, so twice the cap in scalars is at least the cap in columns — and
    // seeded differently each round so the boundary lands on each kind of
    // scalar, and on each kind of run leading up to one, in turn.
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    for _ in 0..500 {
        let mut path = String::with_capacity(MAX_MESSAGE_WIDTH * 8);
        for _ in 0..MAX_MESSAGE_WIDTH * 2 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            path.push(ALPHABET[(seed % ALPHABET.len() as u64) as usize]);
        }
        let drawn = safe_path(&path);
        assert!(
            display_width(&drawn) <= MAX_PATH_WIDTH,
            "{} columns: {drawn:?}",
            display_width(&drawn)
        );
        assert!(
            display_width(&safe_message(&path)) <= MAX_MESSAGE_WIDTH,
            "message: {drawn:?}"
        );
    }
}
