//! Process environment: `.env` files and `import.meta`.
//!
//! The dotenv reader is deliberately not a shell: it splits on the first `=`
//! and strips one layer of matching quotes, so nothing in a `.env` file can be
//! expanded, substituted or executed.

use compact_str::{CompactString, ToCompactString};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// Dotenv key-value pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DotEnvPair {
    /// Environment variable name.
    pub key: CompactString,
    /// Environment variable value.
    pub value: CompactString,
}

/// Runtime-safe `import.meta` descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportMeta {
    /// Module URL.
    pub url: CompactString,
    /// Directory name, when available for the host.
    pub dirname: Option<CompactString>,
    /// File name, when available for the host.
    pub filename: Option<CompactString>,
}

impl ImportMeta {
    /// Create an import meta descriptor.
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_compact_string(),
            dirname: None,
            filename: None,
        }
    }

    /// Attach directory and filename fields.
    pub fn with_file(mut self, dirname: &str, filename: &str) -> Self {
        self.dirname = Some(dirname.to_compact_string());
        self.filename = Some(filename.to_compact_string());
        self
    }
}

/// Inline dotenv pair list.
pub type DotEnvPairs = SmallVec<[DotEnvPair; 16]>;

/// Parse simple `.env` files without executing shell syntax.
pub fn parse_dotenv(source: &str) -> DotEnvPairs {
    let mut pairs = DotEnvPairs::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        pairs.push(DotEnvPair {
            key: key.trim().to_compact_string(),
            value: trim_env_value(value.trim()),
        });
    }
    pairs
}

fn trim_env_value(value: &str) -> CompactString {
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        let quoted = (bytes[0] == b'"' && bytes[value.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[value.len() - 1] == b'\'');
        if quoted {
            return value[1..value.len() - 1].to_compact_string();
        }
    }
    value.to_compact_string()
}
