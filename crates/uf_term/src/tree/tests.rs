//! Trees: path building, ordering, and branch drawing.

use super::*;
use crate::capability::{Capabilities, ColorLevel, GlyphSet, Tty};
use crate::text::display_width;

fn renderer(color: ColorLevel, glyphs: GlyphSet) -> Renderer {
    Renderer::new(Capabilities::new(color, glyphs, Tty::Piped))
}

fn plain() -> Renderer {
    renderer(ColorLevel::Never, GlyphSet::Unicode)
}

#[test]
fn a_tree_uses_box_drawing_branches() {
    let mut out = String::new();
    let tree = Tree::from_paths("demo", ["app/page.js", "app/layout.js", "uf.config.js"]);
    plain().tree(&mut out, 0, &tree);

    assert_eq!(
        out,
        "demo\n├─ app\n│  ├─ layout.js\n│  └─ page.js\n└─ uf.config.js\n"
    );
}

#[test]
fn a_tree_nests_deeply_without_losing_its_trunk() {
    let mut out = String::new();
    let tree = Tree::from_paths("root", ["a/b/c/d.js", "a/b/e.js", "f.js"]);
    plain().tree(&mut out, 0, &tree);

    assert_eq!(
        out,
        "root\n├─ a\n│  └─ b\n│     ├─ c\n│     │  └─ d.js\n│     └─ e.js\n└─ f.js\n"
    );
}

#[test]
fn a_tree_puts_directories_before_files() {
    let tree = Tree::from_paths("root", ["z.js", "a/b.js"]);
    let labels: Vec<_> = tree.children().iter().map(Tree::label).collect();

    assert_eq!(labels, ["a", "z.js"]);
}

#[test]
fn a_tree_ignores_empty_path_segments() {
    let tree = Tree::from_paths("root", ["/a//b.js", ""]);
    assert_eq!(tree.children().len(), 1);
    assert_eq!(tree.children()[0].label(), "a");
}

#[test]
fn a_tree_merges_repeated_paths() {
    let tree = Tree::from_paths("root", ["a/b.js", "a/b.js"]);
    assert_eq!(tree.children().len(), 1);
    assert_eq!(tree.children()[0].children().len(), 1);
}

#[test]
fn an_empty_tree_is_just_its_root() {
    let mut out = String::new();
    plain().tree(&mut out, 0, &Tree::new("root"));
    assert_eq!(out, "root\n");
}

#[test]
fn an_ascii_tree_uses_ascii_branches() {
    let mut out = String::new();
    renderer(ColorLevel::Never, GlyphSet::Ascii).tree(
        &mut out,
        0,
        &Tree::from_paths("demo", ["a/b.js", "c.js"]),
    );

    assert!(out.is_ascii(), "{out}");
    assert!(out.contains("|- a"));
    assert!(out.contains("`- c.js"));
}

#[test]
fn an_indented_tree_shifts_every_row_equally() {
    let mut out = String::new();
    plain().tree(&mut out, 4, &Tree::from_paths("demo", ["a/b.js", "c.js"]));

    for line in out.lines() {
        assert!(line.starts_with("    "), "{line:?}");
    }
}

#[test]
fn styling_never_changes_the_rendered_geometry() {
    let tree = Tree::from_paths("demo", ["a/b.js", "c.js"]);
    let mut styled = String::new();
    renderer(ColorLevel::TrueColor, GlyphSet::Unicode).tree(&mut styled, 0, &tree);
    let mut plain_out = String::new();
    plain().tree(&mut plain_out, 0, &tree);

    let styled_widths: Vec<_> = styled.lines().map(display_width).collect();
    let plain_widths: Vec<_> = plain_out.lines().map(display_width).collect();
    assert_eq!(styled_widths, plain_widths);
}

#[test]
fn rendering_is_idempotent_across_repeated_calls() {
    let renderer = plain();
    let tree = Tree::from_paths("demo", ["a/b.js", "c.js"]);
    let mut first = String::new();
    let mut second = String::new();
    renderer.tree(&mut first, 0, &tree);
    renderer.tree(&mut second, 0, &tree);

    assert_eq!(first, second);
}

#[test]
fn a_tree_built_by_hand_matches_one_built_from_paths() {
    let mut manual = Tree::new("demo");
    manual.child("app").child("page.js");
    manual.child("uf.config.js");
    manual.sort();

    assert_eq!(
        manual,
        Tree::from_paths("demo", ["app/page.js", "uf.config.js"])
    );
}
