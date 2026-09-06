//! `uf test --coverage`: where the counts come from, and what is done with them.
//!
//! The measuring itself is Node's and the arithmetic is [`uf_test::coverage`];
//! what lives here is the part that is a *command*. Three jobs, and the order
//! matters:
//!
//! 1. Give the run a directory of its own to write raw V8 documents into, and
//!    take it away again afterwards. It is per-run rather than shared because
//!    Node appends and never reads: a directory left over from a previous run
//!    would merge that run's counts into this one's, and the number would be
//!    wrong in the flattering direction with nothing to show for it.
//! 2. Read them back, restrict them to the files the project says it cares
//!    about, and name the project files no test loaded at all.
//! 3. Write the reports and decide whether the run passed. The threshold is
//!    checked here, not in the runner, because it is the one part of coverage
//!    that changes the exit code — and an exit code is the command's to set.

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use uf_config::{CoverageConfig, CoverageReporterConfig};
use uf_test::{
    Coverage, CoverageRow, CoverageScope, ThresholdViolation, Thresholds, Totals, cobertura, lcov,
    text_rows,
};

use crate::cli::CoverageReporterArg;

/// Where reports go when nothing names a directory.
const DEFAULT_DIRECTORY: &str = "coverage";

/// The file name every code-host coverage integration already looks for.
const LCOV_FILE: &str = "lcov.info";

/// The name Cobertura consumers expect; Azure Pipelines and Jenkins both
/// default to a glob that matches it.
const COBERTURA_FILE: &str = "cobertura-coverage.xml";

/// This run's raw coverage directory, removed when the run is over.
///
/// Raw V8 documents are an implementation detail — one per worker process,
/// named by Node, holding offsets into JavaScript nobody wrote — and leaving
/// them behind would invite somebody to merge two runs' worth by accident. The
/// reports are the artefact; these are the workings.
#[derive(Debug)]
pub(super) struct RawCoverage {
    directory: Utf8PathBuf,
}

