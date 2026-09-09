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

/// `NO_COLOR` takes the colour and leaves the characters.
///
/// It asserted the opposite until ubugeeei-prod/uf#393: uf and
/// `@uniflowed/tui` made opposite decisions here, both deliberately, and a
/// project whose CLI and whose TUI library disagree in the same shell is the
/// failure #316 names. The convention at no-color.org is about ANSI colour and
/// says nothing about characters, so this follows the JavaScript rather than
/// the other way round — and a UTF-8 terminal keeps its box-drawing.
#[test]
fn no_color_takes_the_colour_and_leaves_the_glyphs() {
    let caps = Capabilities::detect(
        ColorChoice::Auto,
        Tty::Interactive,
        &env().with_no_color("1"),
    );
    assert_eq!(caps.color(), ColorLevel::Never);
    assert_eq!(caps.glyphs(), GlyphSet::Unicode);
}

/// And what does still take the glyphs down, so the rule is not merely looser.
#[test]
fn a_dumb_terminal_or_a_non_utf8_locale_downgrades_glyphs() {
    let dumb = Capabilities::detect(
        ColorChoice::Auto,
        Tty::Interactive,
        &env().with_term("dumb"),
    );
    assert_eq!(dumb.glyphs(), GlyphSet::Ascii);

    let latin1 = Capabilities::detect(
        ColorChoice::Auto,
        Tty::Interactive,
        &env().with_locale("en_US.ISO-8859-1"),
    );
    assert_eq!(latin1.glyphs(), GlyphSet::Ascii);
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

// --- inline images -----------------------------------------------------

/// An image is a much larger escape sequence than a colour, so a stream that
/// may not carry colour may not carry one either.
#[test]
fn colour_being_off_turns_inline_images_off() {
    let kitty = ImageEnv::default().with_term("xterm-kitty");

    assert_eq!(
        detect_image(ColorLevel::Never, Tty::Interactive, &kitty),
        None
    );
    assert_eq!(
        detect_image(ColorLevel::TrueColor, Tty::Interactive, &kitty),
        Some(ImageProtocol::Kitty)
    );
}

/// A picture in a pipe is bytes in a log.
#[test]
fn a_piped_stream_never_gets_an_inline_image() {
    let kitty = ImageEnv::default().with_term("xterm-kitty");

    assert_eq!(
        detect_image(ColorLevel::TrueColor, Tty::Piped, &kitty),
        None
    );
}

#[test]
fn a_terminal_that_speaks_no_protocol_gets_no_image_however_capable_it_is() {
    let plain = ImageEnv::default().with_term("xterm-256color");

    assert_eq!(
        detect_image(ColorLevel::TrueColor, Tty::Interactive, &plain),
        None
    );
}

#[test]
fn the_conservative_floor_carries_no_image_protocol() {
    assert_eq!(Capabilities::plain().image(), None);
    assert_eq!(
        Capabilities::new(ColorLevel::TrueColor, GlyphSet::Unicode, Tty::Interactive).image(),
        None,
        "a directly built capability opts in explicitly"
    );
    assert_eq!(
        Capabilities::new(ColorLevel::TrueColor, GlyphSet::Unicode, Tty::Interactive)
            .with_image(Some(ImageProtocol::ITerm2))
            .image(),
        Some(ImageProtocol::ITerm2)
    );
}

/// `COLUMNS` and `LINES` are the override, and each one stands on its own.
#[test]
fn the_environment_is_asked_before_the_terminal_is() {
    let env = env().with_columns("40").with_lines("12");
    let size = detect_size(&env, Some((100, 30)));

    assert_eq!((size.columns(), size.rows()), (40, 12));
}

#[test]
fn a_half_answered_environment_takes_the_other_half_from_the_terminal() {
    // `COLUMNS` without `LINES` is the common shape — a wrapper that cares
    // about width sets one of them — so the two are resolved separately
    // rather than as a pair that is present or absent.
    let env = env().with_columns("40");
    let size = detect_size(&env, Some((100, 30)));

    assert_eq!((size.columns(), size.rows()), (40, 30));
}

#[test]
fn the_terminal_answers_when_the_environment_will_not() {
    let size = detect_size(&env(), Some((132, 43)));

    assert_eq!((size.columns(), size.rows()), (132, 43));
}

#[test]
fn a_terminal_the_environment_already_described_is_not_asked() {
    // The probe is a process spawn. Paying for it to get an answer that the
    // next line would discard is the kind of cost nobody notices until a
    // command that runs a hundred times does it.
    let env = env().with_columns("40").with_lines("12");

    assert_eq!(
        TerminalSize::detect(Tty::Interactive, &env),
        TerminalSize::new(40, 12)
    );
}

#[test]
fn a_stream_nobody_is_watching_is_never_measured() {
    // A piped stream draws no region at all, so paying a process spawn to
    // find out how wide the window behind it is buys nothing. There is no
    // observable difference to assert here other than the size it settles on,
    // so the branch is stated where it lives and asserted by its result.
    assert_eq!(
        TerminalSize::detect(Tty::Piped, &env()),
        TerminalSize::fallback()
    );
}

#[test]
fn a_variable_that_names_no_number_falls_through_to_the_next_rule() {
    for value in ["", "0", "wide", "-1", "80x24"] {
        let env = env().with_columns(value);
        let size = detect_size(&env, Some((132, 43)));
        assert_eq!(
            size.columns(),
            132,
            "COLUMNS={value:?} says nothing usable and must not win"
        );
    }
}

#[test]
fn a_terminal_that_reports_nothing_gets_the_fallback() {
    let size = detect_size(&env(), None);

    assert_eq!(size, TerminalSize::fallback());
    assert_eq!((size.columns(), size.rows()), (80, 24));
}

#[test]
fn a_zero_dimension_is_a_terminal_that_is_not_ready_yet() {
    // Some CI shells answer `stty size` with `0 0`. A region zero columns wide
    // is worse than one laid out for eighty.
    let size = detect_size(&env(), Some((0, 0)));

    assert_eq!((size.columns(), size.rows()), (80, 24));
}

#[test]
fn stty_size_reports_rows_before_columns() {
    assert_eq!(parse_stty_size("43 132\n"), Some((132, 43)));
    assert_eq!(parse_stty_size("  24   80  "), Some((80, 24)));
    assert_eq!(parse_stty_size(""), None);
    assert_eq!(parse_stty_size("43"), None);
    assert_eq!(parse_stty_size("stty: stdin: Not a typewriter"), None);
}

#[test]
fn stty_is_run_from_a_root_owned_directory_and_not_from_path() {
    // The whole of CWE-426 is *which directory* the program came out of. A
    // relative name, or an absolute one under a directory an unprivileged
    // process can write to, is a program somebody else chooses.
    for program in STTY_PROGRAMS {
        let path = Path::new(program);
        assert!(
            path.is_absolute(),
            "{program} would be resolved through PATH"
        );
        assert!(
            path.parent() == Some(Path::new("/bin"))
                || path.parent() == Some(Path::new("/usr/bin")),
            "{program} is outside POSIX's default utility path"
        );
    }
    // And the resolver hands back one of those or nothing — never a path it
    // went looking for.
    if let Some(resolved) = stty_program() {
        assert!(
            STTY_PROGRAMS
                .iter()
                .any(|program| Path::new(program) == resolved),
            "{} is not one of the paths this crate trusts",
            resolved.display()
        );
    }
}

#[test]
fn no_subprocess_in_this_module_is_named_by_anything_but_an_absolute_path() {
    // The finding is about *names*, so the guard has to be about names rather
    // than about the one call site that had one. A future `Command::new` given
    // a bare program here is the same vulnerability again, and this fails on
    // it whichever line it is written on.
    let source = include_str!("../capability.rs");
    for (index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }
        let Some((_, rest)) = line.split_once("Command::new(\"") else {
            continue;
        };
        let named = rest.split('"').next().unwrap_or_default();
        assert!(
            named.starts_with('/'),
            "capability.rs:{}: `{named}` is resolved through the inherited PATH",
            index + 1
        );
    }
}
