//! The spacing rule every renderer follows: **a rule is never followed by a
//! blank line.**
//!
//! A banner is its title and the rule under it, and the rule already separates
//! the banner from what follows. A blank line under it as well puts two
//! separators in a row and pushes the report one line further down for
//! nothing. Every command used to add one, each at its own call site. So the
//! rule is enforced here, on the text, instead of trusting sixty call sites
//! never to add it back.
//!
//! [`RuleSpacing`] sits between a renderer and its stream and drops any blank
//! lines that directly follow a rule line. It carries one bit of state between
//! writes, because a banner is often rendered by itself and the report under
//! it by a later write that starts with a blank line.
//!
//! A *rule line* is a line with no indentation, made only of the rule glyph
//! (`─`, or `-` in ASCII), and at least four columns wide, which is the
//! narrowest rule [`crate::Renderer::banner`] draws. Escape sequences do not
//! count toward any of that. Indented text is never a rule, so an indented
//! `----` line inside a unified diff is left alone.

use crate::text::display_width;

/// The narrowest rule a banner draws, and so the shortest line read as one.
const MIN_RULE: usize = 4;

/// Whether `line` (without its newline) is a horizontal rule.
pub fn is_rule_line(line: &str) -> bool {
    // Nearly every line is text, and text starts with neither a rule glyph
    // nor an escape; this is what keeps a report of thousands of lines from
    // paying for the check.
    if !(line.starts_with('─') || line.starts_with('-') || line.starts_with('\x1b')) {
        return false;
    }
    let visible = strip_escapes(line);
    let Some(first) = visible.chars().next() else {
        return false;
    };
    (first == '─' || first == '-')
        && visible.chars().all(|ch| ch == first)
        && display_width(&visible) >= MIN_RULE
}

/// The first line of `text` that is blank and directly follows a rule, if
/// there is one, as a zero-based line number.
///
/// For tests: every rendered transcript can be checked against the rule with
/// one assertion.
pub fn blank_after_rule(text: &str) -> Option<usize> {
    let mut previous_was_rule = false;
    for (index, line) in text.lines().enumerate() {
        if previous_was_rule && is_blank_line(line) {
            return Some(index);
        }
        previous_was_rule = is_rule_line(line);
    }
    None
}

/// Whether `line` shows nothing: empty, or only whitespace and escapes.
fn is_blank_line(line: &str) -> bool {
    strip_escapes(line).trim().is_empty()
}

/// `text` with every escape sequence removed.
fn strip_escapes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\x1b' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            // CSI: parameters, then a final byte in `@`..=`~`.
            Some('[') => {
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
            // OSC: runs to BEL or to ST (`ESC \`).
            Some(']') => {
                while let Some(next) = chars.next() {
                    if next == '\x07' {
                        break;
                    }
                    if next == '\x1b' {
                        let _ = chars.next();
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Drops the blank lines that follow a rule, across any number of writes to
/// one stream.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RuleSpacing {
    /// Whether the last complete line written was a rule, and blank lines
    /// should be dropped until something else is written.
    after_rule: bool,
}

impl RuleSpacing {
    /// A stream nothing has been written to.
    pub const fn new() -> Self {
        Self { after_rule: false }
    }

    /// Append `text` to `out` without the blank lines that follow a rule.
    ///
    /// A write that ends partway through a line clears the state: whatever
    /// comes next continues that line, so it cannot be a blank line under a
    /// rule.
    pub fn apply(&mut self, text: &str, out: &mut String) {
        for line in text.split_inclusive('\n') {
            let complete = line.ends_with('\n');
            let content = line.strip_suffix('\n').unwrap_or(line);
            if complete && self.after_rule && is_blank_line(content) {
                continue;
            }
            out.push_str(line);
            self.after_rule = complete && is_rule_line(content);
        }
    }

    /// Record that something was written to the stream without passing
    /// through [`RuleSpacing::apply`]: raw output that the rule does not
    /// police, but that still separates a rule from what follows it.
    pub fn interrupt(&mut self) {
        self.after_rule = false;
    }

    /// `text` without the blank lines that follow a rule, from a fresh
    /// stream.
    pub fn tighten(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        Self::new().apply(text, &mut out);
        out
    }
}

#[cfg(test)]
mod tests;
