//! Source Map v3 generation for emitted chunks.
//!
//! Erasure and rewriting in `uf` are both line-preserving: [`uf_flow::strip_types`]
//! blanks types in place, and the bundler blanks import and export statements
//! the same way. A module's line *n* is therefore still a module's line *n*
//! after transformation, and a chunk only needs to record which module each of
//! its own lines came from. That makes the map a per-line table rather than a
//! per-token one — smaller to build, smaller to ship, and exact at the
//! granularity a stack trace uses.

use serde::Serialize;

/// Which module, and which of its lines, one output line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineOrigin {
    /// Index into the map's `sources` list.
    pub source: u32,
    /// Zero-based line in that source.
    pub line: u32,
}

/// Collects output lines and where each came from.
#[derive(Debug, Default, Clone)]
pub struct SourceMapBuilder {
    sources: Vec<String>,
    sources_content: Vec<String>,
    lines: Vec<Option<LineOrigin>>,
}

impl SourceMapBuilder {
    /// A builder with no sources and no lines.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a source file, returning its index.
    pub fn add_source(&mut self, name: impl Into<String>, content: impl Into<String>) -> u32 {
        self.sources.push(name.into());
        self.sources_content.push(content.into());
        (self.sources.len() - 1) as u32
    }

    /// Record one output line that came from nowhere in particular.
    pub fn generated_line(&mut self) {
        self.lines.push(None);
    }

    /// Record one output line that came from `origin`.
    pub fn mapped_line(&mut self, origin: LineOrigin) {
        self.lines.push(Some(origin));
    }

    /// How many output lines have been recorded.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Render the map as the JSON a browser expects.
    #[must_use]
    pub fn finish(self, file: &str) -> String {
        let map = SourceMap {
            version: 3,
            file,
            sources: &self.sources,
            sources_content: &self.sources_content,
            names: &[],
            mappings: encode_mappings(&self.lines),
        };
        serde_json::to_string(&map).unwrap_or_else(|_| String::from("{\"version\":3}"))
    }
}

/// The on-disk shape of a `.map` file.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceMap<'a> {
    version: u8,
    file: &'a str,
    sources: &'a [String],
    sources_content: &'a [String],
    names: &'a [&'a str],
    mappings: String,
}

/// Encode one segment per mapped output line.
///
/// Every field of a segment except the generated column is relative to the
/// previous segment, and the generated column resets at every line, which is
/// why the encoder carries `previous_*` across lines but not the column.
fn encode_mappings(lines: &[Option<LineOrigin>]) -> String {
    let mut out = String::with_capacity(lines.len() * 6);
    let mut previous_source: i64 = 0;
    let mut previous_line: i64 = 0;

    for (index, origin) in lines.iter().enumerate() {
        if index > 0 {
            out.push(';');
        }
        let Some(origin) = origin else {
            continue;
        };
        let source = i64::from(origin.source);
        let line = i64::from(origin.line);
        encode_vlq(&mut out, 0);
        encode_vlq(&mut out, source - previous_source);
        encode_vlq(&mut out, line - previous_line);
        encode_vlq(&mut out, 0);
        previous_source = source;
        previous_line = line;
    }

    out
}

/// Base64 alphabet used by Source Map v3, in its canonical order.
const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Append one base64 variable-length quantity.
fn encode_vlq(out: &mut String, value: i64) {
    // The sign travels in the low bit, which is why the magnitude is shifted up
    // by one rather than encoded as two's complement.
    let mut bits = if value < 0 {
        ((-value) as u64) << 1 | 1
    } else {
        (value as u64) << 1
    };

    loop {
        let mut digit = (bits & 0b1_1111) as usize;
        bits >>= 5;
        if bits > 0 {
            digit |= 0b10_0000;
        }
        out.push(BASE64[digit] as char);
        if bits == 0 {
            return;
        }
    }
}
