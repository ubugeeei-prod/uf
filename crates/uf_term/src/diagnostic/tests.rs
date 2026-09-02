//! Code frames: geometry, tabs, wide characters, and clamping.

use super::*;
use crate::capability::{Capabilities, ColorLevel, GlyphSet, Tty};
use crate::render::Renderer;
use crate::text::display_width;

fn plain_renderer() -> Renderer {
    Renderer::new(Capabilities::new(
        ColorLevel::Never,
        GlyphSet::Unicode,
        Tty::Piped,
    ))
}

fn render(frame: &CodeFrame<'_>) -> String {
    let mut out = String::new();
    render_frame(&plain_renderer(), &mut out, frame, 0);
    out
}

fn frame<'a>(source: &'a str, column: usize, span: usize) -> CodeFrame<'a> {
    CodeFrame::new(
        DiagnosticLevel::Error,
        "unclear type",
        "src/app.js",
        3,
        column,
    )
    .with_rule("flow/unclear-type")
    .with_span(span)
    .with_source_line(source)
}

/// The column the caret row's first `^` lands on, zero-based.
fn caret_offset(rendered: &str) -> usize {
    let caret_line = rendered
        .lines()
        .find(|line| line.contains('^'))
        .expect("a caret row");
    let prefix = &caret_line[..caret_line.find('^').unwrap()];
    display_width(prefix)
}

/// The column the source text starts on in the source row, zero-based.
fn source_offset(rendered: &str, needle: &str) -> usize {
    let source_line = rendered
        .lines()
        .find(|line| line.contains(needle))
        .expect("a source row");
    display_width(&source_line[..source_line.find(needle).unwrap()])
}

#[test]
fn a_frame_has_a_header_a_location_and_a_caret() {
    let rendered = render(&frame("const value: any = load();", 14, 3));
    let lines: Vec<_> = rendered.lines().collect();

    assert_eq!(lines[0], "error[flow/unclear-type]: unclear type");
    assert_eq!(lines[1], " --> src/app.js:3:14");
    assert_eq!(lines[2], "  │");
    assert_eq!(lines[3], "3 │ const value: any = load();");
    assert_eq!(lines[4], "  │              ^^^");
}

#[test]
fn the_caret_lines_up_under_the_offending_bytes() {
    let rendered = render(&frame("const value: any = load();", 14, 3));
    assert_eq!(caret_offset(&rendered), source_offset(&rendered, "any"));
}

#[test]
fn a_frame_without_a_source_line_still_reports_the_location() {
    let frame = CodeFrame::new(DiagnosticLevel::Warning, "no route", "app/page.js", 1, 1);
    let rendered = render(&frame);

    assert_eq!(rendered, "warning: no route\n --> app/page.js:1:1\n");
}

#[test]
fn tabs_are_expanded_and_the_caret_follows_them() {
    let rendered = render(&frame("\t\tconst x: any = 1;", 12, 3));
    assert!(!rendered.contains('\t'), "tabs must not reach the terminal");
    assert_eq!(caret_offset(&rendered), source_offset(&rendered, "any"));
}

#[test]
fn a_tab_in_the_middle_of_a_line_still_aligns() {
    let rendered = render(&frame("if (x)\t{ any }", 10, 3));
    assert_eq!(caret_offset(&rendered), source_offset(&rendered, "any"));
}

#[test]
fn wide_characters_before_the_span_shift_the_caret() {
    let source = "const 日本語 = any;";
    let column = source.find("any").unwrap() + 1;
    let rendered = render(&frame(source, column, 3));

    assert_eq!(caret_offset(&rendered), source_offset(&rendered, "any"));
}

#[test]
fn combining_marks_before_the_span_do_not_shift_the_caret() {
    let source = "const e\u{0301}tat = any;";
    let column = source.find("any").unwrap() + 1;
    let rendered = render(&frame(source, column, 3));

    assert_eq!(caret_offset(&rendered), source_offset(&rendered, "any"));
}

#[test]
fn a_wide_span_gets_a_wide_caret() {
    let source = "const x = 日本;";
    let column = source.find('日').unwrap() + 1;
    let rendered = render(&frame(source, column, "日本".len()));
    let carets = rendered
        .lines()
        .find(|line| line.contains('^'))
        .unwrap()
        .matches('^')
        .count();

    assert_eq!(carets, 4, "two wide characters occupy four cells");
}

