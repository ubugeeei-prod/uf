//! `uf test --bench`: the benchmarks a run timed, against a stored baseline.
//!
//! `bench()` in `@uniflowed/test` declares a benchmark. An ordinary run reports
//! it skipped, and `uf test --bench` runs the benchmarks in place of the tests;
//! `packages/test/internal/run.js` is how one is timed. What comes back is one
//! [`uf_test::BenchStats`] per benchmark, and this module is what happens to
//! them: a table, a comparison with the baseline an earlier run saved, and a
//! verdict.
//!
//! # A baseline
//!
//! `.uf/bench-baseline.json`, or the file `--baseline` names: each benchmark's
//! median, by its file and its full name. `--save-baseline` writes it from the
//! run. The median is what is compared, because a benchmark's slowest calls
//! are the machine's (a collection, a timer, a busy core) and its fastest are
//! luck; the middle call is the one that moves when the code does.
//!
//! A baseline is only as good as the machine that wrote it is like the one that
//! reads it, so the default file lives in `.uf`, which is not committed. A team
//! that wants CI to gate on one saves it from CI — `--save-baseline` on the
//! branch it merges into, restored before `--bench` on a pull request — rather
//! than from a laptop.
//!
//! # A verdict
//!
//! A benchmark whose median is more than `--bench-threshold` per cent over its
//! baseline's, 20 unless told, has regressed, and a regression fails the run:
//! that is what makes a baseline a gate rather than a log. One that improved by
//! as much is named as well. One the baseline has no entry for is new. One
//! whose baseline median is under a microsecond cannot be compared by a
//! percentage, and says so rather than failing over a rounding.

use std::io::Read;

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uf_term::{Cell, Column, KeyValue, Status, Table, Tone, push_spaces};
use uf_test::{BenchStats, TestRunReport};

use super::TestArgs;
use crate::support::plural;
use crate::ui::Ui;

/// Where a baseline is read from and saved to, unless `--baseline` says.
pub(crate) const DEFAULT_BASELINE: &str = ".uf/bench-baseline.json";

/// How far past its baseline a median may go before it has regressed, in per
/// cent, unless `--bench-threshold` says.
pub(crate) const DEFAULT_THRESHOLD_PERCENT: u32 = 20;

/// The version of the baseline file this uf reads and writes.
const BASELINE_VERSION: u32 = 1;

/// Largest baseline read. One entry is a path, a name and two numbers.
const MAX_BASELINE_BYTES: u64 = 16 * 1024 * 1024;

/// A saved baseline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Baseline {
    version: u32,
    benchmarks: Vec<BaselineEntry>,
}

/// One benchmark's saved median.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BaselineEntry {
    file: String,
    name: String,
    median_micros: u64,
    samples: usize,
}

/// How one benchmark compares with its baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Verdict {
    /// Its median is past the threshold over the baseline's.
    Regressed,
    /// Its median is past the threshold under the baseline's.
    Improved,
    /// Within the threshold either way.
    Steady,
    /// The baseline has no entry for it.
    New,
    /// The baseline's median is under a microsecond.
    Incomparable,
}

impl Verdict {
    const fn label(self) -> &'static str {
        match self {
            Self::Regressed => "regressed",
            Self::Improved => "improved",
            Self::Steady => "steady",
            Self::New => "new",
            Self::Incomparable => "too fast to compare",
        }
    }
}

/// One benchmark the run timed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Row {
    file: String,
    name: String,
    stats: BenchStats,
    /// The baseline's median, when it has one.
    baseline_median_micros: Option<u64>,
    /// How far this median moved from the baseline's, in hundredths of a per
    /// cent: `1250` is 12.5% slower.
    change_basis_points: Option<i64>,
    verdict: Verdict,
}

/// Every benchmark a run timed, compared with the baseline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Comparison {
    /// The baseline file, as it is shown.
    baseline: String,
    /// Whether that file held a baseline when the run started.
    found: bool,
    threshold: u32,
    rows: Vec<Row>,
    /// Benchmarks the baseline has and this run did not time.
    not_run: Vec<String>,
    /// Whether this run was saved as the baseline.
    saved: bool,
}

