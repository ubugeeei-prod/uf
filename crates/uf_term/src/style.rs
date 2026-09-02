//! A native SGR layer: colours, attributes, and the downgrade rules between
//! palettes.
//!
//! Every style renders through a [`ColorLevel`]. At [`ColorLevel::Never`] a
//! style writes **nothing at all** — not an empty escape, not a reset — so
//! redirecting output to a file yields clean plain text.

use crate::capability::ColorLevel;
use crate::text::push_u32;

/// A colour in one of the three ANSI palettes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    /// ANSI 0.
    Black,
    /// ANSI 1.
    Red,
    /// ANSI 2.
    Green,
    /// ANSI 3.
    Yellow,
    /// ANSI 4.
    Blue,
    /// ANSI 5.
    Magenta,
    /// ANSI 6.
    Cyan,
    /// ANSI 7.
    White,
    /// ANSI 8.
    BrightBlack,
    /// ANSI 9.
    BrightRed,
    /// ANSI 10.
    BrightGreen,
    /// ANSI 11.
    BrightYellow,
    /// ANSI 12.
    BrightBlue,
    /// ANSI 13.
    BrightMagenta,
    /// ANSI 14.
    BrightCyan,
    /// ANSI 15.
    BrightWhite,
    /// An index into the 256-colour palette.
    Indexed(u8),
    /// A 24-bit colour.
    Rgb(u8, u8, u8),
}

impl Color {
    /// Reduce the colour to what `level` can express.
    ///
    /// `Rgb` collapses into the 6x6x6 cube or the grey ramp for
    /// [`ColorLevel::Ansi256`], and into the nearest base colour for
    /// [`ColorLevel::Ansi16`].
    pub fn downgrade(self, level: ColorLevel) -> Self {
        match (self, level) {
            (Self::Rgb(r, g, b), ColorLevel::Ansi256) => Self::Indexed(rgb_to_indexed(r, g, b)),
            (Self::Rgb(r, g, b), ColorLevel::Ansi16) => nearest_base(r, g, b),
            (Self::Indexed(index), ColorLevel::Ansi16) => {
                let (r, g, b) = indexed_to_rgb(index);
                nearest_base(r, g, b)
            }
            (color, _) => color,
        }
    }

    /// The base ANSI number 0..16, when this colour is one of them.
    fn base_index(self) -> Option<u8> {
        Some(match self {
            Self::Black => 0,
            Self::Red => 1,
            Self::Green => 2,
            Self::Yellow => 3,
            Self::Blue => 4,
            Self::Magenta => 5,
            Self::Cyan => 6,
            Self::White => 7,
            Self::BrightBlack => 8,
            Self::BrightRed => 9,
            Self::BrightGreen => 10,
            Self::BrightYellow => 11,
            Self::BrightBlue => 12,
            Self::BrightMagenta => 13,
            Self::BrightCyan => 14,
            Self::BrightWhite => 15,
            _ => return None,
        })
    }

    fn write_sgr(self, level: ColorLevel, background: bool, out: &mut String) {
        let color = self.downgrade(level);
        if let Some(index) = color.base_index() {
            let code = match (background, index < 8) {
                (false, true) => 30 + u32::from(index),
                (false, false) => 90 + u32::from(index - 8),
                (true, true) => 40 + u32::from(index),
                (true, false) => 100 + u32::from(index - 8),
            };
            push_u32(out, code);
            return;
        }
        match color {
            Self::Indexed(index) => {
                out.push_str(if background { "48;5;" } else { "38;5;" });
                push_u32(out, u32::from(index));
            }
            Self::Rgb(r, g, b) => {
                out.push_str(if background { "48;2;" } else { "38;2;" });
                push_u32(out, u32::from(r));
                out.push(';');
                push_u32(out, u32::from(g));
                out.push(';');
                push_u32(out, u32::from(b));
            }
            _ => unreachable!("base colours are handled above"),
        }
    }
}

/// Text attributes, packed into one byte.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Attributes(u8);

impl Attributes {
    const BOLD: u8 = 1 << 0;
    const DIM: u8 = 1 << 1;
    const ITALIC: u8 = 1 << 2;
    const UNDERLINE: u8 = 1 << 3;

    /// No attributes.
    pub const fn none() -> Self {
        Self(0)
    }

    /// Whether no attribute is set.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether bold is set.
    pub const fn is_bold(self) -> bool {
        self.0 & Self::BOLD != 0
    }

    /// Whether dim is set.
    pub const fn is_dim(self) -> bool {
        self.0 & Self::DIM != 0
    }

    /// Whether italic is set.
    pub const fn is_italic(self) -> bool {
        self.0 & Self::ITALIC != 0
    }

    /// Whether underline is set.
    pub const fn is_underline(self) -> bool {
        self.0 & Self::UNDERLINE != 0
    }
}

/// A text style: foreground, background, and attributes.
///
/// Styles are `Copy` and every builder is `const`, so a theme is a set of
/// compile-time constants rather than a runtime allocation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Style {
    fg: Option<Color>,
    bg: Option<Color>,
    attributes: Attributes,
}

impl Style {
    /// A style that renders nothing.
    pub const fn new() -> Self {
        Self {
            fg: None,
            bg: None,
            attributes: Attributes::none(),
        }
    }

    /// Set the foreground colour.
    pub const fn fg(mut self, color: Color) -> Self {
        self.fg = Some(color);
        self
    }

    /// Set the background colour.
    pub const fn bg(mut self, color: Color) -> Self {
        self.bg = Some(color);
        self
    }

    /// Render bold.
    pub const fn bold(mut self) -> Self {
        self.attributes.0 |= Attributes::BOLD;
        self
    }

