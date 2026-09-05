//! The menu's logic, which is filtering and scrolling.
//!
//! Nothing here opens a terminal. The two things a picker gets wrong are which
//! rows it shows and which one is highlighted, and both are decided by pure
//! functions on purpose so they can be asserted rather than looked at.

use crate::capability::{Capabilities, ColorChoice, GlyphSet, TerminalEnv, Tty};
use crate::theme::Theme;

use super::draw;
use super::menu::{Choice, Menu, VISIBLE};

const CHOICES: &[Choice<'static>] = &[
    Choice::new("build", "Build the project for production"),
    Choice::new("check", "Lint the project, then type check it with Flow"),
    Choice::new("dev", "Start the development server"),
    Choice::new("fmt", "Format every file uf understands"),
    Choice::new("lint", "Lint the project"),
    Choice::new("test", "Run the test suite"),
];

fn names<'a>(menu: &Menu<'a>) -> Vec<&'a str> {
    menu.visible().map(|(choice, _)| choice.name).collect()
}

fn highlighted<'a>(menu: &Menu<'a>) -> Option<&'a str> {
    menu.selected().map(|choice| choice.name)
}

#[test]
fn everything_shows_before_anything_is_typed() {
    let menu = Menu::new(CHOICES);

    assert_eq!(names(&menu).len(), CHOICES.len());
    assert_eq!(highlighted(&menu), Some("build"));
}

#[test]
fn a_prefix_of_a_name_ranks_above_a_name_that_merely_contains_it() {
    // "lint" is `lint`'s whole name and the tail of nothing else here, but
    // `check`'s description contains it. The name has to come first.
    let mut menu = Menu::new(CHOICES);
    for character in "lint".chars() {
        menu.push(character);
    }

    assert_eq!(names(&menu), vec!["lint", "check"]);
    assert_eq!(highlighted(&menu), Some("lint"));
}

#[test]
fn a_description_match_is_offered_when_no_name_matches() {
    let mut menu = Menu::new(CHOICES);
    for character in "server".chars() {
        menu.push(character);
    }

    assert_eq!(names(&menu), vec!["dev"]);
}

#[test]
fn a_subsequence_is_the_last_resort_rather_than_the_first() {
    // `bld` is not a prefix, not a substring, and in no description. It is in
    // `build` as a subsequence, and that is the only reason to offer it.
    let mut menu = Menu::new(CHOICES);
    for character in "bld".chars() {
        menu.push(character);
    }

    assert_eq!(names(&menu), vec!["build"]);
}

#[test]
fn filtering_is_case_insensitive() {
    let mut menu = Menu::new(CHOICES);
    for character in "BUILD".chars() {
        menu.push(character);
    }

    assert_eq!(names(&menu), vec!["build"]);
}

#[test]
fn a_filter_that_matches_nothing_says_so_rather_than_showing_everything() {
    let mut menu = Menu::new(CHOICES);
    for character in "zzz".chars() {
        menu.push(character);
    }

    assert!(menu.is_empty());
    assert_eq!(highlighted(&menu), None);
}

#[test]
fn backspace_puts_back_what_it_took() {
    let mut menu = Menu::new(CHOICES);
    for character in "buildx".chars() {
        menu.push(character);
    }
    assert!(menu.is_empty());

    menu.backspace();

    assert_eq!(names(&menu), vec!["build"]);
}

#[test]
fn clearing_the_filter_shows_everything_again() {
    let mut menu = Menu::new(CHOICES);
    for character in "dev".chars() {
        menu.push(character);
    }
    menu.clear();

    assert_eq!(names(&menu).len(), CHOICES.len());
    assert_eq!(menu.filter(), "");
}

#[test]
fn the_highlight_returns_to_the_top_on_every_keystroke() {
    // Otherwise the row under the highlight changes meaning as the list is
    // filtered, and Enter runs whatever happened to land there.
    let mut menu = Menu::new(CHOICES);
    menu.down();
    menu.down();
    assert_eq!(highlighted(&menu), Some("dev"));

    menu.push('t');

    assert_eq!(highlighted(&menu), Some("test"));
}