/// Compare the benchmarks `report` timed with the baseline, and save them as
/// the baseline when `--save-baseline` asks.
pub(crate) fn compare(
    root: &Utf8Path,
    args: &TestArgs,
    report: &TestRunReport,
) -> Result<Comparison> {
    let named = args.baseline.as_deref();
    let path = baseline_path(root, named);
    let threshold = args.bench_threshold.unwrap_or(DEFAULT_THRESHOLD_PERCENT);
    let baseline = read_baseline(&path)?;
    if baseline.is_none() && named.is_some() && !args.save_baseline {
        bail!(uf_infra::cstr!(
            "there is no benchmark baseline at {path}: run `uf test --bench --save-baseline \
             --baseline {path}` to write one"
        ));
    }

    let rows: Vec<Row> = report
        .files
        .iter()
        .flat_map(|file| file.records.iter())
        .filter_map(|record| {
            let stats = record.bench?;
            let base = baseline.as_ref().and_then(|baseline| {
                baseline
                    .benchmarks
                    .iter()
                    .find(|entry| entry.file == record.file && entry.name == record.name)
                    .map(|entry| entry.median_micros)
            });
            let (verdict, change_basis_points) = judge(stats.median_micros, base, threshold);
            Some(Row {
                file: record.file.clone(),
                name: record.name.clone(),
                stats,
                baseline_median_micros: base,
                change_basis_points,
                verdict,
            })
        })
        .collect();
    let not_run: Vec<String> = baseline
        .as_ref()
        .map(|baseline| {
            baseline
                .benchmarks
                .iter()
                .filter(|entry| {
                    !rows
                        .iter()
                        .any(|row| row.file == entry.file && row.name == entry.name)
                })
                .map(|entry| {
                    uf_infra::into_string(uf_infra::cstr!("{} > {}", entry.file, entry.name))
                })
                .collect()
        })
        .unwrap_or_default();

    if args.save_baseline {
        write_baseline(&path, &rows)?;
    }
    Ok(Comparison {
        baseline: path
            .strip_prefix(root)
            .map_or_else(|_| path.to_string(), ToString::to_string),
        found: baseline.is_some(),
        threshold,
        rows,
        not_run,
        saved: args.save_baseline,
    })
}

impl Comparison {
    /// Fail when a benchmark regressed, unless this run was saved as the
    /// baseline: saving is accepting the numbers.
    pub(crate) fn verdict(&self) -> Result<()> {
        if self.saved {
            return Ok(());
        }
        let regressed: Vec<String> = self
            .rows
            .iter()
            .filter(|row| row.verdict == Verdict::Regressed)
            .map(|row| {
                uf_infra::into_string(uf_infra::cstr!(
                    "{} > {}: {} against {} ({})",
                    row.file,
                    row.name,
                    duration(row.stats.median_micros),
                    row.baseline_median_micros
                        .map_or_else(String::new, duration),
                    row.change_basis_points.map_or_else(String::new, change)
                ))
            })
            .collect();
        if regressed.is_empty() {
            return Ok(());
        }
        bail!(uf_infra::cstr!(
            "{} slower than {} allows, more than {}% over its median:\n  {}",
            plural(regressed.len(), "benchmark ran"),
            self.baseline,
            self.threshold,
            regressed.join("\n  ")
        ))
    }

    /// The `benchmarks` object in `uf test --bench --json`.
    pub(crate) fn payload(&self) -> Value {
        json!({
            "baseline": self.baseline,
            "found": self.found,
            "saved": self.saved,
            "thresholdPercent": self.threshold,
            "benchmarks": self.rows,
            "notRun": self.not_run,
        })
    }
}

/// The benchmarks table, and what was compared with what.
pub(crate) fn render(ui: &mut Ui, comparison: &Comparison) {
    let rows: Vec<[String; 7]> = comparison
        .rows
        .iter()
        .map(|row| {
            [
                uf_infra::into_string(uf_infra::cstr!("{} > {}", row.file, row.name)),
                duration(row.stats.median_micros),
                duration(row.stats.p75_micros),
                duration(row.stats.min_micros),
                row.stats.samples.to_string(),
                row.change_basis_points
                    .map_or_else(|| String::from("—"), change),
                row.verdict.label().to_string(),
            ]
        })
        .collect();
    let against = match (comparison.found, comparison.saved) {
        (_, true) => uf_infra::into_string(uf_infra::cstr!("saved to {}", comparison.baseline)),
        (true, false) => uf_infra::into_string(uf_infra::cstr!(
            "{} · a median more than {}% over it fails the run",
            comparison.baseline,
            comparison.threshold
        )),
        (false, false) => uf_infra::into_string(uf_infra::cstr!(
            "none at {} yet · `--save-baseline` writes one",
            comparison.baseline
        )),
    };
    ui.render(|renderer, out| {
        renderer.blank(out);
        renderer.heading(out, 2, "benchmarks");
        if rows.is_empty() {
            push_spaces(out, 2);
            renderer.status(out, Status::Info, "no benchmarks ran");
        } else {
            let mut table = Table::new(vec![
                Column::left("benchmark"),
                Column::left("median"),
                Column::left("p75"),
                Column::left("min"),
                Column::left("calls"),
                Column::left("change"),
                Column::left("verdict"),
            ]);
            for [name, median, p75, min, calls, moved, verdict] in &rows {
                let tone = match verdict.as_str() {
                    "regressed" => Tone::Bad,
                    "improved" => Tone::Good,
                    _ => Tone::Muted,
                };
                table.push(vec![
                    Cell::new(name.as_str()),
                    Cell::toned(median, Tone::Number),
                    Cell::toned(p75, Tone::Number),
                    Cell::toned(min, Tone::Number),
                    Cell::toned(calls, Tone::Number),
                    Cell::toned(moved, tone),
                    Cell::toned(verdict, tone),
                ]);
            }
            renderer.table(out, 2, &table);
        }
        renderer.blank(out);
        renderer.key_values(out, 2, &[KeyValue::new("baseline", &against)]);
        for missing in &comparison.not_run {
            push_spaces(out, 2);
            renderer.status(
                out,
                Status::Warn,
                uf_infra::cstr!("{missing} is in the baseline and did not run").as_str(),
            );
        }
    });
}