    /// Render dim.
    pub const fn dim(mut self) -> Self {
        self.attributes.0 |= Attributes::DIM;
        self
    }

    /// Render italic.
    pub const fn italic(mut self) -> Self {
        self.attributes.0 |= Attributes::ITALIC;
        self
    }

    /// Render underlined.
    pub const fn underline(mut self) -> Self {
        self.attributes.0 |= Attributes::UNDERLINE;
        self
    }

    /// The attributes this style sets.
    pub const fn attributes(self) -> Attributes {
        self.attributes
    }

    /// Whether the style would render nothing even with colour enabled.
    pub const fn is_empty(self) -> bool {
        self.fg.is_none() && self.bg.is_none() && self.attributes.is_empty()
    }

    /// Append the opening escape sequence, or nothing when colour is off.
    pub fn open(self, level: ColorLevel, out: &mut String) {
        if !level.is_enabled() || self.is_empty() {
            return;
        }
        out.push_str("\x1b[");
        let mut first = true;
        let separate = |out: &mut String, first: &mut bool| {
            if *first {
                *first = false;
            } else {
                out.push(';');
            }
        };
        for (set, code) in [
            (self.attributes.is_bold(), 1u32),
            (self.attributes.is_dim(), 2),
            (self.attributes.is_italic(), 3),
            (self.attributes.is_underline(), 4),
        ] {
            if set {
                separate(out, &mut first);
                push_u32(out, code);
            }
        }
        if let Some(color) = self.fg {
            separate(out, &mut first);
            color.write_sgr(level, false, out);
        }
        if let Some(color) = self.bg {
            separate(out, &mut first);
            color.write_sgr(level, true, out);
        }
        out.push('m');
    }

    /// Append the closing escape sequence, or nothing when colour is off.
    pub fn close(self, level: ColorLevel, out: &mut String) {
        if !level.is_enabled() || self.is_empty() {
            return;
        }
        out.push_str("\x1b[0m");
    }

    /// Append `text` wrapped in this style.
    ///
    /// The text is appended to the caller's buffer, so a render loop reuses one
    /// allocation instead of building a `String` per styled span.
    pub fn paint(self, level: ColorLevel, text: &str, out: &mut String) {
        self.open(level, out);
        out.push_str(text);
        self.close(level, out);
    }
}

/// The six levels of the 256-colour cube.
const CUBE_STEPS: [u8; 6] = [0, 95, 135, 175, 215, 255];

/// The classic VGA palette the first sixteen slots render as.
const BASE_RGB: [(u8, u8, u8); 16] = [
    (0, 0, 0),
    (170, 0, 0),
    (0, 170, 0),
    (170, 85, 0),
    (0, 0, 170),
    (170, 0, 170),
    (0, 170, 170),
    (170, 170, 170),
    (85, 85, 85),
    (255, 85, 85),
    (85, 255, 85),
    (255, 255, 85),
    (85, 85, 255),
    (255, 85, 255),
    (85, 255, 255),
    (255, 255, 255),
];

fn cube_index(value: u8) -> u8 {
    let mut best = 0u8;
    let mut best_distance = u8::MAX;
    for (index, step) in CUBE_STEPS.iter().enumerate() {
        let distance = value.abs_diff(*step);
        if distance < best_distance {
            best_distance = distance;
            best = index as u8;
        }
    }
    best
}

/// The grey-ramp slot closest to `value`, ignoring the cube.
fn grey_index(value: u8) -> u8 {
    let slot = (i32::from(value) - 8 + 5).clamp(0, 239) / 10;
    232 + slot as u8
}

fn rgb_to_indexed(r: u8, g: u8, b: u8) -> u8 {
    let cube = 16 + 36 * cube_index(r) + 6 * cube_index(g) + cube_index(b);
    if r == g && g == b {
        // Both the grey ramp and the cube's diagonal can express a grey; pick
        // whichever lands closer so the mapping stays reversible.
        let grey = grey_index(r);
        let grey_value = 8 + (grey - 232) * 10;
        let cube_value = CUBE_STEPS[usize::from(cube_index(r))];
        if r.abs_diff(grey_value) < r.abs_diff(cube_value) {
            return grey;
        }
    }
    cube
}

fn indexed_to_rgb(index: u8) -> (u8, u8, u8) {
    match index {
        0..=15 => BASE_RGB[usize::from(index)],
        16..=231 => {
            let offset = index - 16;
            (
                CUBE_STEPS[usize::from(offset / 36)],
                CUBE_STEPS[usize::from((offset / 6) % 6)],
                CUBE_STEPS[usize::from(offset % 6)],
            )
        }
        232..=255 => {
            let level = 8 + (index - 232) * 10;
            (level, level, level)
        }
    }
}

fn nearest_base(r: u8, g: u8, b: u8) -> Color {
    let max = r.max(g).max(b);
    let bits = u8::from(r > 0x7f) | (u8::from(g > 0x7f) << 1) | (u8::from(b > 0x7f) << 2);
    let bright = max > 0xc0;
    match (bits, bright) {
        (0b000, false) => Color::Black,
        (0b000, true) => Color::BrightBlack,
        (0b001, false) => Color::Red,
        (0b001, true) => Color::BrightRed,
        (0b010, false) => Color::Green,
        (0b010, true) => Color::BrightGreen,
        (0b011, false) => Color::Yellow,
        (0b011, true) => Color::BrightYellow,
        (0b100, false) => Color::Blue,
        (0b100, true) => Color::BrightBlue,
        (0b101, false) => Color::Magenta,
        (0b101, true) => Color::BrightMagenta,
        (0b110, false) => Color::Cyan,
        (0b110, true) => Color::BrightCyan,
        (_, false) => Color::White,
        (_, true) => Color::BrightWhite,
    }
}

#[cfg(test)]
mod tests;
