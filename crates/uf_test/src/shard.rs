//! Splitting one suite across machines, and putting it back together.
//!
//! `uf test --shard 2/3` runs the second of three parts of a suite and leaves a
//! record of what it ran; `uf test --merge-shards` reads every part's record
//! and writes the reports one run over the whole suite would have written. Both
//! halves live here, because each is only right if it agrees with the other
//! about what a partition is.
//!
//! # Deterministic, and weighted
//!
//! Shards run on machines that never talk to each other, so each has to arrive
//! at the same partition on its own. The partition is a pure function of the
//! schedule — every test file with its expected cost, in
//! [`crate::schedule_files`] order — and the shard count: each file, heaviest
//! first, goes to the shard with the least expected cost so far, the lowest
//! index on a tie. It is the longest-processing-time rule the workers inside one
//! run are fed by, applied one level up, so the shards finish at about the same
//! time rather than holding about the same number of files.
//!
//! The expected cost is the duration `.uf/test-timings.json` recorded for a
//! file, and its size when nothing was recorded. The same timings on every
//! shard — the same restored cache, or none at all — give the same partition.
//! A shard run leaves that file as it found it, so shards run one after another
//! in one checkout cut the same partition as well, and it is the merge that
//! records the whole suite's durations for the next split.
//!
//! # A partition that differs is refused, not merged
//!
//! Timings that differ between machines give partitions that differ: a file in
//! two shards, and another in none. So every record carries a fingerprint of
//! the schedule its shard was cut from, and [`merge_shards`] merges only
//! records that agree on it, that cover every shard exactly once, and that
//! between them ran every file once. A merge that would report a file twice,
//! or a green suite with a file missing, stops and says which records disagree.
//!
//! # What "the same report" means
//!
//! Test results merge exactly. The files, their records and the plan are the
//! unsharded run's, in the same order, and the counts are recomputed from them
//! by the counting a run itself does, so a merged JUnit document differs from an
//! unsharded one only in its durations.
//!
//! Coverage merges the way one run already merges its workers' documents: hits
//! are summed over the processes that loaded a module. What was covered, and
//! every ratio, is what one run reports. A count for code that runs when a
//! module *loads* can differ, because it is the number of processes that loaded
//! the module — which depends on how the files were spread over processes, as
//! it already does between `-j 1` and `-j 8` in a single run.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uf_infra::FxHashMap;

use crate::coverage::Coverage;
use crate::discovery::merge_plans;
use crate::report::{TestRunReport, TestSummary};
use crate::schedule::ScheduleEntry;

/// Most shards a suite can be split into.
///
/// Far past any CI matrix, and small enough that a count typed with a digit too
/// many is refused rather than obeyed.
pub const MAX_SHARDS: u32 = 1_024;

/// The version of [`ShardRecord`] this uf writes and reads.
///
/// A merge refuses a record of any other version rather than guess at its
/// shape: the records a merge reads were written by the uf that ran the shards,
/// and a CI cache can outlive an upgrade.
pub const SHARD_RECORD_VERSION: u32 = 1;

/// How much of an argument that is not a shard is repeated back.
const MAX_ECHOED_CHARS: usize = 32;

/// One part of a split suite: shard `index` of `count`, counting from one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "ShardFields", into = "ShardFields")]
pub struct Shard {
    index: u32,
    count: u32,
}

/// [`Shard`] as a record holds it, checked on the way in.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct ShardFields {
    index: u32,
    count: u32,
}

impl Shard {
    /// Shard `index` of `count`.
    ///
    /// # Errors
    ///
    /// [`ShardError::Count`] for a count of zero or past [`MAX_SHARDS`], and
    /// [`ShardError::Index`] for an index outside `1..=count`.
    pub fn new(index: u32, count: u32) -> Result<Self, ShardError> {
        if count == 0 || count > MAX_SHARDS {
            return Err(ShardError::Count(count));
        }
        if index == 0 || index > count {
            return Err(ShardError::Index { index, count });
        }
        Ok(Self { index, count })
    }

    /// Which shard this is, counting from one.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.index
    }

    /// How many shards the suite is split into.
    #[must_use]
    pub const fn count(self) -> u32 {
        self.count
    }
}

impl TryFrom<ShardFields> for Shard {
    type Error = ShardError;

    fn try_from(fields: ShardFields) -> Result<Self, ShardError> {
        Self::new(fields.index, fields.count)
    }
}

impl From<Shard> for ShardFields {
    fn from(shard: Shard) -> Self {
        Self {
            index: shard.index,
            count: shard.count,
        }
    }
}

impl FromStr for Shard {
    type Err = ShardError;

    /// `2/3`: which shard, a slash, and how many there are.
    fn from_str(text: &str) -> Result<Self, ShardError> {
        let malformed = || ShardError::Malformed(text.chars().take(MAX_ECHOED_CHARS).collect());
        let (index, count) = text.split_once('/').ok_or_else(malformed)?;
        let number = |digits: &str| {
            // Digits and nothing else: `u32::from_str` also takes a leading
            // `+`, and nobody writes a shard as `+1/3`.
            if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(malformed());
            }
            // Too many digits for a `u32` is too many shards, which
            // [`Shard::new`] says in its own words.
            Ok(digits.parse::<u32>().unwrap_or(u32::MAX))
        };
        Self::new(number(index)?, number(count)?)
    }
}

