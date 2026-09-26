// @flow
//
// `@uniflowed/std/sync`: Go's `sync` and `errgroup`, for one thread.
//
// JavaScript has no data races, so half of Go's `sync` package has no reason
// to exist here — there is nothing to protect a `number` from. The other half
// is about *ordering*, and that half is missing entirely:
//
// * "wait until these N things have finished" — `WaitGroup`
// * "only one of these at a time" — `Mutex`
// * "at most N of these at a time" — `Semaphore`
// * "exactly once, however many callers race for it" — `once`
// * "all of these, and stop everything on the first failure" — `Group`
//
// `await Promise.all` answers the first and none of the rest. A single-threaded
// runtime interleaves at every `await`, so two `async` functions reading and
// writing the same record *do* interleave, and every one of these is a real
// hazard in a request handler rather than an import from a language with
// threads.
//
// # Why not `Promise.all` for `Group`
//
// `Promise.all` rejects on the first failure and leaves the rest running, with
// nothing to cancel them and nowhere for their failures to go — a task that
// rejects after the `all` has already rejected is an unhandled rejection, which
// on Node is a process that exits. `Promise.allSettled` fixes the reporting and
// still runs everything. `Group` is the third thing: bounded concurrency, the
// first failure cancels the rest through a signal every task is handed, and no
// rejection is ever unobserved because each is attached the moment the task
// starts.
//
// # Fairness
//
// Every queue here is FIFO. A waiter that arrived first is resumed first, so a
// `Mutex` under continuous contention cannot starve anybody. That is a choice
// rather than a consequence: resuming the most recent waiter is measurably
// faster on some workloads, and a lock somebody waits on forever is worse than
// one that is slightly slower for everybody.
//
// # Cost
//
// A `Mutex` or `Semaphore` with no contention costs one already-resolved
// promise per acquisition and never touches the queue. Under contention it
// costs one array entry and one promise per waiter. `WaitGroup.wait()` with a
// counter already at zero returns an already-resolved promise and allocates no
// waiter at all.

/**
 * What a caller does when it is finished with a lock or a permit.
 *
 * A function rather than a `release()` method on the lock, so the only way to
 * release is to be holding the thing that was handed out — there is no
 * `mutex.unlock()` for code that never locked it to call by mistake.
 *
 * Calling it twice releases once. That is not politeness: a `finally` that
 * releases, inside a function whose body already released, would otherwise wake
 * two waiters for one permit, and the second of them would run inside somebody
 * else's critical section. It is the single most common way a hand-written
 * semaphore goes wrong, so it is closed here rather than documented as a rule.
 */
export type Release = () => void;

/**
 * Wait for a set of tasks to finish, without collecting what they returned.
 *
 * Go's `sync.WaitGroup`, and the shape to reach for when the things being
 * waited on are not a list you can map over — a stream of jobs, a fan-out
 * started from inside a loop, work registered by a callback:
 *
 * ```js
 * const group = new WaitGroup();
 * for (const job of jobs) {
 *   group.add(1);
 *   run(job).finally(() => group.done());
 * }
 * await group.wait();
 * ```
 *
 * `add` before starting the work and `done` when it finishes, both of them
 * outside the task, which is what lets the counter go up from anywhere. When
 * the tasks *are* a list and their results are wanted, [`Group`] is the better
 * tool and `Promise.all` is often better still.
 */
export class WaitGroup {
  /** Outstanding work. `wait` resolves when this reaches zero. */
  #count: number = 0;
  /** Callers parked in `wait`, resumed together when the counter empties. */
  #waiting: Array<() => void> = [];

  /**
   * Add `delta` to the counter, defaulting to one.
   *
   * A negative delta is allowed — it is how `done` is implemented — and driving
   * the counter below zero throws. Go panics for the same reason: a counter
   * that has gone negative means a `done` without an `add`, so some *other*
   * `wait` in the program has already been told the work was finished when it
   * was not, and the failure has to be loud where it happened rather than
   * silent where it is observed.
   */
  add(delta?: number): void {
    const by = delta ?? 1;
    const next = this.#count + by;
    if (next < 0) {
      throw new Error("@uniflowed/std/sync: WaitGroup counter went negative");
    }
    this.#count = next;
    if (next === 0) {
      this.#release();
    }
  }

