//! Coverage: what the suite executed, counted in the author's own lines.
//!
//! # Why V8's own counters, and not instrumentation
//!
//! Every module `uf test` runs has been through `uf transform`: Flow in,
//! JavaScript out, with a `component` lowered to a function, a `match` lowered
//! to a chain of tests, and — in development — the React Compiler's memoisation
//! inserted. Instrumenting the *source* would mean writing counters into Flow
//! and compiling them, which changes the program the suite is supposed to be
//! testing; instrumenting the *output* would mean counting constructs the
//! author never wrote.
//!
//! So nothing is instrumented. V8 counts execution itself, Node writes those
//! counts out when a worker exits (`NODE_V8_COVERAGE`), and this module maps
//! them back through the transform's source map. Three properties of the
//! existing design made that the cheap answer rather than the clever one:
//!
//! * The Node host is already started with `--enable-source-maps`
//!   ([`crate::HostCommand::with_flow_loader`]), which is what makes Node keep
//!   a source-map cache — and Node writes that cache into the coverage document
//!   under `source-map-cache`, with the generated file's line lengths beside it.
//!   Everything the mapping needs is in the one file Node already writes.
//! * The loader appends an inline source map to every module it transforms
//!   (`packages/host/internal/node-hooks.js`), so every module of the project's
//!   own is mapped, and mapped by the same `uf` that compiled it.
//! * The worker is a process that ends by closing its stdin
//!   (`packages/test/worker.js`), so there is an exit for Node to flush at.
//!
//! The cost is stated rather than hidden: this is Node-only. Bun's preload
//! transforms with `sourceMap: false` and Bun does not implement
//! `NODE_V8_COVERAGE`; Deno's ahead-of-time loader has no equivalent coverage
//! flush or source-map cache to read back. `uf test --coverage` on either says
//! so instead of reporting zeroes.
//!
//! # What is counted, exactly
//!
//! The one rule everything else follows: **a generated position that maps to
//! nothing in the author's source is not counted at all.** The printer records
//! a mapping for every node the author wrote and none for the nodes the
//! lowering passes invent (`docs/architecture.md`), so that rule is what keeps
//! a `match` lowering's `typeof` guards, a `component`'s destructuring preamble
//! and the React Compiler's memo blocks out of the numbers. They are not
//! reported as covered and they are not reported as missed; they are not
//! branches of the author's program, so they are not in the denominator.
//!
//! * **Line** — an author line is *counted* when at least one generated
//!   position maps back to it, and *covered* when the innermost V8 range over
//!   one of those positions ran at least once. A line's hit count is the
//!   largest count of any position on it. This is why a `type` alias, a
//!   comment and a blank line are absent from the report rather than missed:
//!   they generate nothing, so nothing maps to them.
//! * **Function** — one per V8 function whose body contains at least one mapped
//!   position, attributed to the *first* such position. Two generated functions
//!   that map to one author position are one function. The script's top-level
//!   wrapper is not a function.
//! * **Branch** — one per V8 *block* range (the arms of an `if` or a ternary,
//!   the right operand of `&&`, a loop body, a `catch`) whose first mapped
//!   position exists, attributed to that position. A block is taken when its
//!   count is above zero.
//!
//! # Merging
//!
//! A run is several worker processes, each writing its own document, and one
//! source file is usually loaded by several of them. Every count in this module
//! is keyed by a position *in the author's source* — never by a generated
//! offset, a script id or a file's place in a list — so merging is addition
//! over a map with a total order on its keys. Two workers that both ran
//! `src/math.js` sum their counts; a worker that never loaded it contributes
//! nothing to it rather than a zero. That is the property [`Coverage::merge`]
//! is tested for: no double count, no lost file.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

use camino::{Utf8Path, Utf8PathBuf};
use oxc_sourcemap::SourceMap;
use serde::{Deserialize, Serialize};

/// Largest coverage document that will be read, in bytes.
///
/// A document is written by a process the run started, but a test can make
/// that process load anything, so the size is bounded like every other input
/// this crate reads.
pub const MAX_COVERAGE_DOCUMENT_BYTES: u64 = 256 * 1024 * 1024;

/// Most scripts one document may describe.
pub const MAX_SCRIPTS_PER_DOCUMENT: usize = 50_000;

/// Most V8 ranges one script may contribute.
///
/// The sweep that resolves a position to its innermost range is linear in this;
/// the bound is what stops a generated file from making it unbounded.
pub const MAX_RANGES_PER_SCRIPT: usize = 500_000;

/// Longest function name kept, in bytes.
pub const MAX_FUNCTION_NAME_BYTES: usize = 200;

