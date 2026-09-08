//! Turning records into something a person or a script can read.
//!
//! A profile is only worth taking if the answer is legible, and the two
//! audiences want different things: a person wants the rows that matter,
//! sorted by the column that names the thing to fix; a CI job wants every
//! number, stable enough to compare against last week's.
//!
//! Both come out of the same [`Report`]. The JSON is written by hand rather
//! than through `serde` — the crate has no dependencies on purpose, and the
//! shape here is a handful of flat objects.

use std::cmp::Reverse;
use std::fmt::Write as _;
use std::time::Duration;

use crate::alloc::AllocDelta;
use crate::scope::ScopeRecord;

/// One run of the workload.
#[derive(Debug, Clone)]
pub struct IterationRecord {
    pub elapsed: Duration,
    pub allocations: AllocDelta,
    pub spans: Vec<ScopeRecord>,
}

/// Which column the rows are ordered by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortBy {
    /// Time in the span itself. The default, because it is the column that
    /// names what to fix — inclusive time always puts the root first, which
    /// every reader already knew.
    #[default]
    SelfTime,
    /// Time in the span and everything under it.
    Inclusive,
    /// Bytes allocated. The order to read in when the question is memory.
    Bytes,
    /// Number of allocations, which is the one that finds a `String` per node.
    Allocations,
}

/// How a report is rendered.
#[derive(Debug, Clone)]
pub struct ReportConfig {
    pub sort_by: SortBy,
    /// How many rows to draw. `None` draws all of them.
    pub rows: Option<usize>,
    /// What one enter/drop pair costs, from
    /// [`crate::scope::calibrate_overhead_ns`]. Zero leaves the overhead
    /// column out — a report that cannot measure its own cost should not
    /// print a column implying it did.
    pub overhead_ns: f64,
}

impl Default for ReportConfig {
    fn default() -> Self {
        Self {
            sort_by: SortBy::default(),
            rows: Some(20),
            overhead_ns: 0.0,
        }
    }
}

/// One span name across every iteration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpanAggregate {
    pub name: &'static str,
    pub hits: u64,
    pub inclusive: Duration,
    pub self_time: Duration,
    pub allocations: u64,
    pub bytes_allocated: u64,
    pub peak_above_baseline: u64,
    pub slowest: Duration,
}

/// A finished measurement.
#[derive(Debug, Clone)]
pub struct Report {
    pub label: String,
    pub iterations: usize,
    /// Wall clock per iteration, ascending — so the percentiles below are a
    /// lookup rather than a sort each time.
    pub elapsed: Vec<Duration>,
    pub allocations: AllocDelta,
    pub spans: Vec<SpanAggregate>,
    config: ReportConfig,
}

impl Report {
    /// Fold iterations into a report.
    #[must_use]
    pub fn from_iterations(
        label: impl Into<String>,
        iterations: Vec<IterationRecord>,
        config: ReportConfig,
    ) -> Self {
        let mut elapsed: Vec<Duration> = iterations.iter().map(|run| run.elapsed).collect();
        elapsed.sort_unstable();

        let mut allocations = AllocDelta::default();
        let mut spans: Vec<SpanAggregate> = Vec::new();
        for run in &iterations {
            allocations.allocations += run.allocations.allocations;
            allocations.deallocations += run.allocations.deallocations;
            allocations.bytes_allocated += run.allocations.bytes_allocated;
            allocations.bytes_deallocated += run.allocations.bytes_deallocated;
            // Summed, not maxed: live growth is a signed difference, and what
            // a reader wants across a repeated workload is what it left behind
            // in total. Leaving it out kept the report's own field at its
            // default, so every report said `"liveGrowth":0` and dropped the
            // live-memory line — including the runs that had a leak to show.
            allocations.live_growth += run.allocations.live_growth;
            allocations.peak_above_baseline = allocations
                .peak_above_baseline
                .max(run.allocations.peak_above_baseline);
            allocations.largest_allocation = allocations
                .largest_allocation
                .max(run.allocations.largest_allocation);
            for (index, count) in run.allocations.size_classes.iter().enumerate() {
                allocations.size_classes[index] += count;
            }
            for record in &run.spans {
                merge_span(&mut spans, record);
            }
        }

        let mut report = Self {
            label: label.into(),
            iterations: iterations.len(),
            elapsed,
            allocations,
            spans,
            config,
        };
        report.sort();
        report
    }

