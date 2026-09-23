//! Wrapping, code spans, and the two-column list, at several widths.

use super::*;
use crate::capability::{Capabilities, ColorLevel, GlyphSet, Tty};

fn renderer(color: ColorLevel) -> Renderer {
    Renderer::new(Capabilities::new(color, GlyphSet::Unicode, Tty::Piped))
}

fn wrap(text: &str, width: usize) -> String {
    let renderer = renderer(ColorLevel::Never);
    let mut out = String::new();
    renderer.prose(&mut out, text, Style::new(), 0, 0, width);
    out
}

/// Strip every escape sequence, to compare what a reader sees.
fn visible(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}

#[test]
fn prose_breaks_between_words_and_never_inside_one() {
    let out = wrap("the quick brown fox jumps over the lazy dog", 15);
    assert_eq!(out, "the quick brown\nfox jumps over\nthe lazy dog");
    for line in out.lines() {
        assert!(display_width(line) <= 15, "{line:?}");
    }
}

#[test]
fn a_word_wider_than_the_line_gets_a_line_to_itself() {
    let out = wrap("see https://docs.uniflowed.dev/reference/cli for more", 20);
    assert_eq!(
        out,
        "see\nhttps://docs.uniflowed.dev/reference/cli\nfor more"
    );
}

#[test]
fn wrapped_lines_hang_at_the_margin() {
    let renderer = renderer(ColorLevel::Never);
    let mut out = String::from("  term  ");
    let column = renderer.prose(
        &mut out,
        "one two three four five six",
        Style::new(),
        8,
        8,
        20,
    );
    assert_eq!(out, "  term  one two\n        three four\n        five six");
    assert_eq!(column, 16);
}

#[test]
fn paragraphs_are_separated_by_a_blank_line_at_the_margin() {
    let renderer = renderer(ColorLevel::Never);
    let mut out = String::new();
    renderer.prose(
        &mut out,
        "first one.\n\nsecond one.",
        Style::new(),
        2,
        2,
        40,
    );
    assert_eq!(out, "first one.\n\n  second one.");
}

#[test]
fn a_list_line_keeps_its_own_line_and_wraps_under_its_text() {
    let out = wrap("items:\n- alpha beta gamma delta\n- two", 16);
    assert_eq!(out, "items:\n- alpha beta\n  gamma delta\n- two");
}

#[test]
fn wide_characters_are_measured_by_their_cells() {
    // Each of these is two columns: four of them fill eight.
    let out = wrap("ビルド ビルド ビルド", 14);
    assert_eq!(out, "ビルド ビルド\nビルド");
}

/// Without colour the backticks are the only thing marking a command, so they
/// stay and count towards the width.
#[test]
fn without_colour_code_spans_keep_their_backticks() {
    let out = wrap("run `uf lint --fix` first", 80);
    assert_eq!(out, "run `uf lint --fix` first");
}

/// With colour the backticks are dropped and the span is drawn in the accent,
/// even when the span is broken across two lines.
#[test]
fn with_colour_a_code_span_is_accented_across_a_line_break() {
    let renderer = renderer(ColorLevel::Ansi16);
    let mut out = String::new();
    renderer.prose(
        &mut out,
        "then run `uf lint --fix` and commit",
        Style::new(),
        0,
        0,
        16,
    );
    assert_eq!(visible(&out), "then run uf lint\n--fix and commit");
    let accent = {
        let mut open = String::new();
        renderer.theme().accent.open(ColorLevel::Ansi16, &mut open);
        open
    };
    // `uf`, `lint` and `--fix` each carry the accent; `then` does not.
    assert_eq!(out.matches(accent.as_str()).count(), 3, "{out:?}");
    assert!(!out.contains('`'));
}

#[test]
fn an_unmatched_backtick_is_text() {
    let renderer = renderer(ColorLevel::Ansi16);
    let mut out = String::new();
    renderer.prose(&mut out, "it's a `stray tick", Style::new(), 0, 0, 80);
    assert_eq!(visible(&out), "it's a `stray tick");
}