/// A position in the author's Flow source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    /// One-based line.
    pub line: u32,
    /// Zero-based column.
    pub column: u32,
}

/// One of the three things coverage counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Metric {
    /// Author lines that at least one generated position maps to.
    Lines,
    /// Functions whose body holds at least one mapped position.
    Functions,
    /// V8 block ranges whose first mapped position exists.
    Branches,
}

impl Metric {
    /// The metric's name, as a threshold names it and a report prints it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lines => "lines",
            Self::Functions => "functions",
            Self::Branches => "branches",
        }
    }

    /// Every metric, in report order.
    #[must_use]
    pub const fn all() -> [Self; 3] {
        [Self::Lines, Self::Functions, Self::Branches]
    }
}

/// How much of something was covered.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ratio {
    /// How many were executed at least once.
    pub covered: usize,
    /// How many there were.
    pub total: usize,
}

impl Ratio {
    /// The percentage covered, as a number between zero and a hundred.
    ///
    /// Nothing to cover is a hundred per cent, which is the convention every
    /// reader of an LCOV file already has: a file with no branch in it is not
    /// a file with missing branch coverage. A file the suite never loaded at
    /// all is a different question and is not answered here — see
    /// [`Coverage::never_loaded`].
    #[must_use]
    pub fn percent(self) -> f64 {
        if self.total == 0 {
            return 100.0;
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "a count large enough to lose precision here is larger than any source tree"
        )]
        {
            self.covered as f64 * 100.0 / self.total as f64
        }
    }

    /// Whether this ratio reaches `percent`.
    ///
    /// Compared as integers — `covered * 100 >= percent * total` — so a
    /// threshold of 80 on 4 of 5 lines passes rather than depending on how
    /// `80.0` rounds.
    #[must_use]
    pub fn meets(self, percent: u8) -> bool {
        if self.total == 0 {
            return true;
        }
        self.covered.saturating_mul(100) >= usize::from(percent).saturating_mul(self.total)
    }

    fn add(&mut self, other: Self) {
        self.covered += other.covered;
        self.total += other.total;
    }
}

/// One function the run could have called.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionCoverage {
    /// Where the author declared it.
    pub at: Position,
    /// What V8 called it; empty for one the generated code did not name.
    pub name: String,
    /// How many times it was entered.
    pub hits: u64,
}

/// One branch: a V8 block range that maps into the author's source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchCoverage {
    /// The first author position inside the block.
    pub at: Position,
    /// How many times the block was entered.
    pub hits: u64,
}

/// What one source file's coverage is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileCoverage {
    lines: BTreeMap<u32, u64>,
    functions: BTreeMap<Position, (String, u64)>,
    branches: BTreeMap<Position, u64>,
}

impl FileCoverage {
    /// Hit counts by one-based author line, in line order.
    pub fn lines(&self) -> impl ExactSizeIterator<Item = (u32, u64)> + '_ {
        self.lines.iter().map(|(line, hits)| (*line, *hits))
    }

    /// Every function, in source order.
    pub fn functions(&self) -> impl ExactSizeIterator<Item = FunctionCoverage> + '_ {
        self.functions
            .iter()
            .map(|(at, (name, hits))| FunctionCoverage {
                at: *at,
                name: name.clone(),
                hits: *hits,
            })
    }

    /// Every branch, in source order.
    pub fn branches(&self) -> impl ExactSizeIterator<Item = BranchCoverage> + '_ {
        self.branches.iter().map(|(at, hits)| BranchCoverage {
            at: *at,
            hits: *hits,
        })
    }

    /// This file's three ratios.
    #[must_use]
    pub fn totals(&self) -> Totals {
        Totals {
            lines: ratio(self.lines.values().copied()),
            functions: ratio(self.functions.values().map(|(_, hits)| *hits)),
            branches: ratio(self.branches.values().copied()),
        }
    }

    /// Whether nothing at all was recorded for this file.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty() && self.functions.is_empty() && self.branches.is_empty()
    }

    /// Add `other`'s counts to this file's.
    ///
    /// Counts are summed because two workers that both loaded this file both
    /// executed it; positions are keys, so nothing is counted twice and nothing
    /// only one of them saw is dropped.
    pub fn merge(&mut self, other: &Self) {
        for (line, hits) in &other.lines {
            let slot = self.lines.entry(*line).or_insert(0);
            *slot = slot.saturating_add(*hits);
        }
        for (at, (name, hits)) in &other.functions {
            match self.functions.entry(*at) {
                Entry::Occupied(mut held) => {
                    let held = held.get_mut();
                    held.1 = held.1.saturating_add(*hits);
                    // Two generated functions at one author position can carry
                    // different names; the smaller one wins so that a merge
                    // does not depend on which worker finished first.
                    if name < &held.0 {
                        held.0 = name.clone();
                    }
                }
                Entry::Vacant(free) => {
                    free.insert((name.clone(), *hits));
                }
            }
        }
        for (at, hits) in &other.branches {
            let slot = self.branches.entry(*at).or_insert(0);
            *slot = slot.saturating_add(*hits);
        }
    }

    fn record_line(&mut self, line: u32, hits: u64) {
        let slot = self.lines.entry(line).or_insert(0);
        // The largest count of any position on the line, not the sum: a line
        // holding two mapped positions was executed once, not twice.
        *slot = (*slot).max(hits);
    }

    fn record_function(&mut self, at: Position, name: &str, hits: u64) {
        match self.functions.entry(at) {
            Entry::Occupied(mut held) => {
                let held = held.get_mut();
                held.1 = held.1.max(hits);
                if name < held.0.as_str() {
                    held.0 = name.to_owned();
                }
            }
            Entry::Vacant(free) => {
                free.insert((name.to_owned(), hits));
            }
        }
    }

    fn record_branch(&mut self, at: Position, hits: u64) {
        let slot = self.branches.entry(at).or_insert(0);
        *slot = (*slot).max(hits);
    }
}