#[test]
fn a_span_running_past_the_end_of_the_line_is_clamped() {
    let rendered = render(&frame("const x = 1;", 11, 9_999));
    let carets = rendered
        .lines()
        .find(|line| line.contains('^'))
        .unwrap()
        .matches('^')
        .count();

    assert_eq!(carets, 2, "only the remaining columns are marked");
}

#[test]
fn a_column_past_the_end_of_the_line_does_not_panic() {
    let rendered = render(&frame("short", 500, 3));
    assert!(rendered.contains('^'));
    assert_eq!(
        caret_offset(&rendered),
        source_offset(&rendered, "short") + 5
    );
}

#[test]
fn a_zero_column_is_treated_as_the_first_column() {
    let rendered = render(&frame("const x = 1;", 0, 5));
    assert_eq!(caret_offset(&rendered), source_offset(&rendered, "const"));
}

#[test]
fn a_column_inside_a_multibyte_character_does_not_panic() {
    let source = "const 日本 = 1;";
    // Byte column 8 lands inside the first ideograph.
    let rendered = render(&frame(source, 8, 1));
    assert!(rendered.contains('^'));
}

#[test]
fn an_empty_source_line_still_renders_one_caret() {
    let rendered = render(&frame("", 1, 3));
    assert_eq!(rendered.matches('^').count(), 1);
}

#[test]
fn a_very_long_line_is_windowed_around_the_span() {
    let mut source = "x".repeat(400);
    source.push_str("any");
    source.push_str(&"y".repeat(400));
    let column = 401;
    let rendered = render(&frame(&source, column, 3));
    let source_row = rendered
        .lines()
        .find(|line| line.contains("any"))
        .expect("a source row");

    assert!(display_width(source_row) < 160, "{source_row}");
    assert!(source_row.contains('…'));
    assert_eq!(caret_offset(&rendered), source_offset(&rendered, "any"));
}

#[test]
fn a_short_line_is_not_windowed() {
    let rendered = render(&frame("const value: any = load();", 14, 3));
    assert!(!rendered.contains('…'));
}

#[test]
fn ascii_capabilities_swap_the_box_drawing_characters() {
    let mut out = String::new();
    let renderer = Renderer::new(Capabilities::new(
        ColorLevel::Never,
        GlyphSet::Ascii,
        Tty::Piped,
    ));
    render_frame(
        &renderer,
        &mut out,
        &frame("const value: any = load();", 14, 3),
        0,
    );

    assert!(out.is_ascii());
    assert!(out.contains("3 | const value: any = load();"));
}

#[test]
fn a_frame_is_escape_free_without_color() {
    let rendered = render(&frame("const value: any = load();", 14, 3));
    assert!(!rendered.contains('\x1b'));
}

#[test]
fn a_frame_is_styled_with_color() {
    let mut out = String::new();
    let renderer = Renderer::new(Capabilities::new(
        ColorLevel::Ansi256,
        GlyphSet::Unicode,
        Tty::Interactive,
    ));
    render_frame(
        &renderer,
        &mut out,
        &frame("const value: any = load();", 14, 3),
        0,
    );

    assert!(out.contains('\x1b'));
    // Styling must not change the geometry.
    assert_eq!(caret_offset(&out), source_offset(&out, "any"));
}

#[test]
fn indentation_shifts_every_row_equally() {
    let mut out = String::new();
    render_frame(
        &plain_renderer(),
        &mut out,
        &frame("const value: any = load();", 14, 3),
        2,
    );

    for line in out.lines() {
        assert!(line.starts_with("  "), "{line:?}");
    }
    assert_eq!(caret_offset(&out), source_offset(&out, "any"));
}

#[test]
fn severity_labels_are_spelled_out() {
    assert_eq!(DiagnosticLevel::Error.label(), "error");
    assert_eq!(DiagnosticLevel::Warning.label(), "warning");
    assert_eq!(DiagnosticLevel::Note.label(), "note");
    assert_eq!(DiagnosticLevel::Help.label(), "help");
}

#[test]
fn a_label_is_printed_after_the_carets() {
    let frame = frame("const value: any = load();", 14, 3).with_label("use a real type");
    let rendered = render(&frame);
    assert!(rendered.contains("^^^ use a real type"));
}

#[test]
fn a_four_digit_line_number_widens_the_gutter_consistently() {
    let mut frame = frame("const value: any = load();", 14, 3);
    frame.line = 1234;
    let rendered = render(&frame);

    assert!(rendered.contains("     │"));
    assert!(rendered.contains("1234 │ const value"));
    assert_eq!(caret_offset(&rendered), source_offset(&rendered, "any"));
}