  /** Record one task as finished. `add(-1)`, spelled the way Go spells it. */
  done(): void {
    this.add(-1);
  }

  /** The number of tasks still outstanding. */
  count(): number {
    return this.#count;
  }

  /**
   * Resolve once the counter is zero.
   *
   * Immediately when it is already zero, which is the case a `wait` on an empty
   * group hits and is worth not allocating for. Several callers may wait at
   * once and all are resumed together; a group whose counter returns to zero,
   * rises and falls again resolves each `wait` at the first zero *after* that
   * caller asked, which is the only reading that does not depend on scheduling.
   */
  wait(): Promise<void> {
    if (this.#count === 0) {
      return Promise.resolve();
    }
    return new Promise((resolve) => {
      this.#waiting.push(resolve);
    });
  }

  /** Resume everybody parked in `wait`. */
  #release(): void {
    const waiting = this.#waiting;
    this.#waiting = [];
    for (const resume of waiting) {
      resume();
    }
  }
}

/**
 * At most `permits` holders at a time, FIFO.
 *
 * Go has no `Semaphore` in `sync` — it has a buffered channel, which is the
 * same thing — and `golang.org/x/sync/semaphore` is the name it goes by.
 * The uses are the ones where unbounded concurrency is the bug: a fan-out over
 * ten thousand URLs that opens ten thousand sockets, an import that runs one
 * database query per row.
 *
 * ```js
 * const limit = new Semaphore(8);
 * await Promise.all(urls.map((url) => limit.withPermit(() => fetch(url))));
 * ```
 *
 * [`Group`] is usually the better answer for exactly that example, because it
 * also cancels the rest when one fails. Reach for the semaphore when the limit
 * has to be shared between call sites that do not know about each other.
 */
export class Semaphore {
  /** Permits not currently held. */
  #free: number;
  /** Callers parked in `acquire`, in arrival order. */
  #waiting: Array<(release: Release) => void> = [];

  constructor(permits: number) {
    if (!Number.isInteger(permits) || permits < 1) {
      throw new Error(
        `@uniflowed/std/sync: a Semaphore needs at least one permit, got ${String(permits)}`,
      );
    }
    this.#free = permits;
  }

  /** Permits available right now. Zero means the next `acquire` will wait. */
  available(): number {
    return this.#free;
  }

  /** Callers currently parked in `acquire`. */
  waiting(): number {
    return this.#waiting.length;
  }

  /**
   * Take a permit, waiting if none is free, and return how to give it back.
   *
   * The caller is responsible for releasing, which means a `try`/`finally` —
   * and [`withPermit`] is that `try`/`finally` written once. Reach for this
   * directly only when the permit's lifetime is not a block: a stream that
   * holds one until it is closed, say.
   */
  acquire(): Promise<Release> {
    if (this.#free > 0) {
      this.#free -= 1;
      return Promise.resolve(this.#releaser());
    }
    return new Promise((resolve) => {
      this.#waiting.push(resolve);
    });
  }

  /**
   * Run `task` holding a permit, and release it however `task` ends.
   *
   * The result is `task`'s, at `task`'s type: `withPermit(() => fetch(url))`
   * is a `Promise<Response>` with nothing written down at the call site.
   */
  async withPermit<T>(task: () => Promise<T> | T): Promise<T> {
    const release = await this.acquire();
    try {
      return await task();
    } finally {
      release();
    }
  }

  /**
   * One permit's worth of release, spent at most once.
   *
   * The `spent` flag is what makes a double release a no-op rather than a
   * corrupted count; see [`Release`] for why that case is worth closing.
   */
  #releaser(): Release {
    let spent = false;
    return () => {
      if (spent) {
        return;
      }
      spent = true;
      const next = this.#waiting.shift();
      if (next === undefined) {
        this.#free += 1;
        return;
      }
      // The permit is handed straight to the next waiter rather than returned
      // to the pool and taken again: a round trip through `#free` would let a
      // caller arriving in between jump the queue, which is the starvation
      // this class says it does not have.
      next(this.#releaser());
    };
  }
}

/**
 * One holder at a time.
 *
 * Go's `sync.Mutex`. A `Semaphore` of one, and a different name because the
 * intent is different: a semaphore bounds a resource, a mutex protects an
 * invariant. What it protects on a single-threaded runtime is a sequence of
 * `await`s — a read-modify-write that yields in the middle is interleaved with
 * every other copy of itself, and no amount of JavaScript's single-threadedness
 * prevents that:
 *
 * ```js
 * // Two concurrent callers here lose one increment, every time.
 * const seen = await store.get(key);
 * await store.set(key, seen + 1);
 * ```
 *
 * There is no `RWMutex` here yet, deliberately: readers and writers cannot
 * actually run at the same time in one runtime, so the only thing it would buy
 * over this is a fairness policy, and it should be added when somebody has the
 * workload that needs one.
 */
export class Mutex {
  #semaphore: Semaphore = new Semaphore(1);

