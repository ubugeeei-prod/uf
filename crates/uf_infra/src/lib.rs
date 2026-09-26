#![cfg_attr(test, allow(clippy::disallowed_macros))]

//! Shared high-throughput primitives for uniflowed.
//!
//! This crate is intentionally tiny and boring at the API boundary. Internals can
//! change as benchmarks teach us more, while downstream crates get one stable
//! import path for the fast defaults we want everywhere.
//!
//! [`cache`] is the one module here that touches a disk rather than a buffer.
//! It is here for the same reason the rest is: three crates keep an answer
//! cache under `.uf/cache/`, all three need the same bound on it, and three
//! copies of an eviction policy would be three policies.

pub mod cache;
pub mod parallel;
mod string_builder;
pub use string_builder::into_string;

pub use bumpalo::{Bump, collections::Vec as ArenaVec};
pub use compact_str::{CompactString, format_compact};
pub use fast_float2::parse as parse_float;
pub use memchr::{memchr, memchr_iter};
pub use phf;
pub use rustc_hash::{FxHashMap, FxHashSet, FxHasher};
pub use smallvec::{SmallVec, smallvec};

use simdutf8::basic;

pub type InlineVec<T, const N: usize> = SmallVec<[T; N]>;

pub static FLOW_KEYWORDS: phf::Set<&'static str> = phf::phf_set! {
    "as",
    "component",
    "declare",
    "enum",
    "hook",
    "match",
    "opaque",
    "renders",
    "type",
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineColumn {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineIndex {
    starts: InlineVec<usize, 16>,
}

impl LineIndex {
    pub fn new(source: &str) -> Self {
        // Short modules stay inline. Large inputs get one exact allocation;
        // SIMD newline counting avoids the old scalar pre-scan and repeated
        // SmallVec growth when thousands of lines spill.
        let mut starts = if source.len() > 1024 {
            InlineVec::with_capacity(memchr_iter(b'\n', source.as_bytes()).count() + 1)
        } else {
            InlineVec::new()
        };
        starts.push(0);
        starts.extend(memchr_iter(b'\n', source.as_bytes()).map(|offset| offset + 1));
        Self { starts }
    }

    pub fn line_col(&self, offset: usize) -> LineColumn {
        let line_index = match self.starts.binary_search(&offset) {
            Ok(index) => index,
            Err(index) => index.saturating_sub(1),
        };
        let start = self.starts.get(line_index).copied().unwrap_or(0);
        LineColumn {
            line: line_index + 1,
            column: offset.saturating_sub(start) + 1,
        }
    }

    pub fn line_count(&self) -> usize {
        self.starts.len()
    }
}

pub fn validate_utf8(bytes: &[u8]) -> Result<&str, basic::Utf8Error> {
    basic::from_utf8(bytes)
}

pub fn is_flow_keyword(value: &str) -> bool {
    FLOW_KEYWORDS.contains(value)
}

pub fn normalize_slashes(path: &str) -> CompactString {
    // For heap-sized strings, replace's allocation becomes the CompactString's
    // backing buffer directly. It was faster than rebuilding chunk by chunk.
    if path.len() > std::mem::size_of::<CompactString>() {
        return CompactString::from(path.replace('\\', "/"));
    }
    let mut normalized = CompactString::new(path);
    // SAFETY: every byte starts as valid UTF-8. Only ASCII backslash becomes
    // ASCII slash, preserving length and every multibyte sequence. The result
    // remains valid UTF-8 throughout; the borrow cannot escape this function.
    let bytes = unsafe { normalized.as_mut_bytes() };
    for byte in bytes {
        if *byte == b'\\' {
            *byte = b'/';
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_index_maps_offsets_to_one_based_positions() {
        let index = LineIndex::new("one\ntwo\nthree");

        assert_eq!(index.line_col(0), LineColumn { line: 1, column: 1 });
        assert_eq!(index.line_col(4), LineColumn { line: 2, column: 1 });
        assert_eq!(index.line_col(8), LineColumn { line: 3, column: 1 });
        assert_eq!(index.line_count(), 3);
    }

    #[test]
    fn utf8_validation_uses_simd_fast_path() {
        assert_eq!(
            validate_utf8(b"component Foo() renders Bar").unwrap(),
            "component Foo() renders Bar"
        );
        assert!(validate_utf8(&[0xff]).is_err());
    }

    #[test]
    fn keyword_set_knows_new_flow_syntax() {
        assert!(is_flow_keyword("component"));
        assert!(is_flow_keyword("hook"));
        assert!(is_flow_keyword("opaque"));
        assert!(is_flow_keyword("match"));
        assert!(!is_flow_keyword("Component"));
    }

    #[test]
    fn compact_paths_are_normalized() {
        assert_eq!(
            normalize_slashes("src\\app\\index.js").as_str(),
            "src/app/index.js"
        );
    }
}