#[test]
fn moving_up_from_the_top_wraps_to_the_bottom() {
    let mut menu = Menu::new(CHOICES);

    menu.up();

    assert_eq!(highlighted(&menu), Some("test"));
}

#[test]
fn moving_down_from_the_bottom_wraps_to_the_top() {
    let mut menu = Menu::new(CHOICES);
    for _ in 0..CHOICES.len() - 1 {
        menu.down();
    }
    assert_eq!(highlighted(&menu), Some("test"));

    menu.down();

    assert_eq!(highlighted(&menu), Some("build"));
}

#[test]
fn moving_in_an_empty_menu_does_nothing_rather_than_panicking() {
    let mut menu = Menu::new(CHOICES);
    for character in "zzz".chars() {
        menu.push(character);
    }

    menu.up();
    menu.down();

    assert_eq!(highlighted(&menu), None);
}

/// Enough choices to need scrolling, named so their order is obvious.
fn many() -> Vec<Choice<'static>> {
    const NAMES: [&str; 14] = [
        "a0", "a1", "a2", "a3", "a4", "a5", "a6", "a7", "a8", "a9", "b0", "b1", "b2", "b3",
    ];
    NAMES.iter().map(|name| Choice::new(name, "")).collect()
}

#[test]
fn a_long_list_shows_a_window_rather_than_all_of_it() {
    let choices = many();
    let menu = Menu::new(&choices);

    assert_eq!(names(&menu).len(), VISIBLE);
    assert_eq!(menu.hidden_below(), choices.len() - VISIBLE);
    assert_eq!(menu.hidden_above(), 0);
}

#[test]
fn the_window_follows_the_highlight_down() {
    let choices = many();
    let mut menu = Menu::new(&choices);
    for _ in 0..VISIBLE {
        menu.down();
    }

    assert_eq!(highlighted(&menu), Some("b0"));
    assert!(names(&menu).contains(&"b0"), "{:?}", names(&menu));
    assert_eq!(menu.hidden_above(), 1);
}

#[test]
fn wrapping_to_the_bottom_scrolls_the_window_to_it() {
    let choices = many();
    let mut menu = Menu::new(&choices);

    menu.up();

    assert_eq!(highlighted(&menu), Some("b3"));
    assert!(names(&menu).contains(&"b3"));
    assert_eq!(menu.hidden_below(), 0);
}

#[test]
fn the_description_column_is_measured_over_what_is_visible() {
    // A filter that leaves only short names must not keep indenting them past
    // a long name that is no longer on screen.
    const WIDE: &[Choice<'static>] = &[
        Choice::new("a-very-long-command-name", "long"),
        Choice::new("fmt", "short"),
    ];
    let mut menu = Menu::new(WIDE);
    assert_eq!(menu.name_width(), "a-very-long-command-name".len());

    for character in "fmt".chars() {
        menu.push(character);
    }

    assert_eq!(menu.name_width(), 3);
}

fn plain_frame(theme: &Theme) -> draw::Frame<'_> {
    draw::Frame {
        title: "What would you like to run?",
        placeholder: "type to filter",
        capabilities: Capabilities::detect(
            ColorChoice::Never,
            Tty::Interactive,
            &TerminalEnv::default(),
        ),
        theme,
    }
}

/// The name column of every row in a drawn frame, in order.
///
/// Reading the rows rather than searching the whole frame for a name, because
/// a name also appears inside a description — `check`'s own says "type check"
/// — and counting those would assert the wrong thing.
fn drawn_names(out: &str) -> Vec<String> {
    out.lines()
        .filter_map(|line| line.strip_suffix("\x1b[K"))
        .filter_map(|line| line.strip_prefix("  "))
        .filter_map(|line| {
            line.strip_prefix("\u{276f} ")
                .or_else(|| line.strip_prefix("  "))
        })
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_owned)
        .collect()
}

