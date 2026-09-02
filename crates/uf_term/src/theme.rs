//! The named styles every renderer draws from.
//!
//! Accents are declared as 24-bit colours and downgraded by [`Style`] to the
//! 256-colour cube or the sixteen base colours, so one theme definition serves
//! every terminal instead of being written three times.

use crate::style::{Color, Style};

/// A semantic role a piece of text plays, resolved to a [`Style`] by a
/// [`Theme`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tone {
    /// Body text.
    #[default]
    Plain,
    /// Secondary text that should recede.
    Muted,
    /// A file system path.
    Path,
    /// A count or a measurement.
    Number,
    /// A good outcome.
    Good,
    /// Something that needs attention.
    Warn,
    /// A failure.
    Bad,
    /// A highlighted value.
    Accent,
    /// A heading.
    Title,
}

/// The style palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    /// Command banners and section headings.
    pub title: Style,
    /// The text beside a banner title.
    pub subtitle: Style,
    /// Secondary text.
    pub muted: Style,
    /// Keys in a key/value block.
    pub key: Style,
    /// Values in a key/value block.
    pub value: Style,
    /// File system paths.
    pub path: Style,
    /// Counts and measurements.
    pub number: Style,
    /// Highlighted values.
    pub accent: Style,
    /// Success marks and text.
    pub success: Style,
    /// Warning marks and text.
    pub warning: Style,
    /// Error marks and text.
    pub error: Style,
    /// Informational marks and text.
    pub info: Style,
    /// Rules, dividers, and leader dots.
    pub rule: Style,
    /// The line-number gutter of a code frame.
    pub gutter: Style,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            title: Style::new().bold(),
            subtitle: Style::new().fg(Color::BrightBlack),
            muted: Style::new().fg(Color::BrightBlack),
            key: Style::new().fg(Color::BrightBlack),
            value: Style::new(),
            path: Style::new().fg(Color::Cyan),
            number: Style::new().fg(Color::Rgb(0x5f, 0xaf, 0xff)),
            accent: Style::new().fg(Color::Rgb(0x5f, 0xaf, 0xff)),
            success: Style::new().fg(Color::Green),
            warning: Style::new().fg(Color::Yellow),
            error: Style::new().fg(Color::Red),
            info: Style::new().fg(Color::Rgb(0x5f, 0xaf, 0xff)),
            rule: Style::new().fg(Color::BrightBlack),
            gutter: Style::new().fg(Color::BrightBlack),
        }
    }
}

impl Theme {
    /// The style for a semantic role.
    pub fn tone(&self, tone: Tone) -> Style {
        match tone {
            Tone::Plain => self.value,
            Tone::Muted => self.muted,
            Tone::Path => self.path,
            Tone::Number => self.number,
            Tone::Good => self.success,
            Tone::Warn => self.warning,
            Tone::Bad => self.error,
            Tone::Accent => self.accent,
            Tone::Title => self.title,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::ColorLevel;

    #[test]
    fn the_default_theme_is_invisible_without_color() {
        let theme = Theme::default();
        for tone in [
            Tone::Plain,
            Tone::Muted,
            Tone::Path,
            Tone::Number,
            Tone::Good,
            Tone::Warn,
            Tone::Bad,
            Tone::Accent,
            Tone::Title,
        ] {
            let mut out = String::new();
            theme.tone(tone).paint(ColorLevel::Never, "text", &mut out);
            assert_eq!(out, "text", "{tone:?}");
        }
    }

    #[test]
    fn accents_downgrade_across_palettes() {
        let theme = Theme::default();
        for level in [
            ColorLevel::Ansi16,
            ColorLevel::Ansi256,
            ColorLevel::TrueColor,
        ] {
            let mut out = String::new();
            theme.accent.paint(level, "text", &mut out);
            assert!(out.starts_with("\x1b["), "{level:?}");
            assert!(out.ends_with("\x1b[0m"), "{level:?}");
        }
    }

    #[test]
    fn plain_values_carry_no_style_of_their_own() {
        assert!(Theme::default().value.is_empty());
    }

    #[test]
    fn the_default_tone_is_plain() {
        assert_eq!(Tone::default(), Tone::Plain);
    }
}
