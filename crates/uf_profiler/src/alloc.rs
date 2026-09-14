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
//! # Two sets of counters
//!
//! An allocator is process-wide, so the first set of counters is too: every
//! allocation on every thread lands in the same atomics. That is what a
//! profile of a whole run wants — uf fans its work out across threads, and a
//! report that could not see them would see almost nothing uf does. A scoped
//! measurement over them is a snapshot and a delta ([`AllocSnapshot`],
//! [`AllocCounter`]) rather than a reset: two threads measuring at once would
//! otherwise clear each other's baseline, and the second one's numbers would
//! be quietly wrong.
//!
//! The second set belongs to each thread ([`ThreadWindow`]), and it exists
//! because the first cannot answer the question a test asks. A test wants what
//! *its* code allocated, and `cargo test` runs every test in a binary at once,
//! on as many threads as the machine has cores. A delta over the process-wide
//! counters is the test's own allocations plus whatever its neighbours
//! allocated in the same stretch. [`Window`] makes measurements exclusive; it
//! cannot make the process quiet, and neither can taking the quietest of
//! several windows — under load there is no quiet window to find, and the
//! smallest of eight noisy figures is still a noisy figure. A resolver loop
//! that makes 128 allocations read 7,239 that way on a 32-core CI runner
//! (ubugeeei-prod/uf#1015).
//!
//! A thread's own counters have no neighbours in them. What the measured code
//! hands to a thread of its own is counted by that thread handing its figure
//! back, explicitly, with its result — [`Handover`], the allocation half of
//! what [`crate::scope::flush_thread_spans`] does for spans.
//!
//! # Why the process-wide counters are `Relaxed`
//!
//! Nothing here orders anything else. Each counter is an independent tally
//! whose value is read after the work has finished, so the only guarantee it
//! needs is atomicity, which `Relaxed` gives. `AcqRel` on the allocation path
//! would put a fence between the program and its own memory for a number
//! nobody reads until later. The exception is `enabled`, which gates the rest
//! and is `Acquire`/`Release` so that a run turned on before a workload starts
//! is seen by it.
//!
//! # Why a thread's counters are `Cell`s in a `const` thread-local
//!
//! They are touched from inside `GlobalAlloc`, where the one thing a wrapper
//! must not do is allocate. A `thread_local!` with a `const` initialiser, over
//! a type with no `Drop`, is neither lazily initialised nor registered for
//! destruction, so on a platform with native thread-locals — macOS, Linux and
//! Windows all have them — reaching it is an address and nothing more. A lazy
//! or droppable thread-local would allocate its own bookkeeping on first use,
//! which is inside `alloc`, which would recurse.
//!
//! `Cell` rather than an atomic because nobody but the owning thread ever reads
//! or writes them: a worker's figure travels back as a value ([`HandedBack`])
//! and is added by the thread that receives it.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

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

/// One thread's own tally. See [`ThreadWindow`] and the module comment.
///
/// The same counters as [`Counters`], so that a thread's figure is an
/// [`AllocDelta`] like any other. A delta whose histogram nobody filled in
/// would be a column of zeroes every reader believed.
struct ThreadCounters {
    /// Whether this thread's allocations are tallied. Set while a
    /// [`ThreadWindow`] is open on the thread, and by nothing else.
    counting: Cell<bool>,
    allocations: Cell<u64>,
    deallocations: Cell<u64>,
    bytes_allocated: Cell<u64>,
    bytes_deallocated: Cell<u64>,
    live_bytes: Cell<u64>,
    peak_live_bytes: Cell<u64>,
    largest_allocation: Cell<u64>,
    size_classes: [Cell<u64>; SIZE_CLASSES],
}

impl ThreadCounters {
    const fn new() -> Self {
        Self {
            counting: Cell::new(false),
            allocations: Cell::new(0),
            deallocations: Cell::new(0),
            bytes_allocated: Cell::new(0),
            bytes_deallocated: Cell::new(0),
            live_bytes: Cell::new(0),
            peak_live_bytes: Cell::new(0),
            largest_allocation: Cell::new(0),
            size_classes: [const { Cell::new(0) }; SIZE_CLASSES],
        }
    }

