//! Brand-aware terminal snippets shared by human-facing CLI commands.

use uf_term::{Cell, Color, Column, GlyphSet, Renderer, Style, Table, Tone, push_repeat};

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

/// The mark, five rows tall, in the brand's five stops.
///
/// One colour per row rather than per character: a per-character gradient
/// would have to slice a string made of multi-byte block characters, and the
/// result is worth less than the cost. Top to bottom is also the direction the
/// documentation site runs its own gradient, so the two read as one thing.
const MARK: [&str; 5] = [
    "██    ██   ████████",
    "██    ██   ██",
    "██    ██   ██████",
    "██    ██   ██",
    " ██████    ██",
];

/// The same mark where block characters cannot be drawn.
///
/// A terminal in a non-UTF-8 locale, or one calling itself `dumb`, gets `#` —
/// which is not as handsome and is legible, and the second matters more. The
/// rest of this CLI already falls back the same way, so a mark that did not
/// would be the one thing on screen printing replacement characters.
const ASCII_MARK: [&str; 5] = [
    "##    ##   ########",
    "##    ##   ##",
    "##    ##   ######",
    "##    ##   ##",
    " ######    ##",
];

/// The mark and the headline, for the moments that deserve it.
///
/// Not on every command. `uf test` printing a five-row logo before a hundred
/// test results is not beautiful, it is in the way — this is for first contact
/// (`uf create`) and for the command whose whole job is to say what you have
/// (`uf info`).
pub(crate) fn render_mark(renderer: &Renderer, out: &mut String, context: &str) {
    let stops = [CYAN, BLUE, INDIGO, VIOLET, MAGENTA];
    let rows = match renderer.glyph_set() {
        GlyphSet::Ascii => &ASCII_MARK,
        GlyphSet::Unicode => &MARK,
    };

    out.push('\n');
    for (row, colour) in rows.iter().zip(stops) {
        out.push_str("  ");
        Style::new()
            .fg(colour)
            .bold()
            .paint(renderer.color(), row, out);
        out.push('\n');
    }
    out.push('\n');

    out.push_str("  ");
    renderer
        .theme()
        .title
        .paint(renderer.color(), HEADLINE, out);
    out.push_str("  ");
    renderer
        .theme()
        .subtitle
        .paint(renderer.color(), context, out);
    out.push('\n');
}

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
