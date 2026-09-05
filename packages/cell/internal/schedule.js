// @flow
//
// When subscribers are told, as opposed to when values change.
//
// This module knows nothing about cells. It holds a queue of notification
// thunks and a nesting depth, and that separation is deliberate: the graph
// decides *what* changed, and it decides that immediately and synchronously;
// this decides *when the outside world hears about it*, which is the only part
// a caller is allowed to defer.
//
// # Why consistency is never deferred, only notification
//
// The tempting design batches invalidation too — collect the whole write
// burst, then repair the graph at the end. It produces a graph that lies:
// inside `batch(...)`, a read of a derived cell would return the value it held
// before the write, because its invalidation is still sitting in the queue.
// Reads would see the past, and the past is exactly what an application inside
// a batch is trying to move away from.
//
// So `write` stamps the graph before it returns, and only the waking of
// subscribers is queued. `batch(() => { write(a, 1); read(b) })` sees `b`
// derived from `a === 1`, and the subscriber is woken once at the end.
//
// # Why the queue holds thunks, not nodes
//
// Deduplication is the point of the queue: two writes to the same cell in one
// batch must wake React once. A `Set` gives that for free as long as what goes
// in has a stable identity, so a node allocates one notification thunk on its
// first subscribe and re-queues that same function forever.
//
// # Why the flush is a loop
//
// A listener is allowed to write. Those writes belong to the flush that is
// already running rather than to a batch its author never opened, so the drain
// keeps going until the queue is empty rather than taking one pass. The
// re-entrancy guard is what makes that safe: a write from inside a listener
// queues its notifications and returns, instead of starting a second flush
// that would interleave with this one.

/** A node's stable notification thunk. */
export type Wake = () => void;

/** How many [`batch`] calls are open. */
let depth = 0;

/** Whether a drain is already running further up the stack. */
let draining = false;

const queue: Set<Wake> = new Set();

/** Queue a notification. It runs at the end of the batch, or of the write. */
export function enqueue(wake: Wake): void {
  queue.add(wake);
}

/**
 * Hold notifications until the matching [`release`].
 *
 * One counter for every reason to wait, and there are two: an explicit
 * `batch`, and the graph part-way through restructuring itself. They have to
 * be the same counter — an `onMount` that writes inside its own `batch` would
 * otherwise decide the coast was clear and flush into the middle of the
 * evaluation that mounted it, which is how a node ends up recomputing against
 * a dependency list it has not finished rebuilding.
 */
export function hold(): void {
  depth += 1;
}

/** Release one hold, and drain if it was the last. */
export function release(): void {
  depth -= 1;
  settleQueue();
}

/**
 * Run everything queued, unless a batch is open or a drain is already running.
 *
 * Called after every write and at the close of the outermost batch. Both are
 * "the graph is consistent again, tell whoever is listening".
 */
export function settleQueue(): void {
  if (depth > 0 || draining) {
    return;
  }
  draining = true;
  try {
    while (queue.size > 0) {
      const due = Array.from(queue);
      queue.clear();
      for (const wake of due) {
        wake();
      }
    }
  } finally {
    draining = false;
  }
}

/**
 * Run `body`, waking subscribers once at the end instead of once per write.
 *
 * Reads inside the batch still see every write immediately — batching defers
 * notification, never consistency. Nesting is counted, so a batch inside a
 * batch flushes with the outermost one, and the flush happens even when the
 * body throws: the writes it made before throwing are real, and subscribers
 * that never heard about them would render state the graph no longer holds.
 */
export function batch<T>(body: () => T): T {
  hold();
  try {
    return body();
  } finally {
    release();
  }
}