fn ratio(hits: impl Iterator<Item = u64>) -> Ratio {
    let mut total = 0;
    let mut covered = 0;
    for count in hits {
        total += 1;
        if count > 0 {
            covered += 1;
        }
    }
    Ratio { covered, total }
}

/// A file's three ratios, or a whole run's.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    /// Author lines.
    pub lines: Ratio,
    /// Functions.
    pub functions: Ratio,
    /// Branches.
    pub branches: Ratio,
}

impl Totals {
    /// The ratio for one metric.
    #[must_use]
    pub const fn of(&self, metric: Metric) -> Ratio {
        match metric {
            Metric::Lines => self.lines,
            Metric::Functions => self.functions,
            Metric::Branches => self.branches,
        }
    }

    fn add(&mut self, other: Self) {
        self.lines.add(other.lines);
        self.functions.add(other.functions);
        self.branches.add(other.branches);
    }
}

/// A percentage each metric must reach, or nothing when it is not checked.
///
/// Percentages are whole numbers because a coverage gate is a policy, not a
/// measurement: `85` is a decision a team can argue about and `84.73` is a
/// number nobody chose.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Thresholds {
    /// Least line coverage that passes.
    pub lines: Option<u8>,
    /// Least function coverage that passes.
    pub functions: Option<u8>,
    /// Least branch coverage that passes.
    pub branches: Option<u8>,
}

impl Thresholds {
    /// Whether any threshold at all is set.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.lines.is_none() && self.functions.is_none() && self.branches.is_none()
    }

    /// The threshold for one metric.
    #[must_use]
    pub const fn of(&self, metric: Metric) -> Option<u8> {
        match metric {
            Metric::Lines => self.lines,
            Metric::Functions => self.functions,
            Metric::Branches => self.branches,
        }
    }
}

/// One threshold a run did not reach.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThresholdViolation {
    /// The file it is about, or nothing when it is about the whole project.
    pub file: Option<String>,
    /// Which metric.
    pub metric: Metric,
    /// What was measured.
    pub actual: Ratio,
    /// What was required.
    pub required: u8,
}

impl ThresholdViolation {
    /// A one-line explanation for a terminal or an error.
    #[must_use]
    pub fn describe(&self) -> String {
        let scope = match &self.file {
            Some(file) => file.clone(),
            None => String::from("the project"),
        };
        format!(
            "{scope}: {} coverage is {:.2}% ({}/{}), below the required {}%",
            self.metric.as_str(),
            self.actual.percent(),
            self.actual.covered,
            self.actual.total,
            self.required
        )
    }
}

/// Which files a coverage report is about.
///
/// Both lists are **substring** matches on the project-relative path, the same
/// rule and for the same reason as [`crate::TestFilter`]: a pattern from a
/// config file applied once per source file has no business being a
/// backtracking regular expression.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CoverageScope {
    include: Vec<String>,
    exclude: Vec<String>,
}

impl CoverageScope {
    /// A scope that keeps every project file.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Keep only files whose path contains one of these.
    #[must_use]
    pub fn with_include<S: Into<String>>(mut self, patterns: impl IntoIterator<Item = S>) -> Self {
        self.include = patterns
            .into_iter()
            .map(Into::into)
            .filter(|pattern| !pattern.is_empty())
            .collect();
        self
    }

