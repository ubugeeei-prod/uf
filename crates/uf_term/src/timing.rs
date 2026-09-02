//! Phase timing: what a build spent its time on, and how long it took overall.

use std::time::{Duration, Instant};

use crate::text::{push_u32, push_usize};

/// One recorded phase of a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Phase {
    /// Short human label, e.g. `"routes"`.
    pub label: &'static str,
    /// How long the phase took.
    pub duration: Duration,
}

/// Records how long each phase of a command took.
///
/// The timer is monotonic and never reads the wall clock, so a system clock
/// adjustment mid-build cannot produce a negative or absurd duration.
#[derive(Debug)]
pub struct PhaseTimer {
    phases: Vec<Phase>,
    started: Instant,
    mark: Instant,
}

impl Default for PhaseTimer {
    fn default() -> Self {
        Self::start()
    }
}

impl PhaseTimer {
    /// Start timing, with the first phase beginning now.
    pub fn start() -> Self {
        let now = Instant::now();
        Self {
            phases: Vec::new(),
            started: now,
            mark: now,
        }
    }

    /// Close the current phase under `label` and open the next one.
    pub fn lap(&mut self, label: &'static str) -> Duration {
        let now = Instant::now();
        let duration = now.saturating_duration_since(self.mark);
        self.mark = now;
        self.phases.push(Phase { label, duration });
        duration
    }

    /// Run `body`, recording how long it took as `label`.
    pub fn measure<T>(&mut self, label: &'static str, body: impl FnOnce() -> T) -> T {
        self.mark = Instant::now();
        let value = body();
        self.lap(label);
        value
    }

    /// The phases recorded so far, in the order they ran.
    pub fn phases(&self) -> &[Phase] {
        &self.phases
    }

    /// Time since the timer started, including work outside any phase.
    pub fn total(&self) -> Duration {
        Instant::now().saturating_duration_since(self.started)
    }
}

/// Append a duration in the shortest form a reader can act on.
///
/// Sub-millisecond durations keep one decimal, because "0ms" tells a reader
/// nothing about whether a phase is worth optimising.
pub fn push_duration(out: &mut String, duration: Duration) {
    let nanos = duration.as_nanos();
    if nanos < 1_000 {
        push_usize(out, nanos as usize);
        out.push_str("ns");
        return;
    }
    if nanos < 1_000_000 {
        push_fixed(out, (nanos / 100) as u64, 1);
        out.push_str("µs");
        return;
    }
    if nanos < 1_000_000_000 {
        push_fixed(out, (nanos / 100_000) as u64, 1);
        out.push_str("ms");
        return;
    }
    let seconds = duration.as_secs();
    if seconds < 60 {
        push_fixed(out, (nanos / 10_000_000) as u64, 2);
        out.push('s');
        return;
    }
    push_usize(out, (seconds / 60) as usize);
    out.push_str("m ");
    let rest = seconds % 60;
    if rest < 10 {
        out.push('0');
    }
    push_usize(out, rest as usize);
    out.push('s');
}

/// Render `value` scaled by `10^decimals` as a fixed-point decimal.
fn push_fixed(out: &mut String, value: u64, decimals: u32) {
    let scale = 10u64.pow(decimals);
    push_usize(out, (value / scale) as usize);
    let fraction = value % scale;
    if fraction == 0 {
        return;
    }
    out.push('.');
    let mut divisor = scale / 10;
    while divisor > 0 {
        push_u32(out, ((fraction / divisor) % 10) as u32);
        divisor /= 10;
    }
    while out.ends_with('0') {
        out.pop();
    }
}

/// A duration rendered as a `String`, for callers that need an owned value.
pub fn format_duration(duration: Duration) -> String {
    let mut out = String::new();
    push_duration(&mut out, duration);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nanosecond_durations_keep_their_unit() {
        assert_eq!(format_duration(Duration::from_nanos(0)), "0ns");
        assert_eq!(format_duration(Duration::from_nanos(999)), "999ns");
    }

    #[test]
    fn microsecond_durations_keep_one_decimal() {
        assert_eq!(format_duration(Duration::from_nanos(1_000)), "1µs");
        assert_eq!(format_duration(Duration::from_nanos(1_500)), "1.5µs");
        assert_eq!(format_duration(Duration::from_nanos(999_900)), "999.9µs");
    }

    #[test]
    fn millisecond_durations_keep_one_decimal() {
        assert_eq!(format_duration(Duration::from_millis(1)), "1ms");
        assert_eq!(format_duration(Duration::from_micros(1_500)), "1.5ms");
        assert_eq!(format_duration(Duration::from_millis(999)), "999ms");
    }

    #[test]
    fn second_durations_keep_two_decimals() {
        assert_eq!(format_duration(Duration::from_secs(1)), "1s");
        assert_eq!(format_duration(Duration::from_millis(1_250)), "1.25s");
        assert_eq!(format_duration(Duration::from_millis(59_900)), "59.9s");
    }

    #[test]
    fn minute_durations_pad_their_seconds() {
        assert_eq!(format_duration(Duration::from_secs(60)), "1m 00s");
        assert_eq!(format_duration(Duration::from_secs(75)), "1m 15s");
        assert_eq!(format_duration(Duration::from_secs(3_605)), "60m 05s");
    }

    #[test]
    fn durations_never_render_an_empty_string() {
        for nanos in [0u64, 1, 999, 1_000, 999_999, 1_000_000, 1_000_000_000] {
            assert!(!format_duration(Duration::from_nanos(nanos)).is_empty());
        }
    }

    #[test]
    fn a_timer_records_phases_in_order() {
        let mut timer = PhaseTimer::start();
        timer.lap("config");
        timer.lap("routes");
        let value = timer.measure("analysis", || 41 + 1);

        assert_eq!(value, 42);
        let labels: Vec<_> = timer.phases().iter().map(|phase| phase.label).collect();
        assert_eq!(labels, ["config", "routes", "analysis"]);
    }

    #[test]
    fn a_fresh_timer_has_no_phases() {
        let timer = PhaseTimer::start();
        assert!(timer.phases().is_empty());
        assert!(timer.total() < Duration::from_secs(5));
    }

    #[test]
    fn measure_returns_the_body_result_and_records_a_phase() {
        let mut timer = PhaseTimer::default();
        let result: Result<u8, u8> = timer.measure("step", || Err(3));

        assert_eq!(result, Err(3));
        assert_eq!(timer.phases().len(), 1);
        assert_eq!(timer.phases()[0].label, "step");
    }
}