    /// Record one allocation of `size` bytes, if this thread is counting.
    #[inline]
    fn record_alloc(&self, size: u64) {
        if !self.counting.get() {
            return;
        }
        add(&self.allocations, 1);
        add(&self.bytes_allocated, size);
        add(&self.size_classes[size_class(size)], 1);
        raise(&self.largest_allocation, size);
        let live = self.live_bytes.get().saturating_add(size);
        self.live_bytes.set(live);
        raise(&self.peak_live_bytes, live);
    }

    /// Record one deallocation of `size` bytes, if this thread is counting.
    #[inline]
    fn record_dealloc(&self, size: u64) {
        if !self.counting.get() {
            return;
        }
        add(&self.deallocations, 1);
        add(&self.bytes_deallocated, size);
        // Saturating for the process-wide reason, and for one of a thread's
        // own: a thread frees what other threads allocated as readily as what
        // it allocated itself.
        self.live_bytes
            .set(self.live_bytes.get().saturating_sub(size));
    }

    /// Every counter, read at one moment — which, for cells only this thread
    /// writes, really is one moment.
    fn snapshot(&self) -> AllocSnapshot {
        let mut size_classes = [0_u64; SIZE_CLASSES];
        for (slot, counter) in size_classes.iter_mut().zip(self.size_classes.iter()) {
            *slot = counter.get();
        }
        AllocSnapshot {
            allocations: self.allocations.get(),
            deallocations: self.deallocations.get(),
            bytes_allocated: self.bytes_allocated.get(),
            bytes_deallocated: self.bytes_deallocated.get(),
            live_bytes: self.live_bytes.get(),
            peak_live_bytes: self.peak_live_bytes.get(),
            largest_allocation: self.largest_allocation.get(),
            size_classes,
        }
    }

    /// [`rebase_peaks`], for this thread's high-water marks.
    fn rebase_peaks(&self) -> SavedPeaks {
        SavedPeaks {
            peak_live_bytes: self.peak_live_bytes.replace(self.live_bytes.get()),
            largest_allocation: self.largest_allocation.replace(0),
        }
    }

    /// [`restore_peaks`], for this thread's high-water marks.
    fn restore_peaks(&self, saved: SavedPeaks) {
        raise(&self.peak_live_bytes, saved.peak_live_bytes);
        raise(&self.largest_allocation, saved.largest_allocation);
    }

    /// Add a worker's figure to this thread's, as though this thread had made
    /// the worker's allocations itself — which, to whoever is measuring it, it
    /// did.
    ///
    /// Only while this thread is counting: a figure that arrives after the
    /// window it was meant for has closed belongs to nothing.
    ///
    /// The high-water mark is an estimate. The worker ran while this thread
    /// waited for it, so this thread's live bytes then were about what they
    /// are now, and the worker's peak stood on top of them.
    fn absorb(&self, worker: &AllocDelta) {
        if !self.counting.get() {
            return;
        }
        add(&self.allocations, worker.allocations);
        add(&self.deallocations, worker.deallocations);
        add(&self.bytes_allocated, worker.bytes_allocated);
        add(&self.bytes_deallocated, worker.bytes_deallocated);
        for (counter, count) in self.size_classes.iter().zip(worker.size_classes) {
            add(counter, count);
        }
        raise(&self.largest_allocation, worker.largest_allocation);
        let live = self.live_bytes.get();
        raise(
            &self.peak_live_bytes,
            live.saturating_add(worker.peak_above_baseline),
        );
        self.live_bytes
            .set(live.saturating_add_signed(worker.live_growth));
    }
}

/// Add to a thread's counter.
///
/// Wrapping rather than checked, because a panic inside a global allocator is
/// an abort and a debug build checks every `+`. Nothing makes 2^64
/// allocations.
#[inline]
fn add(counter: &Cell<u64>, amount: u64) {
    counter.set(counter.get().wrapping_add(amount));
}