  /** Whether the mutex is held right now. */
  locked(): boolean {
    return this.#semaphore.available() === 0;
  }

  /** Callers currently waiting for it. */
  waiting(): number {
    return this.#semaphore.waiting();
  }

  /** Take the lock, waiting if it is held, and return how to release it. */
  lock(): Promise<Release> {
    return this.#semaphore.acquire();
  }

  /**
   * Run `task` holding the lock, and release it however `task` ends.
   *
   * The result is `task`'s, at `task`'s type.
   *
   * ```js
   * await counters.withLock(async () => {
   *   const seen = await store.get(key);
   *   await store.set(key, seen + 1);
   * });
   * ```
   *
   * Not re-entrant: a `withLock` inside a `withLock` on the same mutex waits
   * for a lock only its own caller can release, which is a deadlock. Go's mutex
   * is not re-entrant either, and for the same reason — a re-entrant lock lets
   * a function observe a half-finished invariant that its own caller was in the
   * middle of restoring.
   */
  withLock<T>(task: () => Promise<T> | T): Promise<T> {
    return this.#semaphore.withPermit(task);
  }
}

/**
 * Wrap `make` so it runs at most once, however many callers race for it.
 *
 * Go's `sync.Once`, as a function rather than a struct because JavaScript's
 * answer to "the thing I want to do once" is almost always a value:
 *
 * ```js
 * const connect = once(() => open(url));   // () => Promise<Connection>
 * // ...
 * const db = await connect();              // every caller gets the same one
 * ```
 *
 * The result is whatever `make` returned, at its type, including when that is a
 * promise: concurrent callers all receive the *same* promise, so `make` runs
 * once even if every caller arrives before it settles. That is the property a
 * hand-written `if (cached == null)` does not have, because the check and the
 * assignment are separated by an `await`.
 *
 * # A failure is remembered
 *
 * If `make` throws or rejects, every later call throws or rejects with the same
 * error rather than retrying. Go's `Once` behaves this way too, and it is the
 * conservative reading: a "run once" that quietly becomes "run once per
 * failure" is how a failing connection turns into a retry storm. Retrying is a
 * policy, and a policy belongs in the caller — wrap the retry, not the `once`.
 */
export function once<T>(make: () => T): () => T {
  let done = false;
  let value: T;
  let failure: mixed;
  let failed = false;
  return () => {
    if (!done) {
      done = true;
      try {
        value = make();
      } catch (error) {
        failed = true;
        failure = error;
      }
    }
    if (failed) {
      throw failure;
    }
    return value;
  };
}

/** What one of a [`Group`]'s tasks is handed. */
export type Task<T> = (signal: AbortSignal) => Promise<T> | T;

/** How a [`Group`] is bounded. */
export type GroupOptions = {
  /**
   * How many tasks may run at once. Unbounded when absent.
   *
   * A limit is the reason to reach for a group over `Promise.all`, so it is
   * worth setting even when the list is small today: the list that is fifty
   * items in development is the one that is fifty thousand in production.
   */
  readonly limit?: number,
};

/**
 * Run tasks together, bounded, and stop the rest when one fails.
 *
 * Go's `golang.org/x/sync/errgroup`, which is `WaitGroup` plus the two things
 * every use of a wait group turns out to want: the first error, and a context
 * that is cancelled when it happens.
 *
 * ```js
 * const group = new Group<Row>({ limit: 8 });
 * for (const id of ids) {
 *   group.go((signal) => fetchRow(id, signal));
 * }
 * const rows = await group.wait();   // $ReadOnlyArray<Row>, in submission order
 * ```
 *
 * `T` is inferred from the tasks, so `rows` is typed by what `fetchRow`
 * returns and nothing is annotated at the call.
 *
 * # What happens on the first failure
 *
 * Every task still running is told to stop, through the `signal` it was handed,
 * and any task still queued for a permit is dropped without being started.
 * Telling is all a group can do: JavaScript has no way to interrupt a function
 * that ignores its signal.
 *
 * So `wait` still waits for every task, and rejects with the first error once
 * the last one has finished. That is deliberate, and it is the difference from
 * `Promise.all`: a `wait` that rejected while its tasks were still running
 * would hand the caller a scope it believes is finished — which is how a test
 * tears down the database its own fixtures are still writing to. The results
 * of the tasks that finished after the failure are discarded, and their
 * failures are observed here rather than becoming unhandled rejections.
 *
 * A group is used once. After `wait` has been called, `go` throws rather than
 * silently starting work nobody will ever look at.
 */
export class Group<T> {
  #limit: Semaphore | null;
  #settled: Array<Promise<void>> = [];
  #results: Array<T> = [];
  #started: number = 0;
  #controller: AbortController = new AbortController();
  #failure: { readonly error: mixed } | null = null;
  #closed: boolean = false;

