//! A global allocator that counts what passes through it.
//!
//! `uf check` reports a number for inference and `uf fmt` one for a corpus
//! run, and neither says where the time went or what it cost in memory. A
//! wall-clock number tells you a change is slower; it does not tell you that
//! the reason is a `String` per node in a printer that had none. The roadmap
//! asks for exactly that distinction — "ban `String`, `format!`, and
//! allocation-heavy std helpers in parser/lint/router/test hot paths", and
//! "audit hot paths for unnecessary `.clone()` calls" — and neither is
//! checkable without counting allocations.
//!
//! # Why the counters are global
//!
//! An allocator is process-wide, so the counters are too. A scoped measurement
//! is therefore a snapshot and a delta ([`AllocSnapshot`], [`AllocCounter`])
//! rather than a reset: two threads measuring at once would otherwise clear
//! each other's baseline, and the second one's numbers would be quietly wrong.
//!
//! # Why every counter is `Relaxed`
//!
//! Nothing here orders anything else. Each counter is an independent tally
//! whose value is read after the work has finished, so the only guarantee it
//! needs is atomicity, which `Relaxed` gives. `AcqRel` on the allocation path
//! would put a fence between the program and its own memory for a number
//! nobody reads until later. The exception is `enabled`, which gates the rest
//! and is `Acquire`/`Release` so that a run turned on before a workload starts
//! is seen by it.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Power-of-two size classes tallied, `0..=2^31` bytes and everything above.
///
/// Thirty-two covers every allocation a 32-bit length can describe, which is
/// every allocation uf makes: the largest single thing it holds is a source
/// file, and a source file that does not fit in 2 GiB is not a file uf is
/// going to format.
const SIZE_CLASSES: usize = 32;

/// The counters, as one static so the allocator has a single thing to touch.
struct Counters {
    /// Whether the counters move at all. See the module comment.
    enabled: AtomicBool,
    allocations: AtomicU64,
    deallocations: AtomicU64,
    bytes_allocated: AtomicU64,
    bytes_deallocated: AtomicU64,
    live_bytes: AtomicU64,
    peak_live_bytes: AtomicU64,
    largest_allocation: AtomicU64,
    size_classes: [AtomicU64; SIZE_CLASSES],
}

impl Counters {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocations: AtomicU64::new(0),
            deallocations: AtomicU64::new(0),
            bytes_allocated: AtomicU64::new(0),
            bytes_deallocated: AtomicU64::new(0),
            live_bytes: AtomicU64::new(0),
            peak_live_bytes: AtomicU64::new(0),
            largest_allocation: AtomicU64::new(0),
            // `[AtomicU64::new(0); N]` needs `Copy`, which an atomic
            // deliberately is not, so the array is written out.
            size_classes: [const { AtomicU64::new(0) }; SIZE_CLASSES],
        }
    }

    /// Record one allocation of `size` bytes.
    #[inline]
    fn record_alloc(&self, size: usize) {
        if !self.enabled.load(Ordering::Acquire) {
            return;
        }
        let size = size as u64;
        self.allocations.fetch_add(1, Ordering::Relaxed);
        self.bytes_allocated.fetch_add(size, Ordering::Relaxed);
        self.size_classes[size_class(size)].fetch_add(1, Ordering::Relaxed);
        self.largest_allocation.fetch_max(size, Ordering::Relaxed);

        // `fetch_max` on the peak rather than a compare-and-swap loop: the
        // live total is already approximate under concurrency — two threads
        // allocating at once can both read a live figure that never existed
        // as a single instant — and a peak that is the maximum of the values
        // actually observed is the honest version of that. A CAS loop would
        // spin to produce a number no more true.
        let live = self.live_bytes.fetch_add(size, Ordering::Relaxed) + size;
        self.peak_live_bytes.fetch_max(live, Ordering::Relaxed);
    }

    /// Record one deallocation of `size` bytes.
    #[inline]
    fn record_dealloc(&self, size: usize) {
        if !self.enabled.load(Ordering::Acquire) {
            return;
        }
        let size = size as u64;
        self.deallocations.fetch_add(1, Ordering::Relaxed);
        self.bytes_deallocated.fetch_add(size, Ordering::Relaxed);
        // Saturating, because a deallocation whose allocation happened before
        // counting was turned on would otherwise wrap the live total to
        // eighteen quintillion and take the peak with it.
        let _ = self
            .live_bytes
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |live| {
                Some(live.saturating_sub(size))
            });
    }
}

static COUNTERS: Counters = Counters::new();

/// Which power-of-two bucket `size` falls in.
///
/// `0` and `1` share bucket 0, then one bucket per doubling. The bucket is the
/// bit width, which is one instruction rather than a loop.
#[inline]
fn size_class(size: u64) -> usize {
    if size <= 1 {
        return 0;
    }
    let class = (u64::BITS - (size - 1).leading_zeros()) as usize;
    if class >= SIZE_CLASSES {
        SIZE_CLASSES - 1
    } else {
        class
    }
}