/// Raise a thread's high-water mark to `value`, if `value` is higher.
#[inline]
fn raise(mark: &Cell<u64>, value: u64) {
    if value > mark.get() {
        mark.set(value);
    }
}

thread_local! {
    /// This thread's counters. Why `const`, and why `Cell`s: the module
    /// comment.
    static THREAD: ThreadCounters = const { ThreadCounters::new() };
}

/// Record one allocation, for the process and for the thread making it.
#[inline]
fn record_alloc(size: usize) {
    COUNTERS.record_alloc(size);
    // `try_with` rather than `with`, though it cannot fail for a `const`
    // thread-local with no `Drop`: were that ever to change, the failure would
    // be inside the allocator, where a missed count is the only acceptable way
    // to fail.
    let _ = THREAD.try_with(|thread| thread.record_alloc(size as u64));
}

/// Record one deallocation, for the process and for the thread making it.
#[inline]
fn record_dealloc(size: usize) {
    COUNTERS.record_dealloc(size);
    let _ = THREAD.try_with(|thread| thread.record_dealloc(size as u64));
}

/// The high-water marks a window or a span displaced when it opened.
///
/// `peak_live_bytes` and `largest_allocation` are maxima, and a maximum kept
/// since the process started answers a question nobody asked: it says the
/// biggest thing that ever happened, not the biggest thing that happened
/// *here*. Reading one as if it were window-scoped is wrong in a way that
/// looks right — the number is plausible, it is just somebody else's.
///
/// So a window rebases them on the way in and puts them back on the way out,
/// folding what it saw into what it displaced. A span nested inside another
/// does the same, which is what makes the pattern compose: the child measures
/// only its own stretch, and the parent, on restore, ends up with the maximum
/// over its own stretch *and* every child's. Two atomic swaps in, two out.
#[derive(Debug, Clone, Copy)]
pub struct SavedPeaks {
    peak_live_bytes: u64,
    largest_allocation: u64,
}

/// Start a fresh pair of high-water marks, returning the displaced ones.
///
/// The peak restarts at the live bytes standing now rather than at zero, so
/// that "peak above the baseline" is a difference between two numbers measured
/// the same way.
#[inline]
fn rebase_peaks() -> SavedPeaks {
    let live = COUNTERS.live_bytes.load(Ordering::Relaxed);
    SavedPeaks {
        peak_live_bytes: COUNTERS.peak_live_bytes.swap(live, Ordering::Relaxed),
        largest_allocation: COUNTERS.largest_allocation.swap(0, Ordering::Relaxed),
    }
}

/// Put back what [`rebase_peaks`] displaced, keeping the larger of the two.
///
/// `fetch_max` rather than a store: whoever is above wants the maximum over
/// its own stretch and this one's, and a plain store would throw away
/// whichever of the two was bigger.
#[inline]
fn restore_peaks(saved: SavedPeaks) {
    COUNTERS
        .peak_live_bytes
        .fetch_max(saved.peak_live_bytes, Ordering::Relaxed);
    COUNTERS
        .largest_allocation
        .fetch_max(saved.largest_allocation, Ordering::Relaxed);
}

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
/// Installing it costs next to nothing until something counts: every path
/// checks one relaxed-acquire boolean for the process-wide counters and one
/// thread-local `bool` for the thread's own, and otherwise forwards straight
/// to [`System`].
pub struct CountingAllocator;

impl CountingAllocator {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Start counting into the process-wide counters, on every thread.
    ///
    /// A [`ThreadWindow`] needs no switch: opening one is what turns its
    /// thread's counters on.
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

