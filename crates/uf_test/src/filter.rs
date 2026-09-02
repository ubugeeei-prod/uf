//! Choosing which of the discovered tests a run executes.
//!
//! Both filters are **substring** matches, not regular expressions. A pattern
//! comes straight from the command line and is applied once per test name in a
//! suite that may hold hundreds of thousands of them; a backtracking regex
//! engine there is a denial-of-service primitive (the CVE class is ReDoS), and
//! substring matching answers the question developers actually ask of `-t`.
//! Matching is linear in the pattern and the subject, with no backtracking.

use compact_str::CompactString;
use smallvec::SmallVec;

/// Inline list of path patterns; a command line rarely carries more than a few.
pub type PathPatternList = SmallVec<[CompactString; 2]>;

/// Longest pattern accepted, so a pathological command line cannot be used to
/// allocate without bound.
pub const MAX_PATTERN_BYTES: usize = 512;

/// Which tests a run executes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TestFilter {
    name: Option<CompactString>,
    paths: PathPatternList,
}

impl TestFilter {
    /// A filter that excludes nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Keep only tests whose fully qualified name contains `pattern`.
    ///
    /// An empty pattern is ignored rather than matching nothing, so
    /// `uf test -t ""` runs the suite instead of silently reporting zero tests.
    pub fn with_name(mut self, pattern: &str) -> Self {
        let pattern = clamp_pattern(pattern);
        if !pattern.is_empty() {
            self.name = Some(pattern);
        }
        self
    }

    /// Keep only files whose relative path contains `pattern`.
    ///
    /// Repeating this widens the filter: a file matching any pattern is kept.
    pub fn with_path(mut self, pattern: &str) -> Self {
        let pattern = clamp_pattern(pattern);
        if !pattern.is_empty() {
            self.paths.push(pattern);
        }
        self
    }

    /// Add every pattern in `patterns` as a path filter.
    pub fn with_paths<'a>(mut self, patterns: impl IntoIterator<Item = &'a str>) -> Self {
        for pattern in patterns {
            self = self.with_path(pattern);
        }
        self
    }

    /// Whether this filter excludes nothing at all.
    pub fn is_empty(&self) -> bool {
        self.name.is_none() && self.paths.is_empty()
    }

    /// The name pattern, if one was set.
    pub fn name_pattern(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// The path patterns, in the order they were given.
    pub fn path_patterns(&self) -> &[CompactString] {
        &self.paths
    }

    /// Whether a file is worth opening at all.
    ///
    /// Applied before discovery so an excluded file is never scanned, which is
    /// what makes `uf test src/checkout` fast on a large suite.
    pub fn matches_path(&self, path: &str) -> bool {
        self.paths.is_empty()
            || self
                .paths
                .iter()
                .any(|pattern| path.contains(pattern.as_str()))
    }

    /// Whether a fully qualified test name survives the name filter.
    pub fn matches_name(&self, full_name: &str) -> bool {
        match &self.name {
            None => true,
            Some(pattern) => full_name.contains(pattern.as_str()),
        }
    }
}

/// Bound a user-supplied pattern, trimming surrounding whitespace.
fn clamp_pattern(pattern: &str) -> CompactString {
    let pattern = pattern.trim();
    if pattern.len() <= MAX_PATTERN_BYTES {
        return CompactString::from(pattern);
    }
    let mut end = MAX_PATTERN_BYTES;
    while end > 0 && !pattern.is_char_boundary(end) {
        end -= 1;
    }
    CompactString::from(&pattern[..end])
}
