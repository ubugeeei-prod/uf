//! The SGR layer: escape sequences, palettes, and downgrade rules.

use super::*;

fn paint(style: Style, level: ColorLevel) -> String {
    let mut out = String::new();
    style.paint(level, "text", &mut out);
    out
}

#[test]
fn a_style_renders_nothing_when_color_is_off() {
    let style = Style::new().fg(Color::Red).bold().underline();
    let painted = paint(style, ColorLevel::Never);

    assert_eq!(painted, "text");
    assert!(!painted.contains('\x1b'));
}

#[test]
fn every_theme_style_is_escape_free_when_color_is_off() {
    let styles = [
        Style::new().fg(Color::Rgb(1, 2, 3)).bg(Color::Indexed(7)),
        Style::new().bold().dim().italic().underline(),
        Style::new().fg(Color::BrightMagenta),
    ];
    for style in styles {
        assert!(!paint(style, ColorLevel::Never).contains('\x1b'));
    }
}

#[test]
fn an_empty_style_never_writes_escapes() {
    assert_eq!(paint(Style::new(), ColorLevel::TrueColor), "text");
}

#[test]
fn base_foreground_colors_use_the_thirty_range() {
    assert_eq!(
        paint(Style::new().fg(Color::Red), ColorLevel::Ansi16),
        "\x1b[31mtext\x1b[0m"
    );
    assert_eq!(
        paint(Style::new().fg(Color::White), ColorLevel::Ansi16),
        "\x1b[37mtext\x1b[0m"
    );
}

#[test]
fn bright_foreground_colors_use_the_ninety_range() {
    assert_eq!(
        paint(Style::new().fg(Color::BrightBlack), ColorLevel::Ansi16),
        "\x1b[90mtext\x1b[0m"
    );
    assert_eq!(
        paint(Style::new().fg(Color::BrightWhite), ColorLevel::Ansi16),
        "\x1b[97mtext\x1b[0m"
    );
}

#[test]
fn backgrounds_use_the_forty_and_hundred_ranges() {
    assert_eq!(
        paint(Style::new().bg(Color::Blue), ColorLevel::Ansi16),
        "\x1b[44mtext\x1b[0m"
    );
    assert_eq!(
        paint(Style::new().bg(Color::BrightBlue), ColorLevel::Ansi16),
        "\x1b[104mtext\x1b[0m"
    );
}

#[test]
fn attributes_are_emitted_in_ascending_order() {
    assert_eq!(
        paint(
            Style::new().underline().italic().dim().bold(),
            ColorLevel::Ansi16
        ),
        "\x1b[1;2;3;4mtext\x1b[0m"
    );
}

#[test]
fn attributes_and_colors_combine_in_one_sequence() {
    assert_eq!(
        paint(
            Style::new().bold().fg(Color::Green).bg(Color::Black),
            ColorLevel::Ansi16
        ),
        "\x1b[1;32;40mtext\x1b[0m"
    );
}

#[test]
fn indexed_colors_render_at_the_256_level() {
    assert_eq!(
        paint(Style::new().fg(Color::Indexed(75)), ColorLevel::Ansi256),
        "\x1b[38;5;75mtext\x1b[0m"
    );
}

#[test]
fn rgb_renders_directly_at_the_truecolor_level() {
    assert_eq!(
        paint(
            Style::new().fg(Color::Rgb(0x5f, 0xaf, 0xff)),
            ColorLevel::TrueColor
        ),
        "\x1b[38;2;95;175;255mtext\x1b[0m"
    );
}

#[test]
fn rgb_downgrades_into_the_cube_at_the_256_level() {
    assert_eq!(
        Color::Rgb(0x5f, 0xaf, 0xff).downgrade(ColorLevel::Ansi256),
        Color::Indexed(75)
    );
    assert_eq!(
        paint(
            Style::new().fg(Color::Rgb(0x5f, 0xaf, 0xff)),
            ColorLevel::Ansi256
        ),
        "\x1b[38;5;75mtext\x1b[0m"
    );
}

#[test]
fn grey_rgb_downgrades_into_the_grey_ramp() {
    assert_eq!(
        Color::Rgb(0x80, 0x80, 0x80).downgrade(ColorLevel::Ansi256),
        Color::Indexed(244)
    );
    assert_eq!(
        Color::Rgb(0, 0, 0).downgrade(ColorLevel::Ansi256),
        Color::Indexed(16)
    );
    assert_eq!(
        Color::Rgb(255, 255, 255).downgrade(ColorLevel::Ansi256),
        Color::Indexed(231)
    );
}

#[test]
fn rgb_downgrades_to_a_base_color_at_the_sixteen_level() {
    assert_eq!(
        Color::Rgb(0xff, 0x00, 0x00).downgrade(ColorLevel::Ansi16),
        Color::BrightRed
    );
    assert_eq!(
        Color::Rgb(0x80, 0x00, 0x00).downgrade(ColorLevel::Ansi16),
        Color::Red
    );
    assert_eq!(
        Color::Rgb(0x40, 0x00, 0x00).downgrade(ColorLevel::Ansi16),
        Color::Black
    );
    assert_eq!(
        Color::Rgb(0x5f, 0xaf, 0xff).downgrade(ColorLevel::Ansi16),
        Color::BrightCyan
    );
}

#[test]
fn indexed_downgrades_to_a_base_color_at_the_sixteen_level() {
    assert_eq!(
        Color::Indexed(9).downgrade(ColorLevel::Ansi16),
        Color::BrightRed
    );
    assert_eq!(
        Color::Indexed(231).downgrade(ColorLevel::Ansi16),
        Color::BrightWhite
    );
    assert_eq!(
        Color::Indexed(232).downgrade(ColorLevel::Ansi16),
        Color::Black
    );
}

#[test]
fn downgrade_is_idempotent() {
    for level in [ColorLevel::Ansi16, ColorLevel::Ansi256] {
        let once = Color::Rgb(0x21, 0x99, 0x33).downgrade(level);
        assert_eq!(once.downgrade(level), once);
    }
}

#[test]
fn the_cube_and_grey_ramp_round_trip_through_their_own_levels() {
    for index in 16u8..=255 {
        let (r, g, b) = indexed_to_rgb(index);
        assert_eq!(rgb_to_indexed(r, g, b), index, "index {index}");
    }
}

#[test]
fn greys_pick_whichever_of_the_ramp_and_the_cube_is_closer() {
    // 0x80 is exactly on the grey ramp and seven off the cube.
    assert_eq!(rgb_to_indexed(0x80, 0x80, 0x80), 244);
    // 95 is exactly on the cube diagonal and three off the ramp.
    assert_eq!(rgb_to_indexed(95, 95, 95), 59);
}

#[test]
fn open_and_close_can_wrap_borrowed_text_without_allocation() {
    let style = Style::new().fg(Color::Yellow);
    let mut out = String::new();
    style.open(ColorLevel::Ansi16, &mut out);
    out.push_str("body");
    style.close(ColorLevel::Ansi16, &mut out);

    assert_eq!(out, "\x1b[33mbody\x1b[0m");
}

#[test]
fn styles_report_emptiness() {
    assert!(Style::new().is_empty());
    assert!(!Style::new().dim().is_empty());
    assert!(!Style::new().fg(Color::Red).is_empty());
    assert!(Attributes::none().is_empty());
}