/// Where the baseline is: the file `--baseline` names, relative to the project,
/// or [`DEFAULT_BASELINE`].
fn baseline_path(root: &Utf8Path, named: Option<&str>) -> Utf8PathBuf {
    let path = Utf8Path::new(named.unwrap_or(DEFAULT_BASELINE));
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

/// How a median compares with its baseline's, and by how much.
fn judge(median: u64, baseline: Option<u64>, threshold: u32) -> (Verdict, Option<i64>) {
    let Some(base) = baseline else {
        return (Verdict::New, None);
    };
    if base == 0 {
        return (Verdict::Incomparable, None);
    }
    let difference = i128::from(median) - i128::from(base);
    let basis_points = i64::try_from(difference * 10_000 / i128::from(base)).unwrap_or(i64::MAX);
    // In whole numbers: `difference / base > threshold / 100`, so exactly the
    // threshold is still steady.
    let limit = i128::from(threshold) * i128::from(base);
    let verdict = if difference * 100 > limit {
        Verdict::Regressed
    } else if -difference * 100 > limit {
        Verdict::Improved
    } else {
        Verdict::Steady
    };
    (verdict, Some(basis_points))
}

/// A duration in microseconds, in the unit that keeps it short.
fn duration(micros: u64) -> String {
    if micros < 1_000 {
        uf_infra::into_string(uf_infra::cstr!("{micros}µs"))
    } else if micros < 1_000_000 {
        uf_infra::into_string(uf_infra::cstr!(
            "{}.{:02}ms",
            micros / 1_000,
            micros % 1_000 / 10
        ))
    } else {
        uf_infra::into_string(uf_infra::cstr!(
            "{}.{:02}s",
            micros / 1_000_000,
            micros % 1_000_000 / 10_000
        ))
    }
}

/// A change in hundredths of a per cent, signed: `+12.50%`.
fn change(basis_points: i64) -> String {
    let sign = if basis_points < 0 { '-' } else { '+' };
    let magnitude = basis_points.unsigned_abs();
    uf_infra::into_string(uf_infra::cstr!(
        "{sign}{}.{:02}%",
        magnitude / 100,
        magnitude % 100
    ))
}

/// The baseline at `path`, or `None` when there is no file there.
fn read_baseline(path: &Utf8Path) -> Result<Option<Baseline>> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| uf_infra::cstr!("could not open {path}"));
        }
    };
    let mut bytes = Vec::new();
    file.take(MAX_BASELINE_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .with_context(|| uf_infra::cstr!("could not read {path}"))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_BASELINE_BYTES {
        bail!(uf_infra::cstr!(
            "{path} is larger than the {MAX_BASELINE_BYTES} bytes a benchmark baseline can be"
        ));
    }
    let baseline: Baseline = serde_json::from_slice(&bytes).with_context(|| {
        uf_infra::into_string(uf_infra::cstr!(
            "{path} is not a benchmark baseline uf reads"
        ))
    })?;
    if baseline.version != BASELINE_VERSION {
        bail!(uf_infra::cstr!(
            "{path} is a version {} benchmark baseline, and this uf reads version \
             {BASELINE_VERSION}: save a new one with `uf test --bench --save-baseline`",
            baseline.version
        ));
    }
    Ok(Some(baseline))
}