    /// Drop files whose path contains one of these, whatever `include` says.
    #[must_use]
    pub fn with_exclude<S: Into<String>>(mut self, patterns: impl IntoIterator<Item = S>) -> Self {
        self.exclude = patterns
            .into_iter()
            .map(Into::into)
            .filter(|pattern| !pattern.is_empty())
            .collect();
        self
    }

    /// Whether `path` belongs in the report.
    #[must_use]
    pub fn admits(&self, path: &str) -> bool {
        if self.exclude.iter().any(|pattern| path.contains(pattern)) {
            return false;
        }
        self.include.is_empty() || self.include.iter().any(|pattern| path.contains(pattern))
    }
}

/// Everything a run measured.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Coverage {
    files: BTreeMap<String, FileCoverage>,
    never_loaded: Vec<String>,
    unmapped: BTreeSet<String>,
}

impl Coverage {
    /// An empty report.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Every file, in path order.
    pub fn files(&self) -> impl ExactSizeIterator<Item = (&str, &FileCoverage)> {
        self.files.iter().map(|(path, file)| (path.as_str(), file))
    }

    /// One file's coverage, if it has any.
    #[must_use]
    pub fn file(&self, path: &str) -> Option<&FileCoverage> {
        self.files.get(path)
    }

    /// How many files the report covers.
    #[must_use]
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Project files the suite never loaded, in path order.
    ///
    /// They are named rather than scored. A file no test imports has no mapped
    /// positions to count, so it has no honest denominator — and reporting it
    /// as `0/0`, which every ratio here calls a hundred per cent, would let a
    /// file that nothing tests raise the number. See [`Coverage::with_never_loaded`].
    #[must_use]
    pub fn never_loaded(&self) -> &[String] {
        &self.never_loaded
    }

    /// Project modules that ran and could not be mapped back to Flow, in path
    /// order.
    ///
    /// A module uf did not compile has no author position for a count to
    /// belong to: a `@noflow` file the loader passes through untouched, or one
    /// loaded on a thread whose transform wrote no map. They are named — as
    /// *files*, not as executions, because a dozen workers loading one module
    /// is one gap in the report and not a dozen — so that a number missing from
    /// the report is a number somebody can go and look for.
    ///
    /// A file that was measured is not in this list even when one of its many
    /// loads was not: a module imported twice under two specifiers is one V8
    /// script each time, and reporting the file as missing while its numbers
    /// are in the table would send a reader looking for a gap that is not
    /// there.
    #[must_use]
    pub fn unmapped(&self) -> Vec<&str> {
        self.unmapped
            .iter()
            .map(String::as_str)
            .filter(|path| !self.files.contains_key(*path))
            .collect()
    }

    /// Name the project files the suite never loaded.
    #[must_use]
    pub fn with_never_loaded<S: Into<String>>(
        mut self,
        files: impl IntoIterator<Item = S>,
    ) -> Self {
        self.never_loaded = files.into_iter().map(Into::into).collect();
        self.never_loaded.sort();
        self.never_loaded.dedup();
        self
    }

    /// The run's totals, over every file in the report.
    #[must_use]
    pub fn totals(&self) -> Totals {
        let mut totals = Totals::default();
        for file in self.files.values() {
            totals.add(file.totals());
        }
        totals
    }

    /// Add another document's counts to this report.
    pub fn merge(&mut self, other: &Self) {
        for (path, file) in &other.files {
            self.files.entry(path.clone()).or_default().merge(file);
        }
        for script in &other.unmapped {
            self.unmapped.insert(script.clone());
        }
        for file in &other.never_loaded {
            if !self.never_loaded.contains(file) {
                self.never_loaded.push(file.clone());
            }
        }
        self.never_loaded.sort();
    }

    /// Drop every file the scope does not admit.
    #[must_use]
    pub fn within(mut self, scope: &CoverageScope) -> Self {
        self.files.retain(|path, _| scope.admits(path));
        self.never_loaded.retain(|path| scope.admits(path));
        self.unmapped.retain(|path| scope.admits(path));
        self
    }