  constructor(options?: GroupOptions) {
    const limit = options?.limit;
    this.#limit = limit == null ? null : new Semaphore(limit);
  }

  /**
   * The signal every task is handed, aborted on the first failure.
   *
   * Exposed as well as passed so that work started *outside* a task — a shared
   * stream, a timer — can be cancelled with the group.
   */
  signal(): AbortSignal {
    return this.#controller.signal;
  }

  /**
   * Start `task`, at the position in the results its call order gives it.
   *
   * Returns nothing: a group's results come back from `wait`, in submission
   * order rather than completion order, so a caller reading `rows[3]` does not
   * have to know which task happened to finish third.
   */
  go(task: Task<T>): void {
    if (this.#closed) {
      throw new Error("@uniflowed/std/sync: Group.go after wait; make a new Group");
    }
    // The slot is reserved by index before the task runs, and written when it
    // finishes, so a task that finishes third still lands where its `go` put
    // it. Nothing is pushed here: an array entry that held a placeholder would
    // have to be typed `T | void`, which would put a `void` in the result type
    // every caller reads — an internal hole is not worth a public one.
    const index = this.#started;
    this.#started += 1;
    this.#settled.push(this.#run(task, index));
  }

  /** How many tasks have been started. */
  started(): number {
    return this.#started;
  }

  /**
   * Wait for every task, and answer with their results or the first failure.
   *
   * Rejects with the first error any task raised. Resolves with one entry per
   * `go`, in the order the `go` calls were made.
   */
  async wait(): Promise<$ReadOnlyArray<T>> {
    this.#closed = true;
    // Every entry resolves — `#run` catches — so this settles once all the
    // tasks have finished, including the ones that ignored the abort.
    await Promise.all(this.#settled);
    const failure = this.#failure;
    if (failure != null) {
      throw failure.error;
    }
    // Nothing was skipped, because a skip only happens after a failure and a
    // failure has already been thrown above. So every reserved index was
    // written and the array is dense.
    return this.#results;
  }

  /**
   * One task, from permit to result, never rejecting.
   *
   * The returned promise resolving is what `wait` counts; the task's own
   * failure is recorded rather than propagated, which is what keeps a failed
   * task from being an unhandled rejection in the window between it failing and
   * `wait` being awaited.
   */
  async #run(task: Task<T>, index: number): Promise<void> {
    const limit = this.#limit;
    const release = limit == null ? null : await limit.acquire();
    try {
      if (this.#controller.signal.aborted) {
        // Somebody failed while this one was queued for a permit. Running it
        // now would do work whose result `wait` has already decided to throw
        // away, and would hold a permit somebody else could use to finish.
        return;
      }
      this.#results[index] = await task(this.#controller.signal);
    } catch (error) {
      if (this.#failure == null) {
        this.#failure = { error };
        this.#controller.abort(error);
      }
    } finally {
      if (release != null) {
        release();
      }
    }
  }
}