/// Write `rows` as the baseline at `path`, in file and name order.
fn write_baseline(path: &Utf8Path, rows: &[Row]) -> Result<()> {
    let mut benchmarks: Vec<BaselineEntry> = rows
        .iter()
        .map(|row| BaselineEntry {
            file: row.file.clone(),
            name: row.name.clone(),
            median_micros: row.stats.median_micros,
            samples: row.stats.samples,
        })
        .collect();
    benchmarks.sort_by(|a, b| a.file.cmp(&b.file).then(a.name.cmp(&b.name)));
    let baseline = Baseline {
        version: BASELINE_VERSION,
        benchmarks,
    };
    if let Some(parent) = path.parent()
        && !parent.as_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .with_context(|| uf_infra::cstr!("could not create {parent}"))?;
    }
    let mut text =
        serde_json::to_string_pretty(&baseline).context("could not serialise the baseline")?;
    text.push('\n');
    std::fs::write(path, text).with_context(|| uf_infra::cstr!("could not write {path}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use uf_test::{FileReport, FileStatus, TestRecord, TestStatus};

    fn stats(median: u64) -> BenchStats {
        BenchStats::from_samples(&[median]).expect("one sample")
    }

    fn report(benchmarks: &[(&str, u64)]) -> TestRunReport {
        TestRunReport {
            files: vec![FileReport {
                file: String::from("src/a.bench.js"),
                status: FileStatus::Completed,
                duration_micros: 0,
                records: benchmarks
                    .iter()
                    .map(|(name, median)| TestRecord {
                        file: String::from("src/a.bench.js"),
                        name: (*name).to_owned(),
                        line: 1,
                        column: 1,
                        status: TestStatus::Passed,
                        attempts: 1,
                        duration_micros: 0,
                        output: Vec::new(),
                        bench: Some(stats(*median)),
                    })
                    .collect(),
                output: Vec::new(),
            }],
            ..TestRunReport::default()
        }
    }

    #[test]
    fn a_median_past_the_threshold_either_way_is_named_and_exactly_at_it_is_steady() {
        assert_eq!(
            judge(1_201, Some(1_000), 20),
            (Verdict::Regressed, Some(2_010))
        );
        assert_eq!(
            judge(1_200, Some(1_000), 20),
            (Verdict::Steady, Some(2_000))
        );
        assert_eq!(judge(800, Some(1_000), 20), (Verdict::Steady, Some(-2_000)));
        assert_eq!(
            judge(799, Some(1_000), 20),
            (Verdict::Improved, Some(-2_010))
        );
        assert_eq!(judge(5, None, 20), (Verdict::New, None));
        assert_eq!(judge(5, Some(0), 20), (Verdict::Incomparable, None));
    }

    #[test]
    fn durations_and_changes_read_in_the_unit_that_keeps_them_short() {
        assert_eq!(duration(999), "999µs");
        assert_eq!(duration(1_234), "1.23ms");
        assert_eq!(duration(2_500_000), "2.50s");
        assert_eq!(change(1_250), "+12.50%");
        assert_eq!(change(-5), "-0.05%");
        assert_eq!(change(0), "+0.00%");
    }

    #[test]
    fn a_saved_baseline_is_read_back_and_a_regression_against_it_fails() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let root = Utf8Path::from_path(directory.path()).expect("a UTF-8 path");
        let saving = TestArgs {
            bench: true,
            save_baseline: true,
            ..TestArgs::default()
        };

        let saved = compare(root, &saving, &report(&[("parse", 1_000), ("print", 50)]))
            .expect("the baseline is saved");
        assert!(saved.verdict().is_ok());
        assert!(!saved.found);
        assert!(root.join(DEFAULT_BASELINE).is_file());

        let checking = TestArgs {
            bench: true,
            ..TestArgs::default()
        };
        let compared =
            compare(root, &checking, &report(&[("parse", 1_500)])).expect("the baseline is read");
        assert!(compared.found);
        assert_eq!(compared.rows[0].verdict, Verdict::Regressed);
        assert_eq!(compared.not_run, ["src/a.bench.js > print"]);
        let error = compared.verdict().expect_err("a regression fails the run");
        assert!(
            error.to_string().contains(
                "1 benchmark ran slower than .uf/bench-baseline.json allows, more than 20% \
                 over its median:\n  src/a.bench.js > parse: 1.50ms against 1.00ms (+50.00%)"
            ),
            "{error:#}"
        );
    }

    #[test]
    fn a_named_baseline_that_is_not_there_is_refused_and_a_foreign_version_too() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let root = Utf8Path::from_path(directory.path()).expect("a UTF-8 path");
        let args = TestArgs {
            bench: true,
            baseline: Some(String::from("perf/baseline.json")),
            ..TestArgs::default()
        };

        let missing = compare(root, &args, &report(&[])).expect_err("no baseline there");
        assert!(
            missing
                .to_string()
                .contains("there is no benchmark baseline at"),
            "{missing:#}"
        );

        std::fs::create_dir_all(root.join("perf")).expect("a directory");
        std::fs::write(
            root.join("perf/baseline.json"),
            "{\"version\": 9, \"benchmarks\": []}",
        )
        .expect("write");
        let foreign = compare(root, &args, &report(&[])).expect_err("another version");
        assert!(
            foreign
                .to_string()
                .contains("is a version 9 benchmark baseline"),
            "{foreign:#}"
        );
    }
}
