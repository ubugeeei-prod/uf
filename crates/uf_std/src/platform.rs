//! Host platform surfaces: OS, streams, URLs, WebAssembly, motion and the TUI.
//!
//! Everything a std module needs to know about the machine it is running on,
//! plus the descriptors for the host-facing primitives that are not a file
//! system or a network socket.

use compact_str::{CompactString, ToCompactString};
use serde::{Deserialize, Serialize};

use super::fs::join_path;

/// Host OS family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OsFamily {
    /// macOS.
    MacOs,
    /// Linux.
    Linux,
    /// Windows.
    Windows,
    /// Unknown or unsupported OS.
    Unknown,
}

/// Host OS descriptor for `@uniflowed/std/os`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OsInfo {
    /// OS family.
    pub family: OsFamily,
    /// CPU architecture.
    pub arch: CompactString,
    /// Available parallelism.
    pub available_parallelism: usize,
}

impl OsInfo {
    /// Create an OS descriptor.
    pub fn new(family: OsFamily, arch: &str, available_parallelism: usize) -> Self {
        Self {
            family,
            arch: arch.to_compact_string(),
            available_parallelism,
        }
    }
}

/// Stream direction for WinterTC-compatible stream wrappers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StreamKind {
    /// Readable stream.
    Readable,
    /// Writable stream.
    Writable,
    /// Transform stream.
    Transform,
}

/// Lightweight stream descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamDescriptor {
    /// Stream kind.
    pub kind: StreamKind,
    /// Whether backpressure is part of the contract.
    pub backpressure: bool,
}

impl StreamDescriptor {
    /// Create a stream descriptor.
    pub fn new(kind: StreamKind) -> Self {
        Self {
            kind,
            backpressure: true,
        }
    }
}

/// Parsed URL descriptor for typed wrappers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UrlParts {
    /// URL scheme without the trailing colon.
    pub scheme: CompactString,
    /// Host component.
    pub host: CompactString,
    /// Path component.
    pub path: CompactString,
}

/// Parse a simple absolute URL without allocating a full URL object graph.
pub fn parse_url(value: &str) -> Option<UrlParts> {
    let (scheme, rest) = value.split_once("://")?;
    let (host, path) = match rest.split_once('/') {
        Some((host, path)) => (host, path),
        None => (rest, ""),
    };
    Some(UrlParts {
        scheme: scheme.to_compact_string(),
        host: host.to_compact_string(),
        path: join_path(&[path]),
    })
}

/// WebAssembly module descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WasmModulePlan {
    /// Module name.
    pub name: CompactString,
    /// Whether the module should be compiled ahead of time.
    pub ahead_of_time: bool,
}

impl WasmModulePlan {
    /// Create a native WebAssembly module plan.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_compact_string(),
            ahead_of_time: true,
        }
    }
}

/// Motion easing curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MotionEase {
    /// Linear easing.
    Linear,
    /// Standard ease-out.
    Out,
    /// Spring-like native easing.
    Spring,
}

/// Motion transition descriptor for `@uniflowed/std/motion`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MotionTransition {
    /// Duration in milliseconds.
    pub duration_ms: u16,
    /// Easing curve.
    pub ease: MotionEase,
    /// Whether reduced motion is respected.
    pub respects_reduced_motion: bool,
}

impl MotionTransition {
    /// Create a transition that respects reduced motion by default.
    pub fn new(duration_ms: u16, ease: MotionEase) -> Self {
        Self {
            duration_ms,
            ease,
            respects_reduced_motion: true,
        }
    }
}

/// Terminal color depth exposed by `@uniflowed/std/tui`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TerminalColorDepth {
    /// 16-color ANSI terminal.
    Ansi16,
    /// 256-color ANSI terminal.
    Ansi256,
    /// 24-bit true color terminal.
    TrueColor,
}

/// Terminal capability descriptor for native TUI rendering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCapabilities {
    /// Terminal columns.
    pub columns: u16,
    /// Terminal rows.
    pub rows: u16,
    /// Supported color depth.
    pub color_depth: TerminalColorDepth,
    /// Whether Unicode graphemes are supported.
    pub unicode: bool,
    /// Whether mouse input is supported.
    pub mouse: bool,
    /// Whether inline image protocols are available.
    pub inline_images: bool,
    /// Whether sixel images are available.
    pub sixel: bool,
}

impl TerminalCapabilities {
    /// Create terminal capabilities with true-color Unicode defaults.
    pub fn new(columns: u16, rows: u16) -> Self {
        Self {
            columns,
            rows,
            color_depth: TerminalColorDepth::TrueColor,
            unicode: true,
            mouse: true,
            inline_images: false,
            sixel: false,
        }
    }

    /// Enable inline image protocols.
    pub fn with_inline_images(mut self) -> Self {
        self.inline_images = true;
        self
    }

    /// Return whether high fidelity rendering is available.
    pub fn high_fidelity(&self) -> bool {
        self.color_depth == TerminalColorDepth::TrueColor && self.unicode
    }
}

/// Create a terminal capability descriptor.
pub fn terminal_capabilities(columns: u16, rows: u16) -> TerminalCapabilities {
    TerminalCapabilities::new(columns, rows)
}
