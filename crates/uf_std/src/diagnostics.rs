//! Terminal styling and debug events.
//!
//! The colour escapes are applied only when the caller says colour is enabled,
//! so a std module never has to inspect the environment itself to stay
//! pipe-safe.

use compact_str::{CompactString, ToCompactString};
use serde::{Deserialize, Serialize};

/// ANSI style used by `@uniflowed/std/colors`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AnsiStyle {
    /// Bold text.
    Bold,
    /// Dim text.
    Dim,
    /// Red foreground.
    Red,
    /// Green foreground.
    Green,
    /// Cyan foreground.
    Cyan,
}

/// Apply an ANSI style to a string.
pub fn colorize(value: &str, style: AnsiStyle, enabled: bool) -> CompactString {
    if !enabled {
        return value.to_compact_string();
    }

    let code = match style {
        AnsiStyle::Bold => "1",
        AnsiStyle::Dim => "2",
        AnsiStyle::Red => "31",
        AnsiStyle::Green => "32",
        AnsiStyle::Cyan => "36",
    };
    let mut output = CompactString::new("");
    output.push_str("\x1b[");
    output.push_str(code);
    output.push('m');
    output.push_str(value);
    output.push_str("\x1b[0m");
    output
}

/// Debug event emitted by `@uniflowed/std/debug`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugEvent {
    /// Debug channel.
    pub channel: CompactString,
    /// Message payload.
    pub message: CompactString,
}

/// Create a debug event.
pub fn debug_event(channel: &str, message: &str) -> DebugEvent {
    DebugEvent {
        channel: channel.to_compact_string(),
        message: message.to_compact_string(),
    }
}