    /// Every threshold this report does not reach, project first then per file.
    ///
    /// A file the suite never loaded fails every per-file threshold that is
    /// set, with a ratio of `0/0`. That is the one place a zero denominator is
    /// not a pass, and it is deliberate: the per-file threshold exists to catch
    /// the file nobody tested, and a file nobody even imported is the extreme
    /// case of it.
    #[must_use]
    pub fn violations(&self, project: Thresholds, per_file: Thresholds) -> Vec<ThresholdViolation> {
        let mut violations = Vec::new();
        let totals = self.totals();
        for metric in Metric::all() {
            if let Some(required) = project.of(metric) {
                let actual = totals.of(metric);
                if !actual.meets(required) {
                    violations.push(ThresholdViolation {
                        file: None,
                        metric,
                        actual,
                        required,
                    });
                }
            }
        }
        if per_file.is_empty() {
            return violations;
        }
        for (path, file) in &self.files {
            let totals = file.totals();
            for metric in Metric::all() {
                if let Some(required) = per_file.of(metric)
                    && !totals.of(metric).meets(required)
                {
                    violations.push(ThresholdViolation {
                        file: Some(path.clone()),
                        metric,
                        actual: totals.of(metric),
                        required,
                    });
                }
            }
        }
        for path in &self.never_loaded {
            for metric in Metric::all() {
                if let Some(required) = per_file.of(metric) {
                    violations.push(ThresholdViolation {
                        file: Some(path.clone()),
                        metric,
                        actual: Ratio::default(),
                        required,
                    });
                }
            }
        }
        violations
    }

    /// Record one mapped position's count.
    fn record(&mut self, file: &str, line: u32, hits: u64) {
        self.files
            .entry(file.to_owned())
            .or_default()
            .record_line(line, hits);
    }
}

/// Why a coverage document could not be read.
#[derive(Debug, thiserror::Error)]
pub enum CoverageError {
    /// The directory Node was told to write into could not be read.
    #[error("could not read the coverage directory {path}: {source}")]
    Directory {
        /// The directory.
        path: Utf8PathBuf,
        /// What the filesystem said.
        source: std::io::Error,
    },
    /// One document could not be read.
    #[error("could not read {path}: {source}")]
    Read {
        /// The document.
        path: Utf8PathBuf,
        /// What the filesystem said.
        source: std::io::Error,
    },
    /// One document was not the shape Node writes.
    #[error("{path} is not a V8 coverage document: {message}")]
    Malformed {
        /// The document.
        path: Utf8PathBuf,
        /// What was wrong with it.
        message: String,
    },
    /// One document was larger than [`MAX_COVERAGE_DOCUMENT_BYTES`].
    #[error("{path} is {size} bytes, over the {MAX_COVERAGE_DOCUMENT_BYTES}-byte limit")]
    TooLarge {
        /// The document.
        path: Utf8PathBuf,
        /// How large it was.
        size: u64,
    },
}

/// Read and merge every coverage document `directory` holds.
///
/// `root` is the project root: counts land on paths relative to it, and a
/// source outside it — the host's own internals, a dependency compiled
/// elsewhere — is not this project's coverage and is dropped.
///
/// Documents are read in name order so that the merge, which is addition, is
/// the same whichever order the workers happened to exit in.
///
/// # Errors
///
/// [`CoverageError`] when the directory cannot be listed or one of its
/// documents cannot be read or parsed. A malformed document is an error rather
/// than a silent omission: coverage that quietly leaves out a worker is a
/// number that is wrong in the flattering direction.
pub fn read_directory(directory: &Utf8Path, root: &Utf8Path) -> Result<Coverage, CoverageError> {
    let mut documents: Vec<Utf8PathBuf> = Vec::new();
    let entries = std::fs::read_dir(directory).map_err(|source| CoverageError::Directory {
        path: directory.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| CoverageError::Directory {
            path: directory.to_path_buf(),
            source,
        })?;
        let Ok(path) = Utf8PathBuf::from_path_buf(entry.path()) else {
            continue;
        };
        if path.extension() == Some("json") {
            documents.push(path);
        }
    }
    documents.sort();

    let mut coverage = Coverage::new();
    for document in &documents {
        coverage.merge(&read_document(document, root)?);
    }
    Ok(coverage)
}

