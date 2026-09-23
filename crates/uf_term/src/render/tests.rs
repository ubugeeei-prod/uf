//! The rendering primitives, in both glyph vocabularies.

use super::*;
use crate::capability::{ColorLevel, GlyphSet, Tty};
use crate::diagnostic::DiagnosticLevel;
use crate::table::{Cell, Column, Table};
use crate::tree::Tree;

fn renderer(color: ColorLevel, glyphs: GlyphSet) -> Renderer {
    Renderer::new(Capabilities::new(color, glyphs, Tty::Piped))
}

fn plain() -> Renderer {
    renderer(ColorLevel::Never, GlyphSet::Unicode)
}

fn render(body: impl FnOnce(&Renderer, &mut String)) -> String {
    let renderer = plain();
    let mut out = String::new();
    body(&renderer, &mut out);
    out
}

#[test]
fn a_banner_is_a_title_and_a_rule() {
    let out = render(|renderer, out| renderer.banner(out, "uf build", None));
    let lines: Vec<_> = out.lines().collect();

    assert_eq!(lines[0], "uf build");
    assert!(lines[1].chars().all(|ch| ch == '─'));
    assert_eq!(display_width(lines[1]), display_width("uf build"));
}

#[test]
fn a_very_short_banner_title_still_gets_a_rule() {
    let out = render(|renderer, out| renderer.banner(out, "a", None));
    assert_eq!(display_width(out.lines().nth(1).unwrap()), MIN_RULE);
}

#[test]
fn a_banner_subtitle_widens_the_rule() {
    let out = render(|renderer, out| renderer.banner(out, "uf build", Some("demo-app")));
    let lines: Vec<_> = out.lines().collect();

    assert_eq!(lines[0], "uf build · demo-app");
    assert_eq!(display_width(lines[1]), display_width(lines[0]));
}

/// The separator is a middle dot, which is not ASCII: a terminal that cannot
/// take box drawing cannot take this either.
#[test]
fn an_ascii_banner_separates_with_spacing_instead_of_a_dot() {
    let renderer = Renderer::new(Capabilities::new(
        ColorLevel::Never,
        GlyphSet::Ascii,
        Tty::Piped,
    ));
    let mut out = String::new();
    renderer.banner(&mut out, "uf build", Some("demo-app"));
    let lines: Vec<_> = out.lines().collect();

    assert_eq!(lines[0], "uf build  demo-app");
    assert!(lines[0].is_ascii());
    assert_eq!(display_width(lines[1]), display_width(lines[0]));
}

#[test]
fn a_banner_rule_is_bounded() {
    let long = "x".repeat(400);
    let out = render(|renderer, out| renderer.banner(out, &long, None));
    let rule = out.lines().nth(1).unwrap();

    assert_eq!(display_width(rule), MAX_RULE);
}

#[test]
fn a_wide_banner_title_keeps_the_rule_the_same_width() {
    let out = render(|renderer, out| renderer.banner(out, "uf ビルドとチェック", None));
    let lines: Vec<_> = out.lines().collect();

    assert_eq!(display_width(lines[0]), 19);
    assert_eq!(display_width(lines[1]), display_width(lines[0]));
}

#[test]
fn key_values_align_their_value_column() {
    let out = render(|renderer, out| {
        renderer.key_values(
            out,
            2,
            &[
                KeyValue::new("entries", "app.js"),
                KeyValue::new("out dir", "dist"),
                KeyValue::new("r", "1"),
            ],
        );
    });
    let columns: Vec<_> = out
        .lines()
        .map(|line| display_width(&line[..line.rfind("  ").unwrap() + 2]))
        .collect();

    assert_eq!(columns[0], columns[1]);
    assert_eq!(columns[1], columns[2]);
}

#[test]
fn key_values_align_around_wide_keys() {
    let out = render(|renderer, out| {
        renderer.key_values(
            out,
            0,
            &[KeyValue::new("パス", "a"), KeyValue::new("out", "b")],
        );
    });
    let offsets: Vec<_> = out.lines().map(|line| display_width(line) - 1).collect();

    assert_eq!(offsets[0], offsets[1]);
}

