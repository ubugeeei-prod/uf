//! Tables: column sizing and per-column alignment.

use super::*;
use crate::capability::{Capabilities, ColorLevel, GlyphSet, Tty};

fn plain() -> Renderer {
    Renderer::new(Capabilities::new(
        ColorLevel::Never,
        GlyphSet::Unicode,
        Tty::Piped,
    ))
}

fn render(body: impl FnOnce(&Renderer, &mut String)) -> String {
    let renderer = plain();
    let mut out = String::new();
    body(&renderer, &mut out);
    out
}

#[test]
fn a_table_aligns_each_column_by_its_own_rule() {
    let out = render(|renderer, out| {
        let mut table = Table::new(vec![Column::left("file"), Column::right("errors")]);
        table.push(vec![Cell::new("src/app.js"), Cell::new("1")]);
        table.push(vec![Cell::new("src/routes/index.js"), Cell::new("12")]);
        renderer.table(out, 0, &table);
    });
    let lines: Vec<_> = out.lines().collect();

    assert!(lines[0].starts_with("file"));
    assert!(lines[0].ends_with("errors"));
    assert!(lines[1].starts_with("src/app.js"));
    assert!(lines[1].ends_with(" 1"));
    assert!(lines[2].starts_with("src/routes/index.js"));
    assert!(lines[2].ends_with(" 12"));
    assert_eq!(display_width(lines[0]), display_width(lines[1]));
    assert_eq!(display_width(lines[1]), display_width(lines[2]));
}

#[test]
fn a_table_never_leaves_trailing_whitespace() {
    let out = render(|renderer, out| {
        let mut table = Table::new(vec![Column::left("a"), Column::left("b")]);
        table.push(vec![Cell::new("wide-value"), Cell::new("x")]);
        table.push(vec![Cell::new("y"), Cell::new("z")]);
        renderer.table(out, 2, &table);
    });

    for line in out.lines() {
        assert!(!line.ends_with(' '), "{line:?}");
    }
}

#[test]
fn a_table_pads_short_rows() {
    let out = render(|renderer, out| {
        let mut table = Table::new(vec![Column::left("a"), Column::left("b")]);
        table.push(vec![Cell::new("only")]);
        renderer.table(out, 0, &table);
    });

    assert_eq!(out.lines().nth(1).unwrap(), "only");
}

#[test]
fn a_table_with_wide_cells_stays_aligned() {
    let out = render(|renderer, out| {
        let mut table = Table::new(vec![Column::left("file"), Column::right("n")]);
        table.push(vec![Cell::new("src/日本語.js"), Cell::new("1")]);
        table.push(vec![Cell::new("src/app.js"), Cell::new("22")]);
        renderer.table(out, 0, &table);
    });
    let widths: Vec<_> = out.lines().map(display_width).collect();

    assert_eq!(widths[1], widths[2]);
}

#[test]
fn a_table_centres_a_centred_column() {
    let out = render(|renderer, out| {
        let mut table = Table::new(vec![
            Column {
                header: "status",
                align: Align::Center,
            },
            Column::left("file"),
        ]);
        table.push(vec![Cell::new("ok"), Cell::new("src/app.js")]);
        renderer.table(out, 0, &table);
    });

    assert_eq!(out.lines().nth(1).unwrap(), "  ok    src/app.js");
}

#[test]
fn a_table_without_columns_renders_nothing() {
    let out = render(|renderer, out| renderer.table(out, 0, &Table::default()));
    assert!(out.is_empty());
}

#[test]
fn an_empty_table_still_renders_its_header() {
    let out = render(|renderer, out| {
        renderer.table(out, 0, &Table::new(vec![Column::left("file")]));
    });

    assert_eq!(out, "file\n");
}

#[test]
fn a_table_reports_its_own_shape() {
    let mut table = Table::new(vec![Column::left("a")]);
    assert!(table.is_empty());
    table.push(vec![Cell::toned("1", Tone::Number)]);

    assert!(!table.is_empty());
    assert_eq!(table.columns().len(), 1);
    assert_eq!(table.rows().len(), 1);
    assert_eq!(table.rows()[0][0].tone, Tone::Number);
}

#[test]
fn styling_a_table_does_not_change_its_geometry() {
    let mut styled = String::new();
    let colorful = Renderer::new(Capabilities::new(
        ColorLevel::TrueColor,
        GlyphSet::Unicode,
        Tty::Piped,
    ));
    let mut table = Table::new(vec![Column::left("file"), Column::right("n")]);
    table.push(vec![Cell::toned("src/app.js", Tone::Path), Cell::new("1")]);
    colorful.table(&mut styled, 0, &table);

    let mut plain_out = String::new();
    plain().table(&mut plain_out, 0, &table);

    let styled_widths: Vec<_> = styled.lines().map(display_width).collect();
    let plain_widths: Vec<_> = plain_out.lines().map(display_width).collect();
    assert_eq!(styled_widths, plain_widths);
}
