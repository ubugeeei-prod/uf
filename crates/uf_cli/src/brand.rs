//! Brand-aware terminal snippets shared by human-facing CLI commands.

use uf_term::{Cell, Color, Column, Renderer, Style, Table, Tone, push_repeat};

pub(crate) const HEADLINE: &str = "Unified Toolchain for Flow";
pub(crate) const TAGLINE: &str = "All-in-one toolchain for Flow and React.";
pub(crate) const DOCS_URL: &str = "https://docs.uniflowed.dev";
pub(crate) const CURL_INSTALL: &str = "curl -fsSL https://setup.uniflowed.dev | sh";
pub(crate) const NIX_RUN: &str = "nix run github:ubugeeei-prod/uf#uf -- --version";
pub(crate) const NIX_PROFILE: &str = "nix profile install github:ubugeeei-prod/uf#uf";
pub(crate) const BRAND_TOKENS: &str = "brand/tokens.json";

const CYAN: Color = Color::Rgb(0x35, 0xd6, 0xf6);
const BLUE: Color = Color::Rgb(0x26, 0x77, 0xff);
const INDIGO: Color = Color::Rgb(0x5c, 0x49, 0xff);
const VIOLET: Color = Color::Rgb(0x8f, 0x4b, 0xff);
const MAGENTA: Color = Color::Rgb(0xd8, 0x4b, 0xff);

pub(crate) const PALETTE: &[(&str, &str)] = &[
    ("--uf-color-cyan-500", "#35D6F6"),
    ("--uf-color-blue-500", "#2677FF"),
    ("--uf-color-indigo-500", "#5C49FF"),
    ("--uf-color-violet-500", "#8F4BFF"),
    ("--uf-color-magenta-500", "#D84BFF"),
    ("--uf-color-ink-900", "#0F172A"),
    ("--uf-color-slate-600", "#475569"),
    ("--uf-color-mist-50", "#F8FAFC"),
];

pub(crate) fn render_product_card(renderer: &Renderer, out: &mut String, context: &str) {
    let title = renderer.theme().title;
    let subtitle = renderer.theme().subtitle;
    let cyan = Style::new().fg(CYAN).bold();
    let magenta = Style::new().fg(MAGENTA).bold();

    cyan.paint(renderer.color(), "u", out);
    magenta.paint(renderer.color(), "f", out);
    out.push_str("  ");
    title.paint(renderer.color(), HEADLINE, out);
    out.push_str("  ");
    subtitle.paint(renderer.color(), context, out);
    out.push('\n');

    out.push_str("    ");
    subtitle.paint(renderer.color(), TAGLINE, out);
    out.push('\n');

    out.push_str("    ");
    for (color, width) in [(CYAN, 8), (BLUE, 8), (INDIGO, 8), (VIOLET, 8), (MAGENTA, 8)] {
        Style::new().fg(color).open(renderer.color(), out);
        push_repeat(out, renderer.glyphs().horizontal, width);
        Style::new().fg(color).close(renderer.color(), out);
    }
    out.push('\n');

    out.push_str("    ");
    for (index, label) in ["Unified", "Fast", "Elegant", "Modern", "Developer-first"]
        .iter()
        .enumerate()
    {
        if index > 0 {
            out.push_str("  ");
        }
        renderer.theme().accent.paint(renderer.color(), label, out);
    }
    out.push('\n');
}

pub(crate) fn palette_table() -> Table<'static> {
    let mut table = Table::new(vec![Column::left("token"), Column::left("value")]);
    for &(token, value) in PALETTE {
        table.push(vec![
            Cell::toned(token, Tone::Accent),
            Cell::toned(value, Tone::Number),
        ]);
    }
    table
}