impl RawCoverage {
    /// Make a directory this run alone writes into.
    ///
    /// Named for the process and the moment it started, so two `uf test` runs
    /// in one checkout — a watch loop and a terminal, CI running two jobs —
    /// cannot read each other's counts.
    pub(super) fn create(root: &Utf8Path) -> Result<Self> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        let directory = root
            .join(".uf")
            .join("cache")
            .join("coverage")
            .join(format!("{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&directory)
            .with_context(|| format!("could not create the coverage directory {directory}"))?;
        Ok(Self { directory })
    }

    /// The directory workers write into.
    pub(super) fn directory(&self) -> &Utf8Path {
        &self.directory
    }
}

impl Drop for RawCoverage {
    fn drop(&mut self) {
        // Tolerant: failing to tidy up is not a reason to fail a test run, and
        // the next run will not read this directory anyway — it has its own.
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

/// Everything the report and the exit code need from a coverage run.
#[derive(Debug)]
pub(super) struct CoverageSection {
    /// One row per file, in path order.
    pub(super) rows: Vec<CoverageRow>,
    /// The project's totals.
    pub(super) totals: Totals,
    /// The reports that were written, in the order they were written.
    pub(super) written: Vec<Utf8PathBuf>,
    /// Project files no test loaded.
    pub(super) never_loaded: Vec<String>,
    /// Project modules that ran with no source map and were left out.
    pub(super) unmapped: Vec<String>,
    /// Thresholds the run did not reach.
    pub(super) violations: Vec<ThresholdViolation>,
    /// Whether the terminal table was asked for.
    pub(super) show_table: bool,
}

/// Which files a coverage report is about, from the project's config.
fn scope(config: &CoverageConfig) -> CoverageScope {
    CoverageScope::new()
        .with_include(
            config
                .include
                .iter()
                .map(compact_str::CompactString::as_str),
        )
        .with_exclude(
            config
                .exclude
                .iter()
                .map(compact_str::CompactString::as_str),
        )
}

/// The thresholds the project set.
fn thresholds(config: &uf_config::CoverageThresholdConfig) -> Thresholds {
    Thresholds {
        lines: config.lines,
        functions: config.functions,
        branches: config.branches,
    }
}

/// Which reports to write: the command line when it said, the config otherwise.
fn reporters(
    config: &CoverageConfig,
    requested: &[CoverageReporterArg],
) -> Vec<CoverageReporterConfig> {
    if requested.is_empty() {
        return config.reporters.clone();
    }
    let mut chosen: Vec<CoverageReporterConfig> = requested
        .iter()
        .map(|reporter| match reporter {
            CoverageReporterArg::Text => CoverageReporterConfig::Text,
            CoverageReporterArg::Lcov => CoverageReporterConfig::Lcov,
            CoverageReporterArg::Cobertura => CoverageReporterConfig::Cobertura,
        })
        .collect();
    chosen.dedup();
    chosen
}

/// Read what the workers wrote, write the reports, and say what failed.
///
/// `project_paths` is every source file the project scan found: what it adds is
/// the files that were *not* loaded, which is the half of a coverage number
/// that measuring alone cannot see. A file no test imports produces no V8
/// script and no mapping, so it is invisible to the counters and would
/// otherwise raise the project's percentage by being absent from it.
pub(super) fn collect(
    root: &Utf8Path,
    config: &CoverageConfig,
    raw: &RawCoverage,
    directory: &Utf8Path,
    requested: &[CoverageReporterArg],
    project_paths: &[String],
) -> Result<(Coverage, CoverageSection)> {
    let scope = scope(config);
    let measured = uf_test::read_directory(raw.directory(), root)?.within(&scope);
    let never_loaded: Vec<&str> = project_paths
        .iter()
        .map(String::as_str)
        .filter(|path| scope.admits(path) && measured.file(path).is_none())
        .collect();
    let coverage = measured.with_never_loaded(never_loaded);

    let chosen = reporters(config, requested);
    let mut written = Vec::new();
    if chosen
        .iter()
        .any(|reporter| !matches!(reporter, CoverageReporterConfig::Text))
    {
        std::fs::create_dir_all(directory)
            .with_context(|| format!("could not create {directory}"))?;
    }
    for reporter in &chosen {
        let (name, body) = match reporter {
            CoverageReporterConfig::Text => continue,
            CoverageReporterConfig::Lcov => (LCOV_FILE, lcov(&coverage)),
            CoverageReporterConfig::Cobertura => (COBERTURA_FILE, cobertura(&coverage)),
        };
        let path = directory.join(name);
        std::fs::write(&path, body).with_context(|| format!("could not write {path}"))?;
        written.push(path);
    }

    let violations = coverage.violations(
        thresholds(&config.thresholds),
        thresholds(&config.per_file_thresholds),
    );
    let section = CoverageSection {
        rows: text_rows(&coverage),
        totals: coverage.totals(),
        written,
        never_loaded: coverage.never_loaded().to_vec(),
        unmapped: coverage.unmapped().into_iter().map(str::to_owned).collect(),
        violations,
        show_table: chosen.contains(&CoverageReporterConfig::Text),
    };
    Ok((coverage, section))
}

/// Where the reports go: the command line when it said, the config otherwise.
pub(super) fn directory_for(
    root: &Utf8Path,
    config: &CoverageConfig,
    requested: Option<&str>,
) -> Utf8PathBuf {
    let named = requested.unwrap_or(config.directory.as_str());
    let named = if named.is_empty() {
        DEFAULT_DIRECTORY
    } else {
        named
    };
    let path = Utf8Path::new(named);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

/// The `coverage` object in `uf test --json`.
///
/// The same numbers the table shows and the same numbers the threshold is
/// checked against — a reader that trusts one of the three and not the others
/// is a reader uf has misled.
pub(super) fn payload(coverage: &Coverage) -> serde_json::Value {
    let totals = coverage.totals();
    serde_json::json!({
        "lines": ratio_payload(totals.lines),
        "functions": ratio_payload(totals.functions),
        "branches": ratio_payload(totals.branches),
        "neverLoaded": coverage.never_loaded(),
        "unmapped": coverage.unmapped(),
        "files": coverage.files().map(|(path, file)| {
            let totals = file.totals();
            serde_json::json!({
                "file": path,
                "lines": ratio_payload(totals.lines),
                "functions": ratio_payload(totals.functions),
                "branches": ratio_payload(totals.branches),
                "lineHits": file.lines().map(|(line, hits)| serde_json::json!([line, hits]))
                    .collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>(),
    })
}

fn ratio_payload(ratio: uf_test::Ratio) -> serde_json::Value {
    serde_json::json!({
        "covered": ratio.covered,
        "total": ratio.total,
        // Two decimals, so the document is a function of the suite alone: a
        // full-precision float would put the last bits of a division in a
        // file two runs are supposed to be able to compare byte for byte.
        "percent": format!("{:.2}", ratio.percent()),
    })
}