/// Read one V8 coverage document.
///
/// # Errors
///
/// [`CoverageError`] when the file cannot be read or is not a coverage
/// document.
pub fn read_document(path: &Utf8Path, root: &Utf8Path) -> Result<Coverage, CoverageError> {
    let size = std::fs::metadata(path)
        .map_err(|source| CoverageError::Read {
            path: path.to_path_buf(),
            source,
        })?
        .len();
    if size > MAX_COVERAGE_DOCUMENT_BYTES {
        return Err(CoverageError::TooLarge {
            path: path.to_path_buf(),
            size,
        });
    }
    let text = std::fs::read_to_string(path).map_err(|source| CoverageError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    parse_document(&text, root).map_err(|message| CoverageError::Malformed {
        path: path.to_path_buf(),
        message,
    })
}

/// Turn one document's JSON into coverage against `root`.
///
/// # Errors
///
/// The message to put in a [`CoverageError::Malformed`] when the text is not
/// the shape Node writes.
pub fn parse_document(text: &str, root: &Utf8Path) -> Result<Coverage, String> {
    let document: V8Document = serde_json::from_str(text).map_err(|error| error.to_string())?;
    let mut coverage = Coverage::new();
    for script in document.result.iter().take(MAX_SCRIPTS_PER_DOCUMENT) {
        // A run loads thousands of scripts that are nobody's source: Node's own
        // internals, an `eval`, a dependency compiled somewhere else. They are
        // passed over in silence rather than counted, because a report that
        // says "31,499 scripts were left out" has told a reader nothing except
        // that Node is large. What *is* counted is a script that is one of this
        // project's files and could still not be mapped — the case where a
        // number really is missing from the report.
        let Some(named) = project_script(&script.url, root) else {
            continue;
        };
        let Some(entry) = document.source_map_cache.get(&script.url) else {
            coverage.unmapped.insert(named);
            continue;
        };
        let Some(data) = &entry.data else {
            coverage.unmapped.insert(named);
            continue;
        };
        if !absorb_script(&mut coverage, script, entry, data, root) {
            coverage.unmapped.insert(named);
        }
    }
    Ok(coverage)
}

/// A script's own URL as a project-relative path, or nothing when the project
/// is not responsible for it.
///
/// The script's URL rather than the map's sources, because this runs before the
/// map is parsed: it is the cheap test that keeps `node:internal/...` out of the
/// expensive one.
///
/// The `node_modules` rule mirrors `isFlowModule` in
/// `packages/host/transform.js`, and mirrors it deliberately: that function
/// decides which modules `uf` compiles, and coverage is a report about compiled
/// modules, so the two answering differently would mean measuring a set of
/// files nothing produced a map for. A dependency that ships JavaScript is not
/// this project's coverage; `@uniflowed/*` is the same exception there and
/// here, because those packages ship Flow.
///
/// In practice a host resolves symlinks, so a workspace's own package arrives
/// as its real path rather than through `node_modules` at all. The exception is
/// kept anyway: a project that installed `@uniflowed/*` from the registry has
/// no symlink to resolve.
fn project_script(url: &str, root: &Utf8Path) -> Option<String> {
    // A specifier's query is how a host asks for the same file again — the
    // worker's `?uf-run=N` cache-buster, the module mocker's own — and it is
    // not part of the file's name. Dropped here so a module loaded twice under
    // two specifiers is one path, the way `stripQuery` in
    // `packages/host/transform.js` does it for the same reason.
    let url = url.split_once('?').map_or(url, |(path, _)| path);
    let path = decode_file_url(url)?;
    let relative = path.strip_prefix(root).ok()?;
    let relative = relative.as_str();
    if relative.is_empty() {
        return None;
    }
    match relative.rfind("/node_modules/") {
        Some(at) if !relative[at..].starts_with("/node_modules/@uniflowed/") => None,
        _ if relative.starts_with("node_modules/")
            && !relative.starts_with("node_modules/@uniflowed/") =>
        {
            None
        }
        _ => Some(relative.to_owned()),
    }
}

/// The V8 document Node writes at exit, and only the parts this reads.
#[derive(Debug, Deserialize)]
struct V8Document {
    #[serde(default)]
    result: Vec<V8Script>,
    /// Present because the Node host runs with `--enable-source-maps`; the
    /// generated file's line lengths live here beside the map itself, which is
    /// what makes an offset into that file addressable at all.
    #[serde(rename = "source-map-cache", default)]
    source_map_cache: BTreeMap<String, V8SourceMapEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct V8Script {
    #[serde(default)]
    url: String,
    #[serde(default)]
    functions: Vec<V8Function>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct V8Function {
    #[serde(default)]
    function_name: String,
    #[serde(default)]
    ranges: Vec<V8Range>,
    #[serde(default)]
    is_block_coverage: bool,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
struct V8Range {
    start_offset: u32,
    end_offset: u32,
    count: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct V8SourceMapEntry {
    #[serde(default)]
    line_lengths: Vec<u32>,
    #[serde(default)]
    data: Option<serde_json::Value>,
}

/// Where each line of the generated file begins.
///
/// V8 counts in offsets into the source it compiled and a source map is
/// addressed by line and column, so one of the two has to be converted. Node
/// hands over the generated file's line lengths, which is exactly the table
/// that does it — and doing it this way means the generated text itself is
/// never read, which is the difference between a report that needs the build
/// output on disk and one that does not.
#[derive(Debug)]
struct GeneratedLines {
    starts: Vec<u32>,
}

impl GeneratedLines {
    fn new(line_lengths: &[u32]) -> Self {
        let mut starts = Vec::with_capacity(line_lengths.len());
        let mut at: u32 = 0;
        for length in line_lengths {
            starts.push(at);
            // One for the newline that ended the line. Node measures a line
            // without its terminator, and a file whose last line has none only
            // makes the final entry one too large, which nothing indexes.
            at = at.saturating_add(*length).saturating_add(1);
        }
        Self { starts }
    }

    /// The offset a zero-based line and column sit at, when the line exists.
    fn offset_of(&self, line: u32, column: u32) -> Option<u32> {
        let start = self.starts.get(usize::try_from(line).ok()?)?;
        Some(start.saturating_add(column))
    }
}

/// One mapped position: where it is in the generated file, and what it is in
/// the author's.
#[derive(Debug, Clone, Copy)]
struct MappedPosition {
    generated: u32,
    source: u32,
    line: u32,
    column: u32,
}

/// Fold one script's counts into `coverage`, reporting whether it could be.
fn absorb_script(
    coverage: &mut Coverage,
    script: &V8Script,
    entry: &V8SourceMapEntry,
    data: &serde_json::Value,
    root: &Utf8Path,
) -> bool {
    let Ok(json) = serde_json::to_string(data) else {
        return false;
    };
    let Ok(map) = SourceMap::from_json_string(&json) else {
        return false;
    };
    let lines = GeneratedLines::new(&entry.line_lengths);

    // Which of the map's sources are this project's, resolved once: a map has
    // one or two, and a script has thousands of tokens.
    let source_root = map.get_source_root().unwrap_or("");
    let mut sources: Vec<Option<String>> = Vec::new();
    for id in 0.. {
        let Some(source) = map.get_source(id) else {
            break;
        };
        sources.push(project_path(source_root, source, root));
    }
    if sources.iter().all(Option::is_none) {
        return false;
    }

    let mut positions: Vec<MappedPosition> = Vec::new();
    for token in map.get_tokens() {
        let Some(source) = token.get_source_id() else {
            continue;
        };
        if sources.get(source as usize).is_none_or(Option::is_none) {
            continue;
        }
        let Some(generated) = lines.offset_of(token.get_dst_line(), token.get_dst_col()) else {
            continue;
        };
        positions.push(MappedPosition {
            generated,
            source,
            line: token.get_src_line().saturating_add(1),
            column: token.get_src_col(),
        });
    }
    if positions.is_empty() {
        return false;
    }
    positions.sort_by_key(|position| position.generated);

    let ranges = flatten(script);
    let counts = counts_at(&positions, &ranges);
    for (position, count) in positions.iter().zip(&counts) {
        let Some(Some(file)) = sources.get(position.source as usize) else {
            continue;
        };
        coverage.record(file, position.line, *count);
    }

    record_functions(coverage, script, &positions, &sources);
    true
}

/// Every range in the script, outermost first.
///
/// V8's ranges nest — a function's inside the script's, a block's inside its
/// function's — and the count that applies at a position is the innermost
/// one's. Flattening them into one list is what lets a single sweep answer
/// that for every position, and it is why the script's top-level range does
/// not drown out an uncalled function's zero: the zero is deeper.
fn flatten(script: &V8Script) -> Vec<V8Range> {
    let mut ranges: Vec<V8Range> = Vec::new();
    for function in &script.functions {
        for range in &function.ranges {
            if ranges.len() >= MAX_RANGES_PER_SCRIPT {
                break;
            }
            ranges.push(*range);
        }
    }
    ranges.sort_by(|a, b| {
        a.start_offset
            .cmp(&b.start_offset)
            .then(b.end_offset.cmp(&a.end_offset))
    });
    ranges
}

/// The count in force at each position, in the positions' order.
///
/// A stack sweep rather than a search per position: the ranges are properly
/// nested and both lists are sorted, so one pass answers all of them.
fn counts_at(positions: &[MappedPosition], ranges: &[V8Range]) -> Vec<u64> {
    let mut counts = Vec::with_capacity(positions.len());
    let mut stack: Vec<V8Range> = Vec::new();
    let mut next = 0;
    for position in positions {
        while next < ranges.len() && ranges[next].start_offset <= position.generated {
            stack.push(ranges[next]);
            next += 1;
        }
        while stack
            .last()
            .is_some_and(|range| range.end_offset <= position.generated)
        {
            stack.pop();
        }
        counts.push(stack.last().map_or(0, |range| range.count));
    }
    counts
}

/// Attribute each function and each block to the first author position inside
/// it.
///
/// "Inside it" is the whole rule. A generated construct the author did not
/// write holds no mapped position of its own; what it holds is the author code
/// it wraps, and attributing the wrapper to the first line of that code is the
/// only answer that names something the author can go and look at. A generated
/// function or block holding no mapped position at all — a lowering's guard
/// chain, a memo block around nothing — is not counted in either direction.
fn record_functions(
    coverage: &mut Coverage,
    script: &V8Script,
    positions: &[MappedPosition],
    sources: &[Option<String>],
) {
    for function in &script.functions {
        let Some(own) = function.ranges.first() else {
            continue;
        };
        // V8 reports the whole script as a nameless function starting at zero.
        // It is the module's top level, not a function the author declared, and
        // counting it would give every file one function it can never miss.
        let is_script = own.start_offset == 0 && function.function_name.is_empty();
        if !is_script
            && let Some(at) = first_within(positions, own)
            && let Some(Some(file)) = sources.get(at.source as usize)
        {
            coverage
                .files
                .entry(file.clone())
                .or_default()
                .record_function(
                    Position {
                        line: at.line,
                        column: at.column,
                    },
                    &clamp_name(&function.function_name),
                    own.count,
                );
        }

        if !function.is_block_coverage {
            continue;
        }
        for block in function.ranges.iter().skip(1) {
            let Some(at) = first_within(positions, block) else {
                continue;
            };
            let Some(Some(file)) = sources.get(at.source as usize) else {
                continue;
            };
            coverage
                .files
                .entry(file.clone())
                .or_default()
                .record_branch(
                    Position {
                        line: at.line,
                        column: at.column,
                    },
                    block.count,
                );
        }
    }
}

/// The first mapped position inside `range`, when there is one.
fn first_within(positions: &[MappedPosition], range: &V8Range) -> Option<MappedPosition> {
    let at = positions.partition_point(|position| position.generated < range.start_offset);
    let found = positions.get(at)?;
    (found.generated < range.end_offset).then_some(*found)
}

/// A function name, bounded: V8 derives it from generated code, and generated
/// code can name a function with a whole expression.
fn clamp_name(name: &str) -> String {
    if name.len() <= MAX_FUNCTION_NAME_BYTES {
        return name.to_owned();
    }
    let mut end = MAX_FUNCTION_NAME_BYTES;
    while end > 0 && !name.is_char_boundary(end) {
        end -= 1;
    }
    name[..end].to_owned()
}

/// A source map entry as a path relative to the project root, or nothing when
/// it is not this project's.
fn project_path(source_root: &str, source: &str, root: &Utf8Path) -> Option<String> {
    let joined = if source_root.is_empty() || source.starts_with('/') || source.contains("://") {
        source.to_owned()
    } else {
        format!("{}{source}", with_slash(source_root))
    };
    let path = decode_file_url(&joined)?;
    let relative = path.strip_prefix(root).ok()?;
    let relative = relative.as_str();
    if relative.is_empty() {
        return None;
    }
    Some(relative.to_owned())
}

fn with_slash(source_root: &str) -> String {
    if source_root.ends_with('/') {
        source_root.to_owned()
    } else {
        format!("{source_root}/")
    }
}

/// A `file:` URL, or a plain absolute path, as a path.
///
/// Node writes the sources of a map it cached as absolute `file:` URLs, so the
/// percent-encoding has to come back off — a project in a directory with a
/// space in its name is not an exotic case.
fn decode_file_url(source: &str) -> Option<Utf8PathBuf> {
    let path = match source.strip_prefix("file://") {
        Some(rest) => {
            // `file:///a/b` leaves `/a/b`; a `file://host/a` we cannot use.
            if !rest.starts_with('/') {
                return None;
            }
            percent_decode(rest)?
        }
        None => {
            if source.contains("://") {
                return None;
            }
            source.to_owned()
        }
    };
    Some(Utf8PathBuf::from(path))
}

fn percent_decode(text: &str) -> Option<String> {
    if !text.contains('%') {
        return Some(text.to_owned());
    }
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let high = hex(*bytes.get(at + 1)?)?;
            let low = hex(*bytes.get(at + 2)?)?;
            out.push(high * 16 + low);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(out).ok()
}

const fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
