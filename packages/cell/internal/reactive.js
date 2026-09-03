// @flow
//
// Runtime-agnostic cells: the reactive core the rest of uf's state story is
// built on, as ordinary Flow-typed JavaScript with no native binding.
//
// # Why a second channel
//
// Every cell publishes on two channels, not one. `observers` is the internal
// channel a derived cell links itself to; it fires synchronously, always, and
// nothing may defer it. `listeners` is the channel application code and the
// React binding subscribe on; it is batchable.
//
// One channel is not enough. If invalidation were batched with notification,
// then inside `batch(...)` a `read` of a derived cell would return the value it
// held before the write — the cached value, still marked valid because its
// invalidation was sitting in the queue. Reads inside a batch would see the
// past. Splitting the channels means the graph is always consistent the
// instant a write returns, and only the *waking up* of subscribers is
// deferred.
//
// # Why derived cells recompute eagerly when something depends on them
//
// A derived cell with no observers and no listeners just marks itself dirty
// and recomputes on the next read: nobody is watching, so the work can wait.
// The moment something *is* watching, it recomputes immediately and compares
// the result with `Object.is`. That comparison is the point. Writing `42` over
// `42`, or recomputing a selector that filters the same rows out of a changed
// list, must not wake a React tree. Deferring the recompute would make that
// cutoff impossible: we would have to notify first and discover the value was
// unchanged afterwards, which is exactly the spurious render the cutoff
// exists to prevent.
//
// The cost is that a write runs the derives that depend on it. That is the
// honest price of the cutoff, and it is only paid for cells something is
// actually subscribed to.

export type CellScope = "client" | "server" | "react-render" | "async-resource";
export type Unsubscribe = () => void;
type Listener = () => void;

/** How far along a [`resource`]'s load is. */
export type ResourceStatus = "idle" | "pending" | "success" | "failure";

export type CellSnapshot<+T> = {
  +scope: CellScope,
  +value: T,
};

type CellCarrier<T> = {
  +__kind: "Cell",
  +scope: CellScope,
  +get: () => T,
  +set: (T) => void,
  +subscribe: (Listener) => Unsubscribe,
  +observe: (Listener) => Unsubscribe,
};

export opaque type Cell<T> = CellCarrier<T>;

/**
 * The dependency list the derived cell currently recomputing is collecting.
 *
 * A single module-level slot rather than a parameter threaded through every
 * read, because the whole point of automatic tracking is that a derive body is
 * plain JavaScript that knows nothing about being tracked.
 */
let tracking: null | Array<CellCarrier<any>> = null;

/** How many [`batch`] calls are open. */
let batchDepth = 0;

/** Listeners a batch has collected, de-duplicated by set membership. */
const pending: Set<Set<Listener>> = new Set();

function makeCell<T>(carrier: CellCarrier<T>): Cell<T> {
  return carrier;
}

function readCarrier<T>(source: Cell<T>): CellCarrier<T> {
  return (source: any);
}

/**
 * Record that the derive currently running read `source`.
 *
 * Called from inside each cell's own `get`, so a dependency is recorded no
 * matter which accessor reached the value — `read`, `snapshot`, or a derive
 * closing over the cell directly.
 */
function tracked<T>(source: CellCarrier<T>): void {
  if (tracking != null) {
    tracking.push(source);
  }
}