impl fmt::Display for Shard {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "{}/{}", self.index, self.count)
    }
}

/// Why a shard could not be named.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ShardError {
    /// Not `<index>/<count>`.
    #[error("`{0}` is not a shard: write which shard and how many there are, like `1/3`")]
    Malformed(String),
    /// A count of zero, or past [`MAX_SHARDS`].
    #[error("a suite can be split into 1 to {max} shards, not {0}", max = MAX_SHARDS)]
    Count(u32),
    /// An index outside `1..=count`.
    #[error("there is no shard {index} of {count}: shards are numbered from 1 to {count}")]
    Index {
        /// The shard asked for.
        index: u32,
        /// How many shards there are.
        count: u32,
    },
}

/// The entries of `schedule` that `shard` runs, in schedule order.
///
/// `schedule` has to be in [`crate::schedule_files`] order. That order is part
/// of the partition: a caller that sorted it another way would cut a different
/// one, and its shards would not merge with anybody else's.
#[must_use]
pub fn shard_files(schedule: &[ScheduleEntry], shard: Shard) -> Vec<&ScheduleEntry> {
    let mut loads: BinaryHeap<Reverse<(u64, u32)>> =
        (1..=shard.count).map(|index| Reverse((0, index))).collect();
    let mut chosen = Vec::new();
    for entry in schedule {
        let Some(Reverse((load, index))) = loads.pop() else {
            break;
        };
        if index == shard.index {
            chosen.push(entry);
        }
        // At least a microsecond, so files recorded at zero still spread out
        // instead of every one of them landing on the first shard of a tie.
        loads.push(Reverse((
            load.saturating_add(entry.weight_micros.max(1)),
            index,
        )));
    }
    chosen
}

/// A digest of what a partition was cut from: the shard count, and every file
/// in the schedule with its expected cost, in schedule order.
///
/// FNV-1a over 64 bits. It is not a security measure — the records come from
/// the project's own CI — but the way shards that never talked to each other
/// find out afterwards whether they cut the same suite.
#[must_use]
pub fn partition_fingerprint(schedule: &[ScheduleEntry], count: u32) -> String {
    let mut hash = Fnv::new();
    hash.write(&count.to_le_bytes());
    for entry in schedule {
        hash.write(entry.file.as_bytes());
        // A byte no path holds, so where one path ends is never in doubt.
        hash.write(&[0]);
        hash.write(&entry.weight_micros.to_le_bytes());
    }
    uf_infra::cstr!("{:016x}", hash.0).into_string()
}

/// FNV-1a over 64 bits.
struct Fnv(u64);

impl Fnv {
    const fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}

/// What one shard leaves behind for [`merge_shards`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShardRecord {
    /// [`SHARD_RECORD_VERSION`] of the uf that wrote it.
    pub version: u32,
    /// Which shard ran.
    pub shard: Shard,
    /// How many test files the whole suite scheduled, over every shard.
    pub suite_files: usize,
    /// [`partition_fingerprint`] of the schedule the shard was cut from.
    pub fingerprint: String,
    /// What the shard ran.
    pub report: TestRunReport,
    /// What it measured, when the run collected coverage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage: Option<Coverage>,
}

/// A suite put back together from its shards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergedShards {
    /// How many shards it was split into.
    pub count: u32,
    /// The fingerprint every record agreed on.
    pub fingerprint: String,
    /// The report one run over the whole suite makes, its durations aside.
    ///
    /// Its duration is the sum of the shards': what the suite cost, not how
    /// long any one machine waited.
    pub report: TestRunReport,
    /// Every shard's coverage, merged; `None` when no shard collected any.
    pub coverage: Option<Coverage>,
}

/// Why shard records could not be merged.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ShardMergeError {
    /// Nothing to merge.
    #[error("there are no shard records to merge")]
    Empty,
    /// A record from a uf that writes another version.
    #[error(
        "shard {shard} was recorded as version {found}, and this uf reads version {expected}: merge with the uf that ran the shards",
        expected = SHARD_RECORD_VERSION
    )]
    Version {
        /// The shard whose record it is.
        shard: Shard,
        /// The version it was written as.
        found: u32,
    },
    /// Records that disagree about how many shards there are.
    #[error("the records disagree about how many shards there are: {shards}")]
    Counts {
        /// Every record's shard, as written.
        shards: String,
    },
    /// One shard recorded twice.
    #[error("shard {shard} was recorded more than once")]
    Duplicate {
        /// The shard recorded twice.
        shard: Shard,
    },
    /// Shards with no record.
    #[error(
        "no record for shard {missing} of {count}: merge every shard, or the report leaves out the files those shards ran"
    )]
    Missing {
        /// The shards with no record, as a list.
        missing: String,
        /// How many shards there are.
        count: u32,
    },
    /// Records cut from different schedules.
    #[error(
        "the shards were cut from different suites (fingerprints {fingerprints}), so some files ran in two shards and some in none. Give every shard the same `.uf/test-timings.json` — the same restored cache, or none — and run them again"
    )]
    Fingerprints {
        /// Every fingerprint the records carry.
        fingerprints: String,
    },
    /// A file two shards both ran.
    #[error("{file} ran in shard {first} and in shard {second}")]
    Overlap {
        /// The file.
        file: String,
        /// The first shard that ran it.
        first: Shard,
        /// The other.
        second: Shard,
    },
    /// Fewer or more files than the suite scheduled.
    #[error(
        "the shards ran {ran} test files between them, and the suite they were cut from has {suite}"
    )]
    FileCount {
        /// How many the shards ran.
        ran: usize,
        /// How many the suite scheduled.
        suite: usize,
    },
    /// Coverage from some shards and not others.
    #[error(
        "shard {with} collected coverage and shard {without} did not: merge shards that agree on `--coverage`"
    )]
    MixedCoverage {
        /// A shard that collected coverage.
        with: Shard,
        /// A shard that did not.
        without: Shard,
    },
}