    fn sort(&mut self) {
        // Descending, so the row that matters is the first one a reader sees.
        match self.config.sort_by {
            SortBy::SelfTime => self.spans.sort_by_key(|span| Reverse(span.self_time)),
            SortBy::Inclusive => self.spans.sort_by_key(|span| Reverse(span.inclusive)),
            SortBy::Bytes => self.spans.sort_by_key(|span| Reverse(span.bytes_allocated)),
            SortBy::Allocations => self.spans.sort_by_key(|span| Reverse(span.allocations)),
        }
    }

    /// The fastest iteration, which is the one least disturbed by whatever
    /// else the machine was doing.
    #[must_use]
    pub fn fastest(&self) -> Duration {
        self.elapsed.first().copied().unwrap_or_default()
    }

    /// The slowest iteration.
    #[must_use]
    pub fn slowest(&self) -> Duration {
        self.elapsed.last().copied().unwrap_or_default()
    }

    /// The median iteration. Reported rather than the mean because one
    /// scheduler hiccup moves a mean and does not move this.
    #[must_use]
    pub fn median(&self) -> Duration {
        if self.elapsed.is_empty() {
            return Duration::ZERO;
        }
        self.elapsed[self.elapsed.len() / 2]
    }

    /// The mean, for the cases where total throughput is the question.
    #[must_use]
    pub fn mean(&self) -> Duration {
        if self.elapsed.is_empty() {
            return Duration::ZERO;
        }
        let total: Duration = self.elapsed.iter().sum();
        total / self.elapsed.len() as u32
    }

    /// The rows a render will draw, after sorting and the row limit.
    #[must_use]
    pub fn rows(&self) -> &[SpanAggregate] {
        match self.config.rows {
            Some(limit) if limit < self.spans.len() => &self.spans[..limit],
            _ => &self.spans,
        }
    }

    /// A table, for a person.
    #[must_use]
    pub fn render_table(&self) -> String {
        let mut out = String::with_capacity(128 * (self.spans.len() + 8));
        let _ = writeln!(out, "{} — {} iteration(s)", self.label, self.iterations);
        let _ = writeln!(
            out,
            "  fastest {}  median {}  mean {}  slowest {}",
            duration(self.fastest()),
            duration(self.median()),
            duration(self.mean()),
            duration(self.slowest())
        );
        let _ = writeln!(
            out,
            "  {} allocations, {} allocated, {} peak above baseline",
            self.allocations.allocations,
            bytes(self.allocations.bytes_allocated),
            bytes(self.allocations.peak_above_baseline)
        );
        if self.allocations.live_growth != 0 {
            let _ = writeln!(
                out,
                "  {} still live at the end",
                bytes(self.allocations.live_growth.unsigned_abs())
            );
        }

        if self.spans.is_empty() {
            out.push_str("\n  no spans recorded — is uf_profiler::scope::enable() on?\n");
            return out;
        }

        out.push('\n');
        let name_width = self
            .rows()
            .iter()
            .map(|row| row.name.len())
            .max()
            .unwrap_or(4)
            .max("span".len());
        let overhead = self.config.overhead_ns > 0.0;
        let _ = write!(
            out,
            "  {:<name_width$}  {:>7}  {:>10}  {:>10}  {:>10}  {:>9}",
            "span", "hits", "self", "total", "bytes", "allocs"
        );
        if overhead {
            let _ = write!(out, "  {:>9}", "overhead");
        }
        out.push('\n');

        for row in self.rows() {
            let _ = write!(
                out,
                "  {:<name_width$}  {:>7}  {:>10}  {:>10}  {:>10}  {:>9}",
                row.name,
                row.hits,
                duration(row.self_time),
                duration(row.inclusive),
                bytes(row.bytes_allocated),
                row.allocations
            );
            if overhead {
                let cost = Duration::from_nanos((row.hits as f64 * self.config.overhead_ns) as u64);
                let _ = write!(out, "  {:>9}", duration(cost));
            }
            out.push('\n');
        }
        if let Some(limit) = self.config.rows
            && self.spans.len() > limit
        {
            let _ = writeln!(out, "  … and {} more", self.spans.len() - limit);
        }
        out
    }

