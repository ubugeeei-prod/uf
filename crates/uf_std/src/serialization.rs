//! Query strings, JSON, TOML and YAML document contracts.
//!
//! Percent-decoding is done byte-wise and never panics on a truncated or
//! non-hexadecimal escape, because query strings arrive from the network.

use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use super::data::hex_value;

/// Inline query pair list used by `@uniflowed/std/qs`.
pub type QueryPairs = SmallVec<[QueryPair; 8]>;

/// Query string key-value pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryPair {
    /// Query key.
    pub key: CompactString,
    /// Query value.
    pub value: CompactString,
}

/// Parse a query string into ordered key-value pairs.
pub fn parse_query(query: &str) -> QueryPairs {
    let input = query.strip_prefix('?').unwrap_or(query);
    let mut pairs = QueryPairs::new();
    for part in input.split('&') {
        if part.is_empty() {
            continue;
        }
        let mut split = part.splitn(2, '=');
        let key = split.next().unwrap_or("");
        let value = split.next().unwrap_or("");
        pairs.push(QueryPair {
            key: percent_decode(key),
            value: percent_decode(value),
        });
    }
    pairs
}

/// Stringify ordered key-value query pairs.
pub fn stringify_query(pairs: &[QueryPair]) -> CompactString {
    let mut output = CompactString::new("");
    for (index, pair) in pairs.iter().enumerate() {
        if index > 0 {
            output.push('&');
        }
        percent_encode(pair.key.as_str(), &mut output);
        output.push('=');
        percent_encode(pair.value.as_str(), &mut output);
    }
    output
}

fn percent_decode(value: &str) -> CompactString {
    let bytes = value.as_bytes();
    let mut output = CompactString::new("");
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                output.push(' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hi = hex_value(bytes[index + 1]);
                let lo = hex_value(bytes[index + 2]);
                match (hi, lo) {
                    (Some(hi), Some(lo)) => {
                        output.push((hi << 4 | lo) as char);
                        index += 3;
                    }
                    _ => {
                        output.push('%');
                        index += 1;
                    }
                }
            }
            byte => {
                output.push(byte as char);
                index += 1;
            }
        }
    }
    output
}

fn percent_encode(value: &str, output: &mut CompactString) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            output.push(byte as char);
        } else if byte == b' ' {
            output.push('+');
        } else {
            output.push('%');
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
}

/// Parsed JSON document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonDocument {
    /// Parsed JSON value.
    pub value: serde_json::Value,
}

/// Parse JSON through serde's native engine.
pub fn parse_json(source: &str) -> Result<JsonDocument, serde_json::Error> {
    serde_json::from_str(source).map(|value| JsonDocument { value })
}

/// Minify JSON source.
pub fn minify_json(source: &str) -> Result<String, serde_json::Error> {
    serde_json::from_str::<serde_json::Value>(source)
        .and_then(|value| serde_json::to_string(&value))
}

/// Parsed TOML document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TomlDocument {
    /// Parsed TOML value.
    pub value: toml::Value,
}

/// Parse TOML through the native Rust TOML parser.
pub fn parse_toml(source: &str) -> Result<TomlDocument, toml::de::Error> {
    toml::from_str(source).map(|value| TomlDocument { value })
}

/// Coarse YAML document kind for fast dispatch before full parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum YamlDocumentKind {
    /// YAML mapping document.
    Mapping,
    /// YAML sequence document.
    Sequence,
    /// YAML scalar document.
    Scalar,
    /// Empty YAML document.
    Empty,
}

/// Detect a YAML document's coarse shape without allocating a parser tree.
pub fn detect_yaml(source: &str) -> YamlDocumentKind {
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with("- ") {
            return YamlDocumentKind::Sequence;
        }
        if trimmed.contains(':') {
            return YamlDocumentKind::Mapping;
        }
        return YamlDocumentKind::Scalar;
    }
    YamlDocumentKind::Empty
}