    /// Whether the process-wide counters are moving.
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
// around each call touches only atomics and this thread's own cells and
// allocates nothing, so it cannot re-enter the allocator — which is the one
// thing a `GlobalAlloc` wrapper must not do.
unsafe impl GlobalAlloc for CountingAllocator {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            record_alloc(layout.size());
        }
        pointer
    }

    #[inline]
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        record_dealloc(layout.size());
        unsafe { System.dealloc(pointer, layout) }
    }

    #[inline]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            record_alloc(layout.size());
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
            record_dealloc(layout.size());
            record_alloc(new_size);
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
    /// Read the process-wide counters.
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
            // Since the window opened, because that is when the counter was
            // last rebased. Taking `max` with the baseline's, as this once
            // did, could not do anything: the counter only rises, so the
            // later snapshot's value is always the larger and the window's
            // own largest was never computed at all.
            largest_allocation: self.largest_allocation,
            size_classes,
        }
    }
}

/// Held for as long as a measurement window is open. See [`Window::open`].
static WINDOW: Mutex<()> = Mutex::new(());

/// One measurement window: exclusive, and with high-water marks of its own.
///
/// Two things go wrong when windows overlap, and this fixes both by not
/// letting them.
///
/// The peaks are the first. They are process-wide maxima, so a window has to
/// rebase them to get a figure that is about itself (see [`SavedPeaks`]); two
/// windows rebasing each other's would each report the other's stretch.
///
/// The worker spans are the second. A window clears the shared span storage
/// when it opens, so that a span left over from earlier is not attributed to
/// it — and a second window opening midway through the first would clear the
/// spans the first had already collected, deleting worker time from a report
/// that gave no sign anything was missing.
///
/// So a window is exclusive. Opening one while another is open blocks until
/// that one closes, which for the profiler's callers — a benchmark harness, an
/// `alloc_report` example — is the behaviour they would have written by hand.
///
/// It is not what a test wants. Being exclusive stops two measurements from
/// disturbing each other; it does nothing about a neighbouring test that is
/// not measuring and is allocating anyway, and in a test binary there always
/// is one. A test measures with a [`ThreadWindow`].
pub struct Window {
    saved: SavedPeaks,
    /// Dropped last, after the peaks are back, so the next window opens onto a
    /// consistent pair.
    _exclusive: MutexGuard<'static, ()>,
}

impl Window {
    /// Open a window, waiting for any other to close first.
    ///
    /// A panic inside a window poisons nothing that matters here: the counters
    /// are plain integers and the next window rebases them anyway, so the
    /// poison is stepped over rather than propagated.
    #[must_use]
    pub fn open() -> Self {
        let exclusive = WINDOW
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Self {
            saved: rebase_peaks(),
            _exclusive: exclusive,
        }
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        restore_peaks(self.saved);
    }
}

/// A measurement of what one thread allocates, and of nothing else.
///
/// The counterpart to [`Window`] for code that shares its process with work it
/// is not measuring — which is every test, because `cargo test` runs a
/// binary's tests at once. It reads the counters of the thread that opened it,
/// so what another thread allocates meanwhile is not in the figure at all,
/// rather than in it less often.
///
/// What the measured code does on a thread of its own is that thread's, and
/// arrives in the figure only when the thread hands it back: see [`Handover`].
/// Measured without that, code that does its work elsewhere costs what
/// starting a thread costs.
///
/// Windows nest. An inner one measures its own stretch; the outer one, still
/// counting, covers its own stretch and the inner's, and gets its high-water
/// marks back the way [`SavedPeaks`] describes.
///
/// ```
/// # use uf_profiler::{CountingAllocator, ThreadWindow};
/// # #[global_allocator]
/// # static GLOBAL: CountingAllocator = CountingAllocator::new();
/// let window = ThreadWindow::open();
/// let buffers: Vec<Vec<u8>> = (0..8).map(|_| Vec::with_capacity(64)).collect();
/// std::hint::black_box(&buffers);
/// let delta = window.close();
///
/// // The outer vector and its eight buffers, whatever any other thread did.
/// assert_eq!(delta.allocations, 9);
/// ```
#[must_use = "a window counts until it is closed, and closing it is what says how much"]
pub struct ThreadWindow {
    baseline: AllocSnapshot,
    saved: SavedPeaks,
    /// Whether the thread was already counting, for an enclosing window, so
    /// that closing this one leaves the thread as it was found.
    was_counting: bool,
    /// A window is about the thread that opened it, so it cannot be sent to
    /// another thread to be closed there.
    _this_thread: PhantomData<*const ()>,
}

