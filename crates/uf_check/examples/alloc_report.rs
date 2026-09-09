//! What `uf check` costs, in allocations and bytes, for ubugeeei-prod/uf#678.
//!
//! The third of these, after `uf_transform`'s and `uf_lint`'s, and the one the
//! other two exist to be compared against: the checker is where uf's memory
//! goes, and until this ran nobody had counted it. Run it as
//! `cargo run --release --example alloc_report -p uf_check -- <file>`.
//!
//! Three modes, because #678 asks three separate questions:
//!
//! * **plain** — what one module costs, which is the headline figure;
//! * **`--batch N`** — the same module N times, which separates the cost paid
//!   once per *call* (a builtin environment, forced as far as the batch needs
//!   it) from the cost paid per *module*. Neither is visible on its own: a
//!   single run reports their sum and says nothing about which is which;
//! * **`--cache`** — the same module through one [`CheckCache`] three times,
//!   which is the question #678 leaves open. `check_sources_cached` exists for
//!   the caller that checks one file at a time, and a batch proving cheap says
//!   nothing about whether the cache does. Cold, warm, and warm-after-an-edit,
//!   because the third is the one an editor actually asks for and it is not
//!   the second: a record's key is over the file's own text, so a keystroke
//!   makes a key nothing has been filed under.
//!
//! `--phases` adds the table #678 asks for and could not take: one row per
//! `profile_span!` in the checker, so the fixed cost of forcing a builtin
//! environment is separated from what a module costs to infer. The spans are
//! uf's own — the vendored port is a submodule and carries none — so the row
//! that ends up largest is the boundary uf hands work across, not a line
//! inside inference. That boundary is `check::infer_ast`, the single call into
//! `flow_typing::type_inference`, and on `packages/router/internal/runtime.js`
//! it is 89% of every allocation the check makes. What sits on either side of
//! it inside `check::infer_one` — the parse and the diagnostics — is 2.2%
//! between them, and the graph, the options and the project modules are 70
//! allocations for the whole run.
//!
//! Debug and release give identical allocation counts — the counter is in the
//! allocator, not in the optimiser — so this can be run without a release
//! build. Release is worth it for the wall clock and nothing else.

use std::path::Path;

use uf_check::{CheckCache, CheckLimits, Source, check_sources, check_sources_cached};
use uf_profiler::{
    AllocDelta, AllocSnapshot, CountingAllocator, Recorder, ReportConfig, SortBy, Window, scope,
};

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator::new();

fn main() {
    let Arguments {
        path,
        batch,
        cache,
        phases,
    } = Arguments::parse(std::env::args().skip(1));
    let source = std::fs::read_to_string(&path).expect("read the module");

    if !uf_check::is_available() {
        // Without `upstream-typecheck` every entry point returns
        // `CheckError::Unavailable` immediately, and the figures below would be
        // the cost of an early return rather than of a check. Say so instead of
        // printing zeroes that look like an improvement.
        eprintln!("built without `upstream-typecheck`: there is no checker to measure");
        std::process::exit(1);
    }

    // One `Source` per copy, each under its own path: the checker keys a module
    // by the path it is filed under, so N copies of one path is one module
    // checked N times and tells us nothing about the marginal cost.
    let paths: Vec<String> = (0..batch)
        .map(|index| {
            if index == 0 {
                path.clone()
            } else {
                copy_path(&path, index)
            }
        })
        .collect();
    let sources: Vec<Source<'_>> = paths
        .iter()
        .map(|path| Source::new(path, &source))
        .collect();

    println!("{path}: {} bytes", source.len());
    println!("batch          {batch}");

    if cache {
        cache_report(&sources);
        return;
    }

    // Warm up: the first call forces the builtin environment, and that is a
    // one-time cost of the process rather than of a check.
    let _ = check_sources(&sources, &[], &CheckLimits::default()).expect("checks");

    let runs = 3;
    let delta = measure(runs, || {
        let report = check_sources(&sources, &[], &CheckLimits::default()).expect("checks");
        std::hint::black_box(&report);
    });
    print_delta(&delta, runs);

    if phases {
        // A second pass rather than the one above: a span reads the
        // allocator's counters as it opens and closes, and the headline should
        // not be made to carry that.
        // `measure` turns the allocator off when it is done, and a span with
        // nothing counting reads four zeroes — so it goes back on around this
        // pass or the table has a time column and nothing else.
        CountingAllocator::enable();
        scope::enable();
        let mut recorder = Recorder::new("check_sources").with_config(ReportConfig {
            sort_by: SortBy::Allocations,
            ..ReportConfig::default()
        });
        for _ in 0..runs {
            recorder.record(|| {
                let report = check_sources(&sources, &[], &CheckLimits::default()).expect("checks");
                std::hint::black_box(report)
            });
        }
        let report = recorder.finish();
        scope::disable();
        CountingAllocator::disable();
        println!();
        println!("{}", report.render_table());
    }
}