#[test]
fn a_frame_names_every_visible_choice_once() {
    let theme = Theme::default();
    let menu = Menu::new(CHOICES);
    let mut out = String::new();

    draw::frame(&menu, &plain_frame(&theme), &mut out);

    let drawn = drawn_names(&out);
    let expected: Vec<String> = CHOICES
        .iter()
        .map(|choice| choice.name.to_owned())
        .collect();
    assert_eq!(drawn, expected, "{out:?}");
}

#[test]
fn a_frame_marks_exactly_one_row() {
    let theme = Theme::default();
    let mut menu = Menu::new(CHOICES);
    menu.down();
    let mut out = String::new();

    draw::frame(&menu, &plain_frame(&theme), &mut out);

    let marked: Vec<&str> = out
        .lines()
        .filter(|line| line.starts_with("  \u{276f} "))
        .collect();
    assert_eq!(marked.len(), 1, "{out:?}");
    assert!(marked[0].contains("check"), "{out:?}");
}

#[test]
fn a_frame_at_colour_never_contains_no_escape_byte_but_the_line_clears() {
    // The clear is a control sequence rather than a colour, and it is what
    // keeps a shrinking frame from leaving its old rows behind — so it stays
    // even when nothing is being coloured.
    let theme = Theme::default();
    let menu = Menu::new(CHOICES);
    let mut out = String::new();

    draw::frame(&menu, &plain_frame(&theme), &mut out);

    for sequence in out.split("\x1b[K") {
        assert!(!sequence.contains('\x1b'), "unexpected escape in {out:?}");
    }
}

#[test]
fn a_frame_says_how_many_are_off_screen() {
    let theme = Theme::default();
    let choices = many();
    let menu = Menu::new(&choices);
    let mut out = String::new();

    draw::frame(&menu, &plain_frame(&theme), &mut out);

    assert!(out.contains("4 more"), "{out}");
}

#[test]
fn a_frame_with_nothing_matching_says_so() {
    let theme = Theme::default();
    let mut menu = Menu::new(CHOICES);
    for character in "zzz".chars() {
        menu.push(character);
    }
    let mut out = String::new();

    draw::frame(&menu, &plain_frame(&theme), &mut out);

    assert!(out.contains("nothing matches"), "{out}");
    assert!(out.contains("zzz"), "the filter is still shown: {out}");
}

#[test]
fn an_ascii_terminal_gets_ascii_marks() {
    let theme = Theme::default();
    let menu = Menu::new(CHOICES);
    let mut frame = plain_frame(&theme);
    // A locale that is not UTF-8, which is what picks the ASCII vocabulary.
    let env = TerminalEnv {
        locale: Some("C".to_owned()),
        ..TerminalEnv::default()
    };
    frame.capabilities = Capabilities::detect(ColorChoice::Never, Tty::Interactive, &env);
    assert_eq!(frame.capabilities.glyphs(), GlyphSet::Ascii);
    let mut out = String::new();

    draw::frame(&menu, &frame, &mut out);

    assert!(!out.contains('❯'), "{out}");
    assert!(!out.contains('↑'), "{out}");
    assert!(out.contains("enter run"), "{out}");
}

#[test]
fn a_group_heading_is_written_once_per_group() {
    const GROUPED: &[Choice<'static>] = &[
        Choice::grouped("build", "", "commands"),
        Choice::grouped("test", "", "commands"),
        Choice::grouped("run ci", "", "tasks"),
    ];
    let theme = Theme::default();
    let menu = Menu::new(GROUPED);
    let mut out = String::new();

    draw::frame(&menu, &plain_frame(&theme), &mut out);

    assert_eq!(out.matches("commands").count(), 1, "{out}");
    assert_eq!(out.matches("tasks").count(), 1, "{out}");
}