impl ThreadWindow {
    /// Start counting this thread's allocations.
    ///
    /// # Panics
    ///
    /// When [`CountingAllocator`] is not the global allocator. A window over
    /// counters that never move reads zero, and a budget that reads zero
    /// passes — the failure a measurement is least able to notice about
    /// itself. So the outermost window on a thread makes an allocation before
    /// anything else and checks that it was seen. That allocation is in no
    /// window's figure: nothing was counting on this thread before it, and a
    /// window opened inside another skips it, the outer one having already
    /// proved the same thing.
    pub fn open() -> Self {
        THREAD.with(|thread| {
            let was_counting = thread.counting.replace(true);
            if !was_counting {
                let before_probe = thread.allocations.get();
                drop(std::hint::black_box(Box::new(0_u8)));
                assert!(
                    thread.allocations.get() != before_probe,
                    "uf_profiler::ThreadWindow counts through \
                     uf_profiler::CountingAllocator, which is not this binary's \
                     #[global_allocator]; every figure it reported would be zero"
                );
            }
            let saved = thread.rebase_peaks();
            Self {
                baseline: thread.snapshot(),
                saved,
                was_counting,
                _this_thread: PhantomData,
            }
        })
    }

    /// Stop counting, and say what this thread allocated while the window was
    /// open: its own allocations, and every worker's handed back to it.
    #[must_use]
    pub fn close(self) -> AllocDelta {
        // `self` drops on the way out, which puts the thread's high-water
        // marks back and leaves it counting only if an enclosing window is.
        THREAD.with(|thread| thread.snapshot().delta_from(&self.baseline))
    }
}

impl Drop for ThreadWindow {
    /// Put the thread back the way the window found it: on `close`, and on a
    /// panic unwinding past a window nobody closed.
    fn drop(&mut self) {
        let _ = THREAD.try_with(|thread| {
            thread.restore_peaks(self.saved);
            thread.counting.set(self.was_counting);
        });
    }
}

/// What a worker needs in order to count toward a [`ThreadWindow`] on the
/// thread that handed it work: captured there, carried across, and answered
/// with the worker's figure on the way back.
///
/// A [`ThreadWindow`] counts one thread, and uf does much of its work on
/// threads of its own — the checker on one with room for Flow's recursion,
/// the linter's parse on another. Measured from the calling thread alone, a
/// whole `uf_check` call would cost what starting a thread costs. So the code
/// that starts the thread says, in three places, that the thread's work is
/// its caller's:
///
/// ```
/// # use uf_profiler::{CountingAllocator, HandedBack, Handover, ThreadWindow};
/// # #[global_allocator]
/// # static GLOBAL: CountingAllocator = CountingAllocator::new();
/// let window = ThreadWindow::open();
///
/// // Before starting the worker: whether anything here is counting.
/// let handover = Handover::capture();
/// let buffers = std::thread::scope(|scope| {
///     scope
///         // On the worker: count the work if the caller is counting.
///         .spawn(move || handover.run(|| vec![vec![0_u8; 64]; 8]))
///         .join()
///         // Back on the caller: the worker's allocations join its own.
///         .map(HandedBack::receive)
///         .expect("the worker runs")
/// });
///
/// let delta = window.close();
/// assert!(delta.allocations >= 9, "the worker's buffers are counted: {delta:?}");
/// # drop(buffers);
/// ```
///
/// # Why a figure comes back, rather than the worker writing into a shared one
///
/// A count that other threads write into is what made measuring a test through
/// the process-wide counters a coin toss: anything that can write into it
/// eventually does, including a worker that belongs to a different test. A
/// figure returned with the worker's result can only arrive where that result
/// does. It needs no lock and no atomics, and a worker that outlives its
/// caller's window has nobody's count to corrupt.
///
/// When nothing is counting — every run that is not a test or a profile —
/// `capture` reads one thread-local `bool` and `run` calls the work directly.
#[derive(Debug, Clone, Copy)]
pub struct Handover {
    counting: bool,
}

