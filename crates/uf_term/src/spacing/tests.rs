//! A rule is never followed by a blank line.

use super::*;
use crate::capability::{Capabilities, ColorLevel, GlyphSet, Tty};
use crate::render::Renderer;

#[test]
fn a_rule_is_a_line_of_one_glyph_at_the_left_margin() {
    assert!(is_rule_line("────"));
    assert!(is_rule_line("------------"));
    assert!(is_rule_line("\x1b[90m──────\x1b[0m"));
    // Too short to be a banner's rule, indented, mixed, or empty.
    assert!(!is_rule_line("───"));
    assert!(!is_rule_line("    ----"));
    assert!(!is_rule_line("──--"));
    assert!(!is_rule_line(""));
    assert!(!is_rule_line("--- a/file.js"));
}

#[test]
fn the_blank_lines_under_a_rule_are_dropped() {
    assert_eq!(
        RuleSpacing::tighten("uf lint · app\n─────────────\n\n\nsrc/a.js\n\nnext\n"),
        "uf lint · app\n─────────────\nsrc/a.js\n\nnext\n"
    );
}

/// The banner and the report under it are usually two writes.
#[test]
fn the_rule_is_remembered_between_writes() {
    let mut spacing = RuleSpacing::new();
    let mut out = String::new();
    spacing.apply("uf fmt · app\n────────────\n", &mut out);
    spacing.apply("\n", &mut out);
    spacing.apply("\n  - src/a.js\n", &mut out);
    assert_eq!(out, "uf fmt · app\n────────────\n  - src/a.js\n");
}

#[test]
fn anything_written_in_between_ends_the_rule() {
    let mut spacing = RuleSpacing::new();
    let mut out = String::new();
    spacing.apply("title\n─────\n", &mut out);
    spacing.interrupt();
    spacing.apply("\nafter\n", &mut out);
    assert_eq!(out, "title\n─────\n\nafter\n");

    // A write that stops partway through a line continues it next time.
    let mut spacing = RuleSpacing::new();
    let mut out = String::new();
    spacing.apply("─────\npartial", &mut out);
    spacing.apply("\n\nnext\n", &mut out);
    assert_eq!(out, "─────\npartial\n\nnext\n");
}

#[test]
fn blank_lines_elsewhere_are_kept() {
    let text = "one\n\ntwo\n\n\nthree\n";
    assert_eq!(RuleSpacing::tighten(text), text);
    assert_eq!(blank_after_rule(text), None);
}

#[test]
fn a_blank_line_after_a_rule_is_found() {
    assert_eq!(blank_after_rule("title\n─────\n\nbody\n"), Some(2));
    assert_eq!(blank_after_rule("title\n─────\nbody\n"), None);
}

/// Every glyph set and colour level: the banner the renderer draws is a rule
/// line, and so the spacing applies to it.
#[test]
fn every_banner_ends_in_a_rule_line() {
    for glyphs in [GlyphSet::Unicode, GlyphSet::Ascii] {
        for color in [ColorLevel::Never, ColorLevel::Ansi16, ColorLevel::TrueColor] {
            let renderer = Renderer::new(Capabilities::new(color, glyphs, Tty::Piped));
            let mut out = String::new();
            renderer.banner(&mut out, "uf", None);
            renderer.blank(&mut out);
            out.push_str("body\n");
            let tightened = RuleSpacing::tighten(&out);
            assert_eq!(blank_after_rule(&tightened), None, "{glyphs:?} {color:?}");
            assert!(tightened.ends_with("\nbody\n"), "{tightened:?}");
            assert_eq!(tightened.lines().count(), 3, "{tightened:?}");
        }
    }
}