fn list(width: usize, color: ColorLevel) -> String {
    let renderer = renderer(color);
    let mut out = String::new();
    renderer.definitions(
        &mut out,
        2,
        width,
        &[
            Definition::new("--cwd <DIR>", "Run as if uf had been started in DIR"),
            Definition::new("--color <WHEN>", "When to colourise output")
                .with_meta("[default: auto]"),
            Definition::new("-h, --help", ""),
        ],
        false,
    );
    out
}

#[test]
fn a_definition_list_aligns_its_descriptions() {
    assert_eq!(
        list(80, ColorLevel::Never),
        "  --cwd <DIR>     Run as if uf had been started in DIR\n\
         \x20 --color <WHEN>  When to colourise output [default: auto]\n\
         \x20 -h, --help\n"
    );
}

#[test]
fn a_definition_list_wraps_under_its_description_column() {
    assert_eq!(
        list(48, ColorLevel::Never),
        "  --cwd <DIR>     Run as if uf had been started\n\
         \x20                 in DIR\n\
         \x20 --color <WHEN>  When to colourise output\n\
         \x20                 [default: auto]\n\
         \x20 -h, --help\n"
    );
}

#[test]
fn a_narrow_terminal_stacks_the_description_under_its_term() {
    assert_eq!(
        list(32, ColorLevel::Never),
        "  --cwd <DIR>\n\
         \x20     Run as if uf had been\n\
         \x20     started in DIR\n\
         \x20 --color <WHEN>\n\
         \x20     When to colourise output\n\
         \x20     [default: auto]\n\
         \x20 -h, --help\n"
    );
}

#[test]
fn no_line_of_a_list_is_wider_than_asked() {
    for width in [32, 40, 48, 60, 80, 100] {
        for line in list(width, ColorLevel::TrueColor).lines() {
            assert!(
                display_width(line) <= width,
                "{width}: {:?} is {} wide",
                visible(line),
                display_width(line)
            );
        }
    }
}

#[test]
fn colour_does_not_move_anything() {
    for width in [32, 48, 80] {
        assert_eq!(
            visible(&list(width, ColorLevel::TrueColor)),
            list(width, ColorLevel::Never),
            "{width}"
        );
    }
}

#[test]
fn a_term_wider_than_its_column_puts_the_description_under_it() {
    let renderer = renderer(ColorLevel::Never);
    let mut out = String::new();
    renderer.definitions(
        &mut out,
        0,
        40,
        &[
            Definition::new("a", "short"),
            Definition::new("a-very-long-term-indeed", "wide"),
        ],
        false,
    );
    assert_eq!(
        out,
        "a              short\na-very-long-term-indeed\n               wide\n"
    );
}

#[test]
fn spaced_rows_are_separated_by_a_blank_line() {
    let renderer = renderer(ColorLevel::Never);
    let mut out = String::new();
    renderer.definitions(
        &mut out,
        0,
        40,
        &[Definition::new("a", "one"), Definition::new("b", "two")],
        true,
    );
    assert_eq!(out, "a  one\n\nb  two\n");
}

#[test]
fn a_note_moves_to_the_next_line_a_group_at_a_time() {
    let renderer = renderer(ColorLevel::Never);
    let mut out = String::new();
    renderer.definitions(
        &mut out,
        0,
        40,
        &[Definition::new("--color", "When to colourise")
            .with_meta("[values: auto, always, never] [default: auto]")],
        false,
    );
    assert_eq!(
        out,
        "--color  When to colourise\n\
         \x20        [values: auto, always, never]\n\
         \x20        [default: auto]\n"
    );
}

#[test]
fn a_note_too_wide_for_any_line_is_broken_between_its_words() {
    let renderer = renderer(ColorLevel::Never);
    let mut out = String::new();
    renderer.definitions(
        &mut out,
        0,
        24,
        &[Definition::new("-x", "").with_meta("[values: alpha, beta, gamma, delta]")],
        false,
    );
    for line in out.lines() {
        assert!(display_width(line) <= 24, "{line:?}");
    }
    assert!(out.contains("[values: alpha,"), "{out}");
}

#[test]
fn a_wrapped_hint_hangs_under_its_text() {
    let renderer = renderer(ColorLevel::Never);
    let mut out = String::new();
    renderer.hint_within(
        &mut out,
        2,
        30,
        "`uf <command> --help` for a command's options",
    );
    assert_eq!(
        out,
        "  › `uf <command> --help` for\n    a command's options\n"
    );
}
