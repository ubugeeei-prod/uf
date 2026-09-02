//! Deciding what order the files run in.
//!
//! Parallelism is the boring half of a fast runner; the schedule is the half
//! that decides how long the run actually takes. With `n` workers and files of
//! wildly different cost, the run finishes when the *last* file finishes, so
//! the only thing that matters is not starting the most expensive file last.
//!
//! This is longest-processing-time-first (LPT), the classic greedy schedule for
//! identical machines: sort by expected cost descending, hand each free worker
//! the next file. LPT's makespan is within 4/3 - 1/(3n) of optimal, and unlike a
//! smarter schedule it needs no lookahead and no coordination between workers.
//!
//! The expected cost comes from [`crate::timings`] when the file has been seen
//! before, and from its size when it has not. The two are put on one scale so a
//! partially warm cache still produces one ordering rather than two tiers.

use compact_str::CompactString;
use serde::{Deserialize, Serialize};

use crate::timings::TestTimings;

/// Estimated nanoseconds of scanning per source byte on a cold run.
///
/// Discovery and execution are single-pass byte scans that measure in the low
/// hundreds of MB/s, so a byte costs a handful of nanoseconds. The constant only
/// has to rank files against each other and against recorded microseconds; it is
/// not a prediction of wall-clock time.
pub const COLD_NANOS_PER_BYTE: u64 = 5;

/// Where a file's expected cost came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScheduleBasis {
    /// A duration recorded by a previous run.
    Recorded,
    /// The file's size, because no duration was recorded.
    Size,
}

/// One file's place in the schedule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleEntry {
    /// Index of the file in the caller's slice.
    pub index: usize,
    /// Relative source file path.
    pub file: CompactString,
    /// Expected cost in microseconds.
    pub weight_micros: u64,
    /// Where that expectation came from.
    pub basis: ScheduleBasis,
}

/// Order files longest-expected-first.
///
/// Ties break on the path, so the order is a pure function of its inputs: two
/// runs over the same suite with the same recorded timings produce the same
/// schedule, which is what makes a parallel run reproducible enough to debug.
pub fn schedule_files(files: &[(&str, &str)], timings: &TestTimings) -> Vec<ScheduleEntry> {
    let mut entries: Vec<ScheduleEntry> = files
        .iter()
        .enumerate()
        .map(|(index, (file, source))| {
            let (weight_micros, basis) = match timings.get(file) {
                Some(micros) => (micros, ScheduleBasis::Recorded),
                None => (cold_weight_micros(source.len()), ScheduleBasis::Size),
            };
            ScheduleEntry {
                index,
                file: CompactString::from(*file),
                weight_micros,
                basis,
            }
        })
        .collect();

    entries.sort_by(|a, b| {
        b.weight_micros
            .cmp(&a.weight_micros)
            .then(a.file.cmp(&b.file))
    });
    entries
}

/// The cold estimate for a file of `bytes` bytes, in microseconds.
///
/// Saturating: a caller cannot overflow the schedule weight by handing over an
/// absurd size, and every file is worth at least one microsecond so that empty
/// files still order deterministically by path.
pub fn cold_weight_micros(bytes: usize) -> u64 {
    let nanos = (bytes as u64).saturating_mul(COLD_NANOS_PER_BYTE);
    (nanos / 1_000).max(1)
}

/// The length of the critical path of a schedule on `workers` workers, in
/// microseconds: the finish time of the last worker under LPT.
///
/// Used by the benchmark and by the tests that assert LPT actually shortens the
/// makespan compared with source order.
pub fn makespan_micros(entries: &[ScheduleEntry], workers: usize) -> u64 {
    let workers = workers.max(1);
    let mut finish = vec![0u64; workers];
    for entry in entries {
        let Some(earliest) = finish.iter_mut().min_by_key(|value| **value) else {
            break;
        };
        *earliest = earliest.saturating_add(entry.weight_micros);
    }
    finish.into_iter().max().unwrap_or(0)
}
