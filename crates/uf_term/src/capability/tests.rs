//! Capability detection: the flag, the environment, and the stream.

use super::*;

fn env() -> TerminalEnv {
    TerminalEnv::default()
}

fn level(choice: ColorChoice, tty: Tty, env: &TerminalEnv) -> ColorLevel {
    Capabilities::detect(choice, tty, env).color()
}

#[test]
fn color_choice_parses_the_documented_spellings() {
    assert_eq!(ColorChoice::parse("auto"), Some(ColorChoice::Auto));
    assert_eq!(ColorChoice::parse("always"), Some(ColorChoice::Always));
    assert_eq!(ColorChoice::parse("never"), Some(ColorChoice::Never));
    assert_eq!(ColorChoice::parse("sometimes"), None);
    assert_eq!(ColorChoice::default().as_str(), "auto");
}

#[test]
fn a_piped_stream_gets_no_color_by_default() {
    assert_eq!(
        level(ColorChoice::Auto, Tty::Piped, &env()),
        ColorLevel::Never
    );
}

#[test]
fn an_interactive_stream_gets_sixteen_colors_by_default() {
    assert_eq!(
        level(ColorChoice::Auto, Tty::Interactive, &env()),
        ColorLevel::Ansi16
    );
}

#[test]
fn term_256color_upgrades_to_the_indexed_palette() {
    let env = env().with_term("xterm-256color");
    assert_eq!(
        level(ColorChoice::Auto, Tty::Interactive, &env),
        ColorLevel::Ansi256
    );
}

#[test]
fn colorterm_truecolor_upgrades_to_direct_color() {
    let env = env()
        .with_term("xterm-256color")
        .with_colorterm("truecolor");
    assert_eq!(
        level(ColorChoice::Auto, Tty::Interactive, &env),
        ColorLevel::TrueColor
    );
}

#[test]
fn colorterm_24bit_upgrades_to_direct_color() {
    let env = env().with_colorterm("24BIT");
    assert_eq!(
        level(ColorChoice::Auto, Tty::Interactive, &env),
        ColorLevel::TrueColor
    );
}

#[test]
fn term_direct_upgrades_to_direct_color() {
    let env = env().with_term("xterm-direct");
    assert_eq!(
        level(ColorChoice::Auto, Tty::Interactive, &env),
        ColorLevel::TrueColor
    );
}

#[test]
fn no_color_beats_an_interactive_terminal() {
    let env = env().with_term("xterm-256color").with_no_color("1");
    assert_eq!(
        level(ColorChoice::Auto, Tty::Interactive, &env),
        ColorLevel::Never
    );
}

#[test]
fn no_color_with_an_empty_value_does_not_disable_color() {
    let env = env().with_no_color("");
    assert_eq!(
        level(ColorChoice::Auto, Tty::Interactive, &env),
        ColorLevel::Ansi16
    );
}

#[test]
fn no_color_beats_force_color() {
    let env = env().with_no_color("1").with_force_color("3");
    assert_eq!(
        level(ColorChoice::Auto, Tty::Interactive, &env),
        ColorLevel::Never
    );
}

#[test]
fn force_color_levels_map_to_palettes() {
    assert_eq!(
        level(ColorChoice::Auto, Tty::Piped, &env().with_force_color("1")),
        ColorLevel::Ansi16
    );
    assert_eq!(
        level(ColorChoice::Auto, Tty::Piped, &env().with_force_color("2")),
        ColorLevel::Ansi256
    );
    assert_eq!(
        level(ColorChoice::Auto, Tty::Piped, &env().with_force_color("3")),
        ColorLevel::TrueColor
    );
    assert_eq!(
        level(ColorChoice::Auto, Tty::Piped, &env().with_force_color("0")),
        ColorLevel::Never
    );
}

#[test]
fn force_color_with_an_empty_value_enables_color_on_a_pipe() {
    let env = env().with_force_color("");
    assert_eq!(
        level(ColorChoice::Auto, Tty::Piped, &env),
        ColorLevel::Ansi16
    );
}

#[test]
fn clicolor_force_enables_color_on_a_pipe() {
    let env = env().with_clicolor_force("1");
    assert_eq!(
        level(ColorChoice::Auto, Tty::Piped, &env),
        ColorLevel::Ansi16
    );
}