/// Put a split suite back together.
///
/// The records may arrive in any order. Every check that could make the merged
/// report say something the shards did not is made before anything is merged:
/// one version, one count, every shard exactly once, one fingerprint, one
/// answer about coverage, and every scheduled file reported by exactly one
/// shard.
///
/// # Errors
///
/// The first [`ShardMergeError`] the records fail.
pub fn merge_shards(mut records: Vec<ShardRecord>) -> Result<MergedShards, ShardMergeError> {
    records.sort_by_key(|record| (record.shard.count, record.shard.index));
    let Some(first) = records.first() else {
        return Err(ShardMergeError::Empty);
    };
    if let Some(record) = records
        .iter()
        .find(|record| record.version != SHARD_RECORD_VERSION)
    {
        return Err(ShardMergeError::Version {
            shard: record.shard,
            found: record.version,
        });
    }
    let count = first.shard.count;
    if records.iter().any(|record| record.shard.count != count) {
        let shards: Vec<String> = records
            .iter()
            .map(|record| record.shard.to_string())
            .collect();
        return Err(ShardMergeError::Counts {
            shards: shards.join(", "),
        });
    }
    if let Some(pair) = records
        .windows(2)
        .find(|pair| pair[0].shard == pair[1].shard)
    {
        return Err(ShardMergeError::Duplicate {
            shard: pair[0].shard,
        });
    }
    if records.len() != usize::try_from(count).unwrap_or(usize::MAX) {
        let missing: Vec<String> = (1..=count)
            .filter(|index| records.iter().all(|record| record.shard.index != *index))
            .map(|index| index.to_string())
            .collect();
        return Err(ShardMergeError::Missing {
            missing: missing.join(", "),
            count,
        });
    }
    let fingerprint = first.fingerprint.clone();
    let suite = first.suite_files;
    if records
        .iter()
        .any(|record| record.fingerprint != fingerprint || record.suite_files != suite)
    {
        let mut fingerprints: Vec<&str> = records
            .iter()
            .map(|record| record.fingerprint.as_str())
            .collect();
        fingerprints.sort_unstable();
        fingerprints.dedup();
        return Err(ShardMergeError::Fingerprints {
            fingerprints: fingerprints.join(", "),
        });
    }
    let with = records.iter().find(|record| record.coverage.is_some());
    let without = records.iter().find(|record| record.coverage.is_none());
    if let (Some(with), Some(without)) = (with, without) {
        return Err(ShardMergeError::MixedCoverage {
            with: with.shard,
            without: without.shard,
        });
    }

    let mut owners: FxHashMap<String, Shard> = FxHashMap::default();
    let mut files = Vec::new();
    let mut plans = Vec::with_capacity(records.len());
    let mut coverage: Option<Coverage> = None;
    let mut summary = TestSummary::default();
    for record in records {
        for file in &record.report.files {
            if let Some(first) = owners.insert(file.file.clone(), record.shard) {
                return Err(ShardMergeError::Overlap {
                    file: file.file.clone(),
                    first,
                    second: record.shard,
                });
            }
        }
        let ran = &record.report.summary;
        summary.scheduled_warm += ran.scheduled_warm;
        summary.scheduled_cold += ran.scheduled_cold;
        summary.duration_micros = summary.duration_micros.saturating_add(ran.duration_micros);
        summary.bailed |= ran.bailed;
        if let Some(measured) = &record.coverage {
            coverage.get_or_insert_with(Coverage::new).merge(measured);
        }
        files.extend(record.report.files);
        plans.push(record.report.plan);
    }
    if files.len() != suite {
        return Err(ShardMergeError::FileCount {
            ran: files.len(),
            suite,
        });
    }

    files.sort_by(|a, b| a.file.cmp(&b.file));
    let plan = merge_plans(plans);
    summary.files = files.len();
    summary.unsupported_declarations = plan.unsupported.len();
    summary.foreign_declarations = plan.foreign_count();
    summary.count_files(&files);
    Ok(MergedShards {
        count,
        fingerprint,
        report: TestRunReport {
            plan,
            files,
            summary,
        },
        coverage,
    })
}