    /// JSON, for a script.
    ///
    /// Hand-written because the crate has no dependencies and this is a
    /// handful of flat objects. Every string that reaches it is a `'static`
    /// span name or the caller's label, so the escaping is the small one.
    #[must_use]
    pub fn render_json(&self) -> String {
        let mut out = String::with_capacity(96 * (self.spans.len() + 4));
        out.push('{');
        let _ = write!(out, "\"label\":\"{}\"", escape(&self.label));
        let _ = write!(out, ",\"iterations\":{}", self.iterations);
        let _ = write!(out, ",\"fastestNs\":{}", self.fastest().as_nanos());
        let _ = write!(out, ",\"medianNs\":{}", self.median().as_nanos());
        let _ = write!(out, ",\"meanNs\":{}", self.mean().as_nanos());
        let _ = write!(out, ",\"slowestNs\":{}", self.slowest().as_nanos());
        let _ = write!(out, ",\"allocations\":{}", self.allocations.allocations);
        let _ = write!(
            out,
            ",\"bytesAllocated\":{}",
            self.allocations.bytes_allocated
        );
        let _ = write!(
            out,
            ",\"peakAboveBaseline\":{}",
            self.allocations.peak_above_baseline
        );
        let _ = write!(out, ",\"liveGrowth\":{}", self.allocations.live_growth);
        out.push_str(",\"spans\":[");
        for (index, row) in self.spans.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let _ = write!(out, "{{\"name\":\"{}\"", escape(row.name));
            let _ = write!(out, ",\"hits\":{}", row.hits);
            let _ = write!(out, ",\"selfNs\":{}", row.self_time.as_nanos());
            let _ = write!(out, ",\"inclusiveNs\":{}", row.inclusive.as_nanos());
            let _ = write!(out, ",\"slowestNs\":{}", row.slowest.as_nanos());
            let _ = write!(out, ",\"allocations\":{}", row.allocations);
            let _ = write!(out, ",\"bytesAllocated\":{}", row.bytes_allocated);
            let _ = write!(out, ",\"peakAboveBaseline\":{}}}", row.peak_above_baseline);
        }
        out.push_str("]}");
        out
    }
}

fn merge_span(spans: &mut Vec<SpanAggregate>, record: &ScopeRecord) {
    if let Some(slot) = spans.iter_mut().find(|slot| slot.name == record.name) {
        slot.hits += record.hits;
        slot.inclusive += record.inclusive;
        slot.self_time += record.self_time;
        slot.allocations += record.allocations;
        slot.bytes_allocated += record.bytes_allocated;
        slot.peak_above_baseline = slot.peak_above_baseline.max(record.peak_above_baseline);
        slot.slowest = slot.slowest.max(record.slowest);
        return;
    }
    spans.push(SpanAggregate {
        name: record.name,
        hits: record.hits,
        inclusive: record.inclusive,
        self_time: record.self_time,
        allocations: record.allocations,
        bytes_allocated: record.bytes_allocated,
        peak_above_baseline: record.peak_above_baseline,
        slowest: record.slowest,
    });
}

/// A duration at a scale a reader can hold, three significant figures.
fn duration(value: Duration) -> String {
    let nanos = value.as_nanos();
    if nanos < 1_000 {
        return format!("{nanos} ns");
    }
    if nanos < 1_000_000 {
        return format!("{:.2} µs", nanos as f64 / 1_000.0);
    }
    if nanos < 1_000_000_000 {
        return format!("{:.2} ms", nanos as f64 / 1_000_000.0);
    }
    format!("{:.2} s", nanos as f64 / 1_000_000_000.0)
}

/// Bytes in the unit a reader would say them in.
fn bytes(value: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;
    if value < KIB {
        return format!("{value} B");
    }
    if value < MIB {
        return format!("{:.1} KiB", value as f64 / KIB as f64);
    }
    if value < GIB {
        return format!("{:.1} MiB", value as f64 / MIB as f64);
    }
    format!("{:.2} GiB", value as f64 / GIB as f64)
}

/// The JSON escapes a span name or a label could contain.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if control.is_control() => {
                let _ = write!(out, "\\u{:04x}", control as u32);
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests;