#[test]
fn clicolor_force_zero_does_not_enable_color() {
    let env = env().with_clicolor_force("0");
    assert_eq!(
        level(ColorChoice::Auto, Tty::Piped, &env),
        ColorLevel::Never
    );
}

#[test]
fn clicolor_zero_disables_color_on_a_terminal() {
    let env = env().with_clicolor("0");
    assert_eq!(
        level(ColorChoice::Auto, Tty::Interactive, &env),
        ColorLevel::Never
    );
}

#[test]
fn term_dumb_disables_color_and_unicode() {
    let env = env().with_term("dumb");
    let caps = Capabilities::detect(ColorChoice::Auto, Tty::Interactive, &env);
    assert_eq!(caps.color(), ColorLevel::Never);
    assert_eq!(caps.glyphs(), GlyphSet::Ascii);
}

#[test]
fn color_never_beats_every_environment_switch() {
    let env = env()
        .with_force_color("3")
        .with_clicolor_force("1")
        .with_colorterm("truecolor");
    assert_eq!(
        level(ColorChoice::Never, Tty::Interactive, &env),
        ColorLevel::Never
    );
}

#[test]
fn color_always_beats_no_color_and_a_pipe() {
    let env = env().with_no_color("1").with_term("xterm-256color");
    assert_eq!(
        level(ColorChoice::Always, Tty::Piped, &env),
        ColorLevel::Ansi256
    );
}

#[test]
fn a_non_utf8_locale_downgrades_glyphs_but_keeps_color() {
    let env = env().with_locale("C").with_term("xterm-256color");
    let caps = Capabilities::detect(ColorChoice::Auto, Tty::Interactive, &env);
    assert_eq!(caps.glyphs(), GlyphSet::Ascii);
    assert_eq!(caps.color(), ColorLevel::Ansi256);
}

#[test]
fn a_utf8_locale_keeps_unicode_glyphs() {
    for locale in ["en_US.UTF-8", "ja_JP.utf8", "C.UTF-8"] {
        let caps = Capabilities::detect(
            ColorChoice::Auto,
            Tty::Interactive,
            &env().with_locale(locale),
        );
        assert_eq!(caps.glyphs(), GlyphSet::Unicode, "locale {locale}");
    }
}

#[test]
fn an_unset_locale_keeps_unicode_glyphs() {
    assert!(Capabilities::detect(ColorChoice::Auto, Tty::Piped, &env()).is_unicode());
}

#[test]
fn no_color_also_downgrades_glyphs() {
    let caps = Capabilities::detect(
        ColorChoice::Auto,
        Tty::Interactive,
        &env().with_no_color("1"),
    );
    assert_eq!(caps.glyphs(), GlyphSet::Ascii);
}

#[test]
fn plain_capabilities_are_the_conservative_floor() {
    let caps = Capabilities::plain();
    assert_eq!(caps.color(), ColorLevel::Never);
    assert_eq!(caps.glyphs(), GlyphSet::Ascii);
    assert!(!caps.is_interactive());
    assert!(!caps.is_unicode());
}

#[test]
fn interactivity_is_independent_of_color() {
    let caps = Capabilities::detect(
        ColorChoice::Never,
        Tty::Interactive,
        &env().with_term("xterm-256color"),
    );
    assert!(caps.is_interactive());
    assert!(!caps.color().is_enabled());
}

#[test]
fn color_levels_are_ordered_from_least_to_most_capable() {
    assert!(ColorLevel::Never < ColorLevel::Ansi16);
    assert!(ColorLevel::Ansi16 < ColorLevel::Ansi256);
    assert!(ColorLevel::Ansi256 < ColorLevel::TrueColor);
    assert!(!ColorLevel::Never.is_enabled());
    assert!(ColorLevel::Ansi16.is_enabled());
}

#[test]
fn case_insensitive_contains_handles_edges() {
    assert!(contains_ignore_ascii_case("TrueColor", "truecolor"));
    assert!(contains_ignore_ascii_case("xterm-256color", "256"));
    assert!(!contains_ignore_ascii_case("xterm", "256"));
    assert!(!contains_ignore_ascii_case("ab", "abc"));
    assert!(contains_ignore_ascii_case("abc", ""));
}
