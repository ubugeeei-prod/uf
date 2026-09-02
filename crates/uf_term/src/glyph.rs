//! The two glyph vocabularies, and the status marks built from them.
//!
//! There are no emoji here. Emoji are two cells wide in some terminals and one
//! in others, they are unreadable in a CI log, and they carry no information a
//! colour and a one-character mark do not. Every glyph below is one cell wide
//! in both vocabularies, so a column of them always lines up.

use crate::capability::GlyphSet;

/// The box-drawing and marker characters for one glyph vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glyphs {
    /// Horizontal rule segment.
    pub horizontal: char,
    /// Vertical bar, used by the code frame gutter and the tree.
    pub vertical: char,
    /// Tree branch that has following siblings.
    pub branch: &'static str,
    /// Tree branch for the last child.
    pub last_branch: &'static str,
    /// Indent under a branch that has following siblings.
    pub trunk: &'static str,
    /// Indent under the last child.
    pub gap: &'static str,
    /// The arrow on a diagnostic location line.
    pub arrow: &'static str,
    /// The caret under a diagnostic span.
    pub caret: char,
    /// Leader dots between a label and a right-aligned value.
    pub leader: char,
    /// Marker for text elided to keep a line inside the terminal.
    pub ellipsis: &'static str,
}

/// The Unicode vocabulary.
pub const UNICODE_GLYPHS: Glyphs = Glyphs {
    horizontal: '─',
    vertical: '│',
    branch: "├─ ",
    last_branch: "└─ ",
    trunk: "│  ",
    gap: "   ",
    arrow: "-->",
    caret: '^',
    leader: '·',
    ellipsis: "…",
};

/// The ASCII vocabulary, used when the terminal or locale cannot be trusted.
pub const ASCII_GLYPHS: Glyphs = Glyphs {
    horizontal: '-',
    vertical: '|',
    branch: "|- ",
    last_branch: "`- ",
    trunk: "|  ",
    gap: "   ",
    arrow: "-->",
    caret: '^',
    leader: '.',
    ellipsis: "...",
};

impl Glyphs {
    /// The vocabulary for a glyph set.
    pub const fn of(set: GlyphSet) -> Self {
        match set {
            GlyphSet::Unicode => UNICODE_GLYPHS,
            GlyphSet::Ascii => ASCII_GLYPHS,
        }
    }
}

/// The outcome a status line reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The step finished and produced what it promised.
    Success,
    /// The step finished, but something deserves attention.
    Warn,
    /// The step failed.
    Error,
    /// Neutral information.
    Info,
    /// The step was deliberately not run.
    Skip,
}

impl Status {
    /// The one-cell mark for this status.
    pub const fn glyph(self, set: GlyphSet) -> &'static str {
        match (self, set) {
            (Self::Success, GlyphSet::Unicode) => "✓",
            (Self::Success, GlyphSet::Ascii) => "+",
            (Self::Warn, _) => "!",
            (Self::Error, GlyphSet::Unicode) => "✗",
            (Self::Error, GlyphSet::Ascii) => "x",
            (Self::Info, GlyphSet::Unicode) => "›",
            (Self::Info, GlyphSet::Ascii) => ">",
            (Self::Skip, GlyphSet::Unicode) => "·",
            (Self::Skip, GlyphSet::Ascii) => ".",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::display_width;

    #[test]
    fn glyph_sets_map_to_their_vocabulary() {
        assert_eq!(Glyphs::of(GlyphSet::Unicode), UNICODE_GLYPHS);
        assert_eq!(Glyphs::of(GlyphSet::Ascii), ASCII_GLYPHS);
    }

    #[test]
    fn ascii_glyphs_are_pure_ascii() {
        let glyphs = ASCII_GLYPHS;
        assert!(glyphs.horizontal.is_ascii());
        assert!(glyphs.vertical.is_ascii());
        assert!(glyphs.caret.is_ascii());
        assert!(glyphs.leader.is_ascii());
        for text in [
            glyphs.branch,
            glyphs.last_branch,
            glyphs.trunk,
            glyphs.gap,
            glyphs.arrow,
            glyphs.ellipsis,
        ] {
            assert!(text.is_ascii(), "{text:?} is not ASCII");
        }
        for status in [
            Status::Success,
            Status::Warn,
            Status::Error,
            Status::Info,
            Status::Skip,
        ] {
            assert!(status.glyph(GlyphSet::Ascii).is_ascii());
        }
    }

    #[test]
    fn every_status_mark_is_one_cell_wide() {
        for set in [GlyphSet::Unicode, GlyphSet::Ascii] {
            for status in [
                Status::Success,
                Status::Warn,
                Status::Error,
                Status::Info,
                Status::Skip,
            ] {
                assert_eq!(display_width(status.glyph(set)), 1, "{status:?} {set:?}");
            }
        }
    }

    #[test]
    fn tree_branches_are_the_same_width_in_both_vocabularies() {
        for set in [GlyphSet::Unicode, GlyphSet::Ascii] {
            let glyphs = Glyphs::of(set);
            assert_eq!(display_width(glyphs.branch), 3);
            assert_eq!(display_width(glyphs.last_branch), 3);
            assert_eq!(display_width(glyphs.trunk), 3);
            assert_eq!(display_width(glyphs.gap), 3);
        }
    }

    #[test]
    fn no_glyph_is_an_emoji() {
        for set in [GlyphSet::Unicode, GlyphSet::Ascii] {
            let glyphs = Glyphs::of(set);
            assert_eq!(display_width(&glyphs.horizontal.to_string()), 1);
            assert_eq!(display_width(&glyphs.vertical.to_string()), 1);
            assert_eq!(display_width(&glyphs.leader.to_string()), 1);
        }
    }
}
