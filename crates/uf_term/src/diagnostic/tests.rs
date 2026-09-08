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

/// The path in a frame header cannot steer the terminal.
///
/// Every reporter that draws a code frame — `uf lint`, `uf check`, `uf build`,
/// and the dev server's diagnostics — puts the path on the header line through
/// here, so this is the one place the escape has to die. A repository is
/// attacker-authored input: `uf` is run against a clone, and a file in it can
/// be named anything the filesystem accepts. See ubugeeei-prod/uf#640.
#[test]
fn a_frame_header_cannot_be_made_to_clear_the_screen() {
    let hostile = CodeFrame::new(
        DiagnosticLevel::Error,
        "unclear type",
        "src/\x1b[2J\x1b[Hgotcha.js",
        3,
        1,
    );
    let rendered = render(&hostile);
    // The renderer is `ColorLevel::Never`, so the only way an escape reaches
    // the output at all is out of the path.
    assert!(!rendered.contains('\x1b'), "{rendered:?}");
    assert!(!rendered.contains('\r'), "{rendered:?}");
    // And the name is still readable, which is what the diagnostic is for.
    assert!(rendered.contains("src/[2J[Hgotcha.js:3:1"), "{rendered:?}");
}

/// The header stays on one line however the file was named.
#[test]
fn a_frame_header_is_one_line_whatever_the_path_contains() {
    let newline = CodeFrame::new(
        DiagnosticLevel::Error,
        "unclear type",
        "src/a\nerror: fabricated\nb.js",
        3,
        1,
    );
    let rendered = render(&newline);
    // Header, and nothing else: no source line was attached, so a second line
    // in the output could only have come out of the path.
    assert_eq!(rendered.lines().count(), 2, "{rendered:?}");
    assert!(
        rendered.contains("src/aerror: fabricatedb.js"),
        "{rendered:?}"
    );
}

/// The message beside a frame cannot steer the terminal either.
///
/// #649 prepared the path and stopped there, which its `docs/security.md` row
/// said. A message quotes module paths and import specifiers, and those came
/// out of the same clone the filename did. See ubugeeei-prod/uf#659.
#[test]
fn a_frame_message_cannot_be_made_to_clear_the_screen() {
    let hostile = CodeFrame::new(
        DiagnosticLevel::Error,
        "cannot resolve \x1b[2Jgotcha\r fabricated",
        "src/app.js",
        3,
        1,
    );
    let rendered = render(&hostile);
    assert!(!rendered.contains('\x1b'), "{rendered:?}");
    assert!(!rendered.contains('\r'), "{rendered:?}");
    assert!(
        rendered.contains("cannot resolve [2Jgotcha fabricated"),
        "{rendered:?}"
    );
}

/// A source line is checkout text, and the caret still lands where it did.
///
/// This is the half that could have gone wrong: the caret is placed by
/// counting display columns, so dropping characters from the line would
/// misalign it — unless the characters dropped are the ones already worth zero
/// columns, which every control character but the tab is. That is why the
/// assertion here is the caret offset and not only the absence of the escape.
#[test]
fn a_source_line_is_sanitised_without_moving_the_caret() {
    let clean = frame("const x = 1;", 7, 1);
    let hostile = frame("const \x1b[2Jx = 1;", 7, 1);

    let rendered = render(&hostile);
    assert!(!rendered.contains('\x1b'), "{rendered:?}");
    // The escape is gone and the code is still readable.
    assert!(rendered.contains("const [2Jx = 1;"), "{rendered:?}");
    // And the caret is where it would have been without it.
    assert_eq!(caret_offset(&rendered), caret_offset(&render(&clean)));
}

/// A carriage return in a source line would redraw the row above it.
#[test]
fn a_carriage_return_in_a_source_line_is_dropped() {
    let rendered = render(&frame("const x = 1;\rerror: fabricated", 7, 1));
    assert!(!rendered.contains('\r'), "{rendered:?}");
}

/// A tab is a control character and is the one that must survive, because the
/// caret is aligned to the columns it expands to.
#[test]
fn a_tab_in_a_source_line_still_expands() {
    let tabbed = render(&frame("\tconst x = 1;", 8, 1));
    // Four columns of indent from the tab, then the code.
    assert!(tabbed.contains("    const x = 1;"), "{tabbed:?}");
}

/// A legitimate non-ASCII source line renders as it is.
#[test]
fn a_source_line_in_another_script_is_not_touched() {
    let rendered = render(&frame("const 名前 = \"ユーザー\";", 7, 1));
    assert!(
        rendered.contains("const 名前 = \"ユーザー\";"),
        "{rendered:?}"
    );
}

/// A label names a type out of the checkout, so it gets the same treatment.
#[test]
fn a_caret_label_cannot_move_the_cursor() {
    let hostile = CodeFrame::new(DiagnosticLevel::Error, "unclear type", "src/app.js", 3, 1)
        .with_span(1)
        .with_source_line("const x = 1;")
        .with_label("expected \x1b[2JT");
    let rendered = render(&hostile);
    assert!(!rendered.contains('\x1b'), "{rendered:?}");
    assert!(rendered.contains("expected [2JT"), "{rendered:?}");
}