/// The three calls the cache question is actually about.
///
/// A cold run against an empty directory, a warm one against what it wrote, and
/// a third with one character appended — same limits, same process. The second
/// number is what a caller saves by holding a cache; the third is what an
/// editor saves, which is a different number for a reason that is in the key.
fn cache_report(sources: &[Source<'_>]) {
    let root = tempfile::tempdir().expect("a project root to cache under");
    let Some(cache) = CheckCache::open(root.path()) else {
        // `CheckCache::open` returns `None` when the process cannot identify
        // its own binary, and a cache that cannot name its compiler is off in
        // both directions by design. Measuring it would report a cold run
        // twice and call the cache useless.
        eprintln!("this process cannot identify its own binary, so there is no cache to measure");
        std::process::exit(1);
    };

    // Warm up outside the cache, so the builtin environment is forced before
    // either measured run and neither is charged for it.
    let _ = check_sources(sources, &[], &CheckLimits::default()).expect("checks");

    let cold = measure(1, || {
        let report = check_sources_cached(sources, &[], &CheckLimits::default(), Some(&cache))
            .expect("checks");
        std::hint::black_box(&report);
    });
    let warm = measure(1, || {
        let report = check_sources_cached(sources, &[], &CheckLimits::default(), Some(&cache))
            .expect("checks");
        std::hint::black_box(&report);
    });

    // The editor's question, and the one the batch above cannot answer. A
    // keystroke changes the file's text, the text is in the key, so the record
    // written a moment ago is filed under a key this call does not ask for.
    // One appended character is the smallest edit there is; a larger one cannot
    // cost less.
    let edited_source = format!("{}\n", sources[0].source);
    let edited: Vec<Source<'_>> = sources
        .iter()
        .map(|source| Source::new(source.path, &edited_source))
        .collect();
    let typed = measure(1, || {
        let report = check_sources_cached(&edited, &[], &CheckLimits::default(), Some(&cache))
            .expect("checks");
        std::hint::black_box(&report);
    });

    println!("cold (writes the records)");
    print_delta(&cold, 1);
    println!("warm (reads them back)");
    print_delta(&warm, 1);
    println!("edited (one character appended)");
    print_delta(&typed, 1);
    print_saved("cache saves   ", &cold, &warm);
    print_saved("an edit saves ", &cold, &typed);
}

/// What the second run cost less than the first, as a share of the first.
fn print_saved(label: &str, cold: &AllocDelta, warm: &AllocDelta) {
    let saved = cold.allocations.saturating_sub(warm.allocations);
    let percent = if cold.allocations == 0 {
        0.0
    } else {
        saved as f64 * 100.0 / cold.allocations as f64
    };
    println!("{label} {saved} allocations ({percent:.1}%)");
}

/// Run `workload` `runs` times inside a counting window.
fn measure(runs: u64, mut workload: impl FnMut()) -> AllocDelta {
    CountingAllocator::enable();
    let window = Window::open();
    let before = AllocSnapshot::capture();
    for _ in 0..runs {
        workload();
    }
    let after = AllocSnapshot::capture();
    drop(window);
    CountingAllocator::disable();
    after.delta_from(&before)
}

fn print_delta(delta: &AllocDelta, runs: u64) {
    println!("allocs / run   {}", delta.allocations / runs);
    println!(
        "bytes / run    {:.2} MiB",
        delta.bytes_allocated as f64 / runs as f64 / (1024.0 * 1024.0)
    );
    println!(
        "peak           {:.2} MiB",
        delta.peak_above_baseline as f64 / (1024.0 * 1024.0)
    );
}

/// What the command line said.
struct Arguments {
    /// The module to measure.
    path: String,
    /// How many copies of it to hand the checker at once.
    batch: usize,
    /// Whether to run the three cached calls instead of the plain report.
    cache: bool,
    /// Whether to print the per-phase table as well as the headline.
    phases: bool,
}

impl Arguments {
    /// One pass, because two cannot agree about `--batch`.
    ///
    /// A pass that looks for the first argument not starting with `--` reads
    /// the `2` of `--batch 2` as the file to measure, and then fails on a
    /// module named `2`. That is only invisible while the path is written
    /// first, which is how it was written every time it was run by hand.
    fn parse(arguments: impl Iterator<Item = String>) -> Self {
        let mut path = None;
        let mut batch = 1;
        let mut cache = false;
        let mut phases = false;
        let mut arguments = arguments.peekable();
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--cache" => cache = true,
                "--phases" => phases = true,
                "--batch" => {
                    // Taken whatever it says, so a mistyped count is not
                    // silently a path as well as silently a batch of one.
                    batch = arguments
                        .next()
                        .and_then(|count| count.parse().ok())
                        .filter(|count| *count > 0)
                        .unwrap_or(1);
                }
                _ => {
                    if path.is_none() {
                        path = Some(argument);
                    }
                }
            }
        }
        Self {
            path: path.unwrap_or_else(|| String::from("packages/router/internal/runtime.js")),
            batch,
            cache,
            phases,
        }
    }
}

/// `runtime.js` for copy 0, `runtime.copy1.js` for copy 1.
///
/// Beside the original rather than in a temporary directory, so a relative
/// import inside the module resolves to the same file for every copy — which
/// is what makes them the same amount of work.
fn copy_path(path: &str, index: usize) -> String {
    let path = Path::new(path);
    let extension = path.extension().and_then(|extension| extension.to_str());
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("module");
    let name = match extension {
        Some(extension) => format!("{stem}.copy{index}.{extension}"),
        None => format!("{stem}.copy{index}"),
    };
    path.with_file_name(name).to_string_lossy().into_owned()
}