/// A `#[global_allocator]` that counts.
///
/// ```no_run
/// # use uf_profiler::CountingAllocator;
/// #[global_allocator]
/// static GLOBAL: CountingAllocator = CountingAllocator::new();
/// ```
///
/// Installing it costs nothing until [`CountingAllocator::enable`] is called:
/// every path checks one relaxed-acquire boolean and otherwise forwards
/// straight to [`System`].
pub struct CountingAllocator;

impl CountingAllocator {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Start counting.
    pub fn enable() {
        COUNTERS.enabled.store(true, Ordering::Release);
    }

    /// Stop counting. Allocations still work; they stop being tallied.
    ///
    /// Worth turning off around the profiler's own reporting, which allocates
    /// a table and would otherwise appear in the measurement it is printing.
    pub fn disable() {
        COUNTERS.enabled.store(false, Ordering::Release);
    }

    /// Whether the counters are moving.
    #[must_use]
    pub fn is_enabled() -> bool {
        COUNTERS.enabled.load(Ordering::Acquire)
    }
}

impl Default for CountingAllocator {
    fn default() -> Self {
        Self::new()
    }
}

// SAFETY: every method forwards to `System`, which is a correct allocator, with
// the same layout it was given and the same pointer it returned. The counting
// around each call touches only atomics and allocates nothing, so it cannot
// re-enter the allocator — which is the one thing a `GlobalAlloc` wrapper must
// not do.
unsafe impl GlobalAlloc for CountingAllocator {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            COUNTERS.record_alloc(layout.size());
        }
        pointer
    }

    #[inline]
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        COUNTERS.record_dealloc(layout.size());
        unsafe { System.dealloc(pointer, layout) }
    }

    #[inline]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            COUNTERS.record_alloc(layout.size());
        }
        pointer
    }

    // Counted as what it is — a free of the old size and an allocation of the
    // new one — rather than as one allocation of the difference. A `Vec` that
    // doubles eight times did eight allocations, and a report that hid that
    // behind a single net figure would hide the thing worth fixing.
    #[inline]
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let moved = unsafe { System.realloc(pointer, layout, new_size) };
        if !moved.is_null() {
            COUNTERS.record_dealloc(layout.size());
            COUNTERS.record_alloc(new_size);
        }
        moved
    }
}

/// Every counter, read at one moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AllocSnapshot {
    pub allocations: u64,
    pub deallocations: u64,
    pub bytes_allocated: u64,
    pub bytes_deallocated: u64,
    pub live_bytes: u64,
    pub peak_live_bytes: u64,
    pub largest_allocation: u64,
    pub size_classes: [u64; SIZE_CLASSES],
}

impl AllocSnapshot {
    /// Read the counters.
    ///
    /// Not an atomic read of all of them at once — there is no such thing
    /// across this many words, and there does not need to be: a snapshot is
    /// taken outside the work it brackets, when nothing under measurement is
    /// running.
    #[must_use]
    pub fn capture() -> Self {
        let mut size_classes = [0_u64; SIZE_CLASSES];
        for (slot, counter) in size_classes.iter_mut().zip(COUNTERS.size_classes.iter()) {
            *slot = counter.load(Ordering::Relaxed);
        }
        Self {
            allocations: COUNTERS.allocations.load(Ordering::Relaxed),
            deallocations: COUNTERS.deallocations.load(Ordering::Relaxed),
            bytes_allocated: COUNTERS.bytes_allocated.load(Ordering::Relaxed),
            bytes_deallocated: COUNTERS.bytes_deallocated.load(Ordering::Relaxed),
            live_bytes: COUNTERS.live_bytes.load(Ordering::Relaxed),
            peak_live_bytes: COUNTERS.peak_live_bytes.load(Ordering::Relaxed),
            largest_allocation: COUNTERS.largest_allocation.load(Ordering::Relaxed),
            size_classes,
        }
    }

    /// What happened between `baseline` and this snapshot.
    #[must_use]
    pub fn delta_from(&self, baseline: &Self) -> AllocDelta {
        let mut size_classes = [0_u64; SIZE_CLASSES];
        for (index, slot) in size_classes.iter_mut().enumerate() {
            *slot = self.size_classes[index].saturating_sub(baseline.size_classes[index]);
        }
        AllocDelta {
            allocations: self.allocations.saturating_sub(baseline.allocations),
            deallocations: self.deallocations.saturating_sub(baseline.deallocations),
            bytes_allocated: self
                .bytes_allocated
                .saturating_sub(baseline.bytes_allocated),
            bytes_deallocated: self
                .bytes_deallocated
                .saturating_sub(baseline.bytes_deallocated),
            // The window's peak *above where it started*. The raw peak is a
            // high-water mark for the whole process and says nothing about
            // this window; what a caller wants is how much this workload
            // added on top of what was already live.
            peak_above_baseline: self.peak_live_bytes.saturating_sub(baseline.live_bytes),
            live_growth: self.live_bytes as i64 - baseline.live_bytes as i64,
            largest_allocation: self.largest_allocation.max(baseline.largest_allocation),
            size_classes,
        }
    }
}