function subscribeTo(listeners: Set<Listener>, listener: Listener): Unsubscribe {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/**
 * Run the internal channel: synchronous, unbatchable, never re-entrant-unsafe.
 *
 * Iterating a copy because an observer may link or unlink cells as it runs,
 * and a `Set` mutated mid-iteration is how a reactive graph starts skipping
 * updates.
 */
function publish(observers: Set<Listener>): void {
  for (const observer of Array.from(observers)) {
    observer();
  }
}

/**
 * Run the application channel, or hand it to the open batch.
 *
 * The batch queue holds the *set* a listener belongs to, not the listener, so
 * that whether it still belongs is decided when the flush runs. Queueing the
 * listeners themselves meant an unsubscribe between the write and the flush
 * removed it from its set and left the queued copy behind — and React unmounts
 * a `useSyncExternalStore` subscriber by calling exactly that unsubscribe, so a
 * component unmounted inside `batch(…)` was called after it was torn down.
 */
function notify(listeners: Set<Listener>): void {
  if (listeners.size === 0) {
    return;
  }
  if (batchDepth > 0) {
    pending.add(listeners);
    return;
  }
  publish(listeners);
}

/**
 * Drain the batch queue.
 *
 * A loop rather than one pass: a listener is allowed to write, and the writes
 * it makes belong to the same flush rather than to a batch the caller never
 * opened.
 */
function flush(): void {
  while (pending.size > 0) {
    const due = Array.from(pending);
    pending.clear();
    for (const listeners of due) {
      // A copy, because a listener may subscribe or unsubscribe as it runs and
      // a `Set` mutated mid-iteration skips entries — but the membership test
      // is against the live set, so one that unsubscribed a moment ago is not
      // called.
      for (const listener of Array.from(listeners)) {
        if (listeners.has(listener)) {
          listener();
        }
      }
    }
  }
}

/**
 * Run `body`, waking subscribers once at the end instead of once per write.
 *
 * Reads inside the batch still see every write immediately — batching defers
 * notification, never consistency. Nesting is counted, so a batch inside a
 * batch flushes with the outermost one.
 */
export function batch<T>(body: () => T): T {
  batchDepth += 1;
  try {
    return body();
  } finally {
    batchDepth -= 1;
    if (batchDepth === 0) {
      flush();
    }
  }
}

/**
 * Run `body` without recording anything it reads as a dependency.
 *
 * The escape hatch for a derive that wants to *look at* a cell without waking
 * up when it changes — a counter that reads a configuration value, say.
 * Without it the only way to break a dependency is to not read the value,
 * which pushes the problem into the caller's data flow.
 */
export function untracked<T>(body: () => T): T {
  const outer = tracking;
  tracking = null;
  try {
    return body();
  } finally {
    tracking = outer;
  }
}

/**
 * Where each resource is in its load.
 *
 * Kept beside the cells rather than on them so that `Cell<T>` stays one shape:
 * a consumer that does not care about loading states never sees a status field
 * it has to refine away, and a resource is still just a cell everywhere a cell
 * is accepted.
 */
const statuses: WeakMap<CellCarrier<any>, () => ResourceStatus> = new WeakMap();

function readonlyWrite(scope: CellScope): empty {
  throw Error(`@uniflowed/cell ${scope} cells are read-only`);
}

/**
 * A cell holding a value directly: the only kind anything writes to.
 *
 * A write of a value the cell already holds is dropped. Equality is `Object.is`
 * rather than `===` so that writing `NaN` over `NaN` is also a no-op, and
 * because `Object.is` is what React itself compares state with — a cell that
 * disagreed with `useState` about what "changed" means would be a subtle
 * source of extra renders.
 */
export function cell<T>(value: T): Cell<T> {
  let current = value;
  const listeners: Set<Listener> = new Set();
  const observers: Set<Listener> = new Set();
  const carrier: CellCarrier<T> = {
    __kind: "Cell",
    scope: "client",
    get: () => {
      tracked(carrier);
      return current;
    },
    set: (next) => {
      if (Object.is(current, next)) {
        return;
      }
      current = next;
      publish(observers);
      notify(listeners);
    },
    subscribe: (listener) => subscribeTo(listeners, listener),
    observe: (observer) => subscribeTo(observers, observer),
  };
  return makeCell(carrier);
}

/**
 * A cell computed from other cells, which discovers what those are by running.
 *
 * There is no dependency array. `derive` is called with tracking on, every
 * cell it reads links itself to the result, and the links are rebuilt on each
 * recompute — so a derive that branches (`showAll ? read(all) : read(some)`)
 * depends on exactly what it actually read this time, not on the union of
 * everything it might read. A stale dependency array is not a mistake this API
 * lets anyone make.
 */
export function computed<T>(derive: () => T): Cell<T> {
  const listeners: Set<Listener> = new Set();
  const observers: Set<Listener> = new Set();
  let links: Array<Unsubscribe> = [];
  let cached: T;
  let valid = false;
  let computing = false;

  function release(): void {
    for (const unlink of links) {
      unlink();
    }
    links = [];
  }

  function recompute(): T {
    if (computing) {
      // Without this the stack overflows somewhere inside user code and the
      // report names a frame that has nothing to do with the mistake.
      throw Error("@uniflowed/cell computed cell depends on itself");
    }
    release();
    const found: Array<CellCarrier<any>> = [];
    const outer = tracking;
    tracking = found;
    computing = true;
    try {
      cached = derive();
    } finally {
      computing = false;
      tracking = outer;
    }
    for (const dependency of found) {
      links.push(dependency.observe(invalidate));
    }
    valid = true;
    return cached;
  }

  function invalidate(): void {
    if (!valid) {
      return;
    }
    if (observers.size === 0 && listeners.size === 0) {
      valid = false;
      return;
    }
    const previous = cached;
    valid = false;
    const next = recompute();
    if (Object.is(previous, next)) {
      return;
    }
    publish(observers);
    notify(listeners);
  }

  const carrier: CellCarrier<T> = {
    __kind: "Cell",
    scope: "react-render",
    get: () => {
      tracked(carrier);
      return valid ? cached : recompute();
    },
    set: () => readonlyWrite("react-render"),
    subscribe: (listener) => {
      // Subscribing has to make the cell live: an unread derived cell has no
      // links yet, so nothing would ever tell it to recompute.
      if (!valid) {
        recompute();
      }
      return subscribeTo(listeners, listener);
    },
    observe: (observer) => {
      if (!valid) {
        recompute();
      }
      return subscribeTo(observers, observer);
    },
  };
  return makeCell(carrier);
}

/**
 * A cell whose value arrives from a promise.
 *
 * The load starts on first contact — a read or a subscription — rather than at
 * construction, so a resource declared at module scope costs nothing until
 * something actually wants it. It reads as `null` while pending, which keeps
 * the type one `?T` instead of forcing every consumer through a status union;
 * [`status`] is there for the consumers that do care.
 *
 * A rejected load re-throws on every read. Swallowing it would turn a failed
 * fetch into an indistinguishable empty state, which is the bug this design
 * refuses to make easy.
 */
export function resource<T>(load: () => Promise<T>): Cell<?T> {
  let state: ResourceStatus = "idle";
  let current: ?T = null;
  let thrown: mixed = null;
  const listeners: Set<Listener> = new Set();
  const observers: Set<Listener> = new Set();

  function settle(next: ?T, status: ResourceStatus, error: mixed): void {
    state = status;
    current = next;
    thrown = error;
    publish(observers);
    notify(listeners);
  }

  function start(): void {
    if (state !== "idle") {
      return;
    }
    state = "pending";
    load().then(
      (value) => settle(value, "success", null),
      (error) => settle(null, "failure", error),
    );
  }

  const carrier: CellCarrier<?T> = {
    __kind: "Cell",
    scope: "async-resource",
    get: () => {
      tracked(carrier);
      if (state === "failure") {
        throw thrown;
      }
      start();
      return current;
    },
    set: (next) => settle(next, "success", null),
    subscribe: (listener) => {
      start();
      return subscribeTo(listeners, listener);
    },
    observe: (observer) => {
      start();
      return subscribeTo(observers, observer);
    },
  };
  statuses.set(carrier, () => state);
  return makeCell(carrier);
}

/**
 * The load state of a [`resource`], or `"success"` for any other cell.
 *
 * A plain cell always has its value, which is what `"success"` means here; the
 * alternative — returning `null` and making every caller handle a state that
 * cannot happen — buys nothing.
 */
export function status<T>(source: Cell<T>): ResourceStatus {
  const readStatus = statuses.get(readCarrier(source));
  return readStatus == null ? "success" : readStatus();
}

export function read<T>(source: Cell<T>): T {
  return readCarrier(source).get();
}

export function write<T>(source: Cell<T>, value: T): void {
  readCarrier(source).set(value);
}

/**
 * Write the result of `reduce` applied to the current value.
 *
 * The read and the write are one step so that concurrent updaters compose:
 * `update(count, (n) => n + 1)` twice in a row increments twice, where
 * `write(count, read(count) + 1)` twice against a stale binding would not.
 */
export function update<T>(source: Cell<T>, reduce: (T) => T): void {
  const carrier = readCarrier(source);
  carrier.set(untracked(() => reduce(carrier.get())));
}

export function subscribe<T>(source: Cell<T>, listener: () => void): Unsubscribe {
  return readCarrier(source).subscribe(listener);
}

export function snapshot<T>(source: Cell<T>): CellSnapshot<T> {
  const carrier = readCarrier(source);
  return { scope: carrier.scope, value: carrier.get() };
}