impl Handover {
    /// On the thread about to hand work off: whether a [`ThreadWindow`] is
    /// open here.
    #[must_use]
    pub fn capture() -> Self {
        Self {
            counting: THREAD
                .try_with(|thread| thread.counting.get())
                .unwrap_or(false),
        }
    }

    /// On the worker: run `work`, counting it if the handing thread was.
    pub fn run<R>(self, work: impl FnOnce() -> R) -> HandedBack<R> {
        if !self.counting {
            return HandedBack {
                value: work(),
                allocations: None,
            };
        }
        let window = ThreadWindow::open();
        let value = work();
        HandedBack {
            value,
            allocations: Some(window.close()),
        }
    }
}

/// A worker's result, carrying what the worker allocated to produce it.
#[derive(Debug)]
#[must_use = "a worker's allocations count only once `receive` adds them to the thread that joined it"]
pub struct HandedBack<R> {
    value: R,
    allocations: Option<AllocDelta>,
}

impl<R> HandedBack<R> {
    /// On the thread that joined the worker: add the worker's allocations to
    /// this thread's, and take the result.
    pub fn receive(self) -> R {
        if let Some(worker) = &self.allocations {
            let _ = THREAD.try_with(|thread| thread.absorb(worker));
        }
        self.value
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
/// delta is actually made of, the pair is 11.0 ns of a 44.4 ns guard — and the
/// spans that most need measuring are the ones in per-node loops, where forty
/// nanoseconds is not a measurement, it is the thing being measured.
///
/// The histogram is still captured per *window* by [`AllocSnapshot`], where it
/// is read twice an iteration rather than twice a span.
///
/// Two of those nanoseconds are the peak rebase, which is not free and is not
/// optional: without it the peak this reports is the largest thing the process
/// ever did, which is a number about somebody else's work. The clock pair
/// alone is 38.7 ns of the 44.4, so the correctness costs six per cent of a
/// span and none of the floor.
#[derive(Debug, Clone, Copy)]
pub struct AllocCounter {
    allocations: u64,
    bytes_allocated: u64,
    bytes_deallocated: u64,
    live_bytes: u64,
    /// The enclosing window's high-water marks, held until this span closes.
    saved: SavedPeaks,
}

impl AllocCounter {
    /// Take the baseline: four relaxed loads, and a fresh pair of peaks.
    ///
    /// The peaks are rebased rather than merely read, because a maximum is not
    /// a difference and cannot be turned into one by subtraction — see
    /// [`SavedPeaks`]. [`Self::delta`] puts back what this displaced, so a
    /// `start`/`delta` pair must be balanced, which [`crate::ScopeGuard`]
    /// guarantees by doing the second in `Drop`.
    #[must_use]
    #[inline]
    pub fn start() -> Self {
        Self {
            allocations: COUNTERS.allocations.load(Ordering::Relaxed),
            bytes_allocated: COUNTERS.bytes_allocated.load(Ordering::Relaxed),
            bytes_deallocated: COUNTERS.bytes_deallocated.load(Ordering::Relaxed),
            live_bytes: COUNTERS.live_bytes.load(Ordering::Relaxed),
            saved: rebase_peaks(),
        }
    }

    /// What has happened since, and the end of this span's stretch.
    ///
    /// Closes the window [`Self::start`] opened: the peak read here is the one
    /// this span drove, and the enclosing window's is restored around it. Call
    /// it once — a second call would report the peak since the first, against
    /// a baseline that has already been handed back.
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
        restore_peaks(self.saved);
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