#[test]
fn an_empty_key_value_block_renders_nothing() {
    let out = render(|renderer, out| renderer.key_values(out, 2, &[]));
    assert!(out.is_empty());
}

#[test]
fn timings_right_align_their_durations() {
    let out = render(|renderer, out| {
        renderer.timings(
            out,
            2,
            &[
                Phase {
                    label: "config",
                    duration: Duration::from_micros(1_200),
                },
                Phase {
                    label: "rsc analysis",
                    duration: Duration::from_millis(31),
                },
            ],
            Some(Duration::from_millis(33)),
        );
    });
    let widths: Vec<_> = out.lines().map(display_width).collect();

    assert!(out.contains("1.2ms"));
    assert!(out.contains("total"));
    assert_eq!(widths[0], widths[1]);
    assert_eq!(widths[1], widths[2]);
}

#[test]
fn timings_render_nothing_when_there_is_nothing_to_report() {
    let out = render(|renderer, out| renderer.timings(out, 2, &[], None));
    assert!(out.is_empty());
}

#[test]
fn status_lines_start_with_a_one_cell_mark() {
    let out = render(|renderer, out| {
        renderer.status(out, Status::Success, "build succeeded");
        renderer.status(out, Status::Error, "build failed");
    });

    assert_eq!(out, "✓ build succeeded\n✗ build failed\n");
}

#[test]
fn ordered_and_bulleted_lists_render_their_markers() {
    let out = render(|renderer, out| {
        renderer.ordered_list(out, 2, &["cd demo", "uf dev"]);
        renderer.bullet_list(out, 2, &["one"]);
    });

    assert_eq!(out, "  1. cd demo\n  2. uf dev\n  - one\n");
}

#[test]
fn nothing_renders_an_escape_when_color_is_off() {
    let out = render(|renderer, out| {
        renderer.banner(out, "uf build", Some("demo"));
        renderer.heading(out, 0, "output");
        renderer.rule(out, 8);
        renderer.status(out, Status::Warn, "careful");
        renderer.key_values(out, 2, &[KeyValue::toned("k", "v", Tone::Accent)]);
        renderer.timings(
            out,
            2,
            &[Phase {
                label: "config",
                duration: Duration::from_millis(1),
            }],
            Some(Duration::from_millis(2)),
        );
        renderer.tree(out, 0, &Tree::from_paths("demo", ["a/b.js"]));
        renderer.ordered_list(out, 2, &["step"]);
        renderer.bullet_list(out, 2, &["item"]);
        let mut table = Table::new(vec![Column::left("a")]);
        table.push(vec![Cell::toned("1", Tone::Number)]);
        renderer.table(out, 0, &table);
        renderer.code_frame(
            out,
            &CodeFrame::new(DiagnosticLevel::Error, "boom", "a.js", 1, 1),
        );
    });

    assert!(!out.contains('\x1b'), "{out:?}");
}

#[test]
fn everything_renders_escapes_when_color_is_on() {
    let renderer = renderer(ColorLevel::TrueColor, GlyphSet::Unicode);
    let mut out = String::new();
    renderer.banner(&mut out, "uf build", Some("demo"));
    renderer.status(&mut out, Status::Success, "ok");
    renderer.key_values(&mut out, 2, &[KeyValue::toned("k", "v", Tone::Accent)]);

    assert!(out.contains("\x1b["));
    assert!(out.contains("\x1b[0m"));
}

#[test]
fn a_shared_buffer_is_reused_across_renders() {
    let renderer = plain();
    let mut out = String::with_capacity(4_096);
    let capacity = out.capacity();
    for _ in 0..64 {
        out.clear();
        renderer.status(&mut out, Status::Info, "still going");
    }

    assert_eq!(out.capacity(), capacity, "the buffer must not regrow");
}