/// The difference between two [`AllocSnapshot`]s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AllocDelta {
    pub allocations: u64,
    pub deallocations: u64,
    pub bytes_allocated: u64,
    pub bytes_deallocated: u64,
    /// How far live bytes rose above where the window started.
    pub peak_above_baseline: u64,
    /// Live bytes at the end minus at the start. Negative when the window
    /// freed more than it took, which is what a cache being dropped looks
    /// like — and a positive figure across a workload that should be
    /// balanced is a leak worth a name.
    pub live_growth: i64,
    pub largest_allocation: u64,
    pub size_classes: [u64; SIZE_CLASSES],
}

impl AllocDelta {
    /// Bytes that were taken and not given back, as a count rather than a
    /// signed growth.
    #[must_use]
    pub const fn retained_bytes(&self) -> u64 {
        self.bytes_allocated.saturating_sub(self.bytes_deallocated)
    }

    /// The `(size class, count)` pairs that are not zero, smallest first.
    ///
    /// A histogram is mostly zeroes and a report that printed all thirty-two
    /// rows would bury the two that matter.
    pub fn occupied_classes(&self) -> impl Iterator<Item = (u32, u64)> + '_ {
        self.size_classes
            .iter()
            .enumerate()
            .filter(|(_, count)| **count > 0)
            .map(|(class, count)| (class as u32, *count))
    }
}

/// A baseline taken at construction, for measuring a region.
///
/// The counterpart to [`AllocSnapshot`] for callers that want a delta and
/// never the absolute numbers — a span guard, say.
///
/// # Why this is not an [`AllocSnapshot`]
///
/// It was, and that made every span pay for reading all thirty-nine counters
/// twice, thirty-two of them a size-class histogram that
/// [`crate::ScopeRecord`] does not carry and no report prints per span. An
/// enter/drop pair measured 41.9 ns that way. Reading the four counters a span
/// delta is actually made of, it is a fraction of that — and the spans that
/// most need measuring are the ones in per-node loops, where forty nanoseconds
/// is not a measurement, it is the thing being measured.
///
/// The histogram is still captured per *window* by [`AllocSnapshot`], where it
/// is read twice an iteration rather than twice a span.
#[derive(Debug, Clone, Copy)]
pub struct AllocCounter {
    allocations: u64,
    bytes_allocated: u64,
    bytes_deallocated: u64,
    live_bytes: u64,
}

impl AllocCounter {
    /// Take the baseline: four relaxed loads.
    #[must_use]
    #[inline]
    pub fn start() -> Self {
        Self {
            allocations: COUNTERS.allocations.load(Ordering::Relaxed),
            bytes_allocated: COUNTERS.bytes_allocated.load(Ordering::Relaxed),
            bytes_deallocated: COUNTERS.bytes_deallocated.load(Ordering::Relaxed),
            live_bytes: COUNTERS.live_bytes.load(Ordering::Relaxed),
        }
    }

    /// What has happened since.
    ///
    /// `size_classes` is left empty and `largest_allocation` zero: neither is
    /// a per-span number, and putting a figure there that no span report reads
    /// would be a number every reader would believe.
    #[must_use]
    #[inline]
    pub fn delta(&self) -> AllocDelta {
        let allocations = COUNTERS.allocations.load(Ordering::Relaxed);
        let bytes_allocated = COUNTERS.bytes_allocated.load(Ordering::Relaxed);
        let bytes_deallocated = COUNTERS.bytes_deallocated.load(Ordering::Relaxed);
        let live_bytes = COUNTERS.live_bytes.load(Ordering::Relaxed);
        let peak_live_bytes = COUNTERS.peak_live_bytes.load(Ordering::Relaxed);
        AllocDelta {
            allocations: allocations.saturating_sub(self.allocations),
            deallocations: 0,
            bytes_allocated: bytes_allocated.saturating_sub(self.bytes_allocated),
            bytes_deallocated: bytes_deallocated.saturating_sub(self.bytes_deallocated),
            peak_above_baseline: peak_live_bytes.saturating_sub(self.live_bytes),
            live_growth: live_bytes as i64 - self.live_bytes as i64,
            largest_allocation: 0,
            size_classes: [0; SIZE_CLASSES],
        }
    }
}

#[cfg(test)]
mod tests;