#[test]
fn a_summary_is_a_mark_a_verdict_and_its_facts() {
    let out = render(|renderer, out| {
        renderer.summary(
            out,
            Status::Error,
            "3 errors, 1 warning",
            &["10 files checked", "", "41ms"],
        );
    });

    // The empty fact is skipped rather than drawn as a doubled separator.
    assert_eq!(out, "✗ 3 errors, 1 warning · 10 files checked · 41ms\n");
}

#[test]
fn an_ascii_summary_separates_its_facts_with_commas() {
    let renderer = renderer(ColorLevel::Never, GlyphSet::Ascii);
    let mut out = String::new();
    renderer.summary(&mut out, Status::Success, "no problems", &["8 files"]);

    assert_eq!(out, "+ no problems, 8 files\n");
    assert!(out.is_ascii());
}

#[test]
fn a_summary_with_no_facts_is_a_status_line() {
    let out = render(|renderer, out| renderer.summary(out, Status::Success, "done", &[]));
    assert_eq!(out, "✓ done\n");
}

#[test]
fn a_plain_hint_keeps_its_backticks() {
    let out = render(|renderer, out| renderer.hint(out, 2, "run `uf lint --fix` to apply it"));
    assert_eq!(out, "  › run `uf lint --fix` to apply it\n");
}

/// With colour, the code span is set apart by the accent, so the backticks
/// that did the same job in plain text go.
#[test]
fn a_coloured_hint_draws_code_in_the_accent_instead_of_backticks() {
    let renderer = renderer(ColorLevel::TrueColor, GlyphSet::Unicode);
    let mut out = String::new();
    renderer.hint(&mut out, 0, "run `uf fmt` now");

    assert!(!out.contains('`'), "{out:?}");
    let mut accent = String::new();
    renderer
        .theme()
        .accent
        .paint(ColorLevel::TrueColor, "uf fmt", &mut accent);
    assert!(out.contains(&accent), "{out:?}");
}

/// An unmatched backtick is punctuation, not the start of a code span.
#[test]
fn a_hint_with_an_unmatched_backtick_is_left_as_written() {
    let renderer = renderer(ColorLevel::TrueColor, GlyphSet::Unicode);
    let mut out = String::new();
    renderer.hint(&mut out, 0, "a ` alone");

    assert!(out.contains("a ` alone"), "{out:?}");
}

#[test]
fn the_banner_draws_the_command_in_its_own_style() {
    let renderer = renderer(ColorLevel::TrueColor, GlyphSet::Unicode);
    let mut out = String::new();
    renderer.banner(&mut out, "uf lint", None);

    let mut expected = String::new();
    renderer
        .theme()
        .banner
        .paint(ColorLevel::TrueColor, "uf lint", &mut expected);
    assert!(out.starts_with(&expected), "{out:?}");
}

#[test]
fn status_rows_align_their_columns_and_carry_hints() {
    let out = render(|renderer, out| {
        renderer.status_rows(
            out,
            2,
            &[
                StatusRow::new(Status::Success, "node", "24.3.0").with_detail("runtime"),
                StatusRow::new(Status::Error, "pnpm@10", "missing")
                    .with_detail("package manager")
                    .with_hint("run `uf env install`"),
                StatusRow::new(Status::Warn, "bun", ""),
            ],
        );
    });

    assert_eq!(
        out,
        "  ✓ node     24.3.0   runtime\n\
         \x20 ✗ pnpm@10  missing  package manager\n\
         \x20   › run `uf env install`\n\
         \x20 ! bun\n"
    );
}

#[test]
fn status_rows_render_no_escape_without_color() {
    let out = render(|renderer, out| {
        renderer.status_rows(
            out,
            0,
            &[StatusRow::new(Status::Error, "a", "b")
                .with_detail("c")
                .with_hint("`d`")],
        );
    });
    assert!(!out.contains('\x1b'), "{out:?}");
}

#[test]
fn a_hint_s_later_lines_stay_under_its_first() {
    let out = render(|renderer, out| renderer.hint(out, 2, "first\nsecond"));
    assert_eq!(out, "  › first\n    second\n");
}
