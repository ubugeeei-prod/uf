// @flow
//
// The dependency graph: what a cell is, how it learns what it depends on, and
// how one write reaches everything that cares — exactly once, with no
// intermediate value ever observed.
//
// This module owns every mutation of a node. The constructors beside it
// (`source.js`, `derived.js`, `resource.js`) decide *what* a node computes;
// this file decides *when*, and is the only place that assigns to a field.
//
// # Why marking and computing are separate passes
//
// The obvious reactive graph recomputes a derived cell the moment one of its
// dependencies changes. It is also wrong, and the shape that breaks it is a
// diamond:
//
//     a → b ┐
//     a → c ┴→ d
//
// Writing `a` reaches `b`, which recomputes and pushes to `d` — but `c` has
// not been touched yet, so `d` recomputes against the *new* `b` and the *old*
// `c`. That value never existed as far as the application is concerned. Then
// `c` recomputes and `d` runs a second time. Two recomputes, two
// notifications, and the first of each carried a value that is not a function
// of any single state of the world. The eager implementation this replaced did
// exactly that: `d` ran twice per write and its subscriber was woken twice.
//
// So a write does not compute anything. It stamps the graph:
//
//   1. the written cell bumps its `version`,
//   2. its direct observers are marked `DIRTY` — "a dependency of yours
//      definitely changed",
//   3. everything downstream of those is marked `CHECK` — "something upstream
//      *may* have changed",
//   4. any node with listeners is queued for notification.
//
// Nothing is evaluated. Values are then *pulled* — by a read, or by the flush
// that wakes subscribers — and a pull resolves a node's dependencies before
// the node itself. `d` is therefore computed once, after both `b` and `c` are
// current, and it is impossible for it to observe a half-updated graph.
//
// # Why versions, and not a boolean
//
// `CHECK` is the interesting mark. It says a *transitive* dependency changed,
// which is not the same as this node needing to run: `b` may have recomputed
// to the value it already had. So a `CHECK` node compares each dependency's
// `version` against the version it recorded when it last ran, and recomputes
// only if one actually moved. A version moves only when the committed value
// (or error, or resource status) really changed under the node's own equality.
//
// That is what makes the equality cutoff *propagate*. A selector over a list
// that still counts three rows does not bump its version, so nothing below it
// runs and no React tree wakes up — even though the list itself changed.
//
// # Why an unwatched node is never CLEAN
//
// A node is *watched* when something depends on it: a listener, or a derived
// node that is itself watched. Only watched nodes install back edges
// (`observers`), because a back edge from a long-lived source to a short-lived
// selector is a leak — the source would keep every selector ever derived from
// it alive.
//
// An unwatched node therefore never gets marked, so it may not trust a `CLEAN`
// stamp. It rests at `CHECK` instead: reading it walks its recorded
// dependencies and compares versions, which is cheap and cannot be stale.
// Repeated reads of an unchanged graph still return the memoised value without
// running the derive.
//
// # Why mounting is a cascade
//
// Becoming watched is what installs back edges, and it has to reach all the
// way up: subscribing to `d` must make `b`, `c` and `a` push to their
// observers, or `d` would never be told. Losing the last watcher tears the
// same chain down, which is also when `onMount` teardowns run — the moment
// nothing is listening is the moment a subscription to the outside world
// should stop.

import { enqueue, hold, release, settleQueue } from "./schedule.js";

/** Which side of the application a cell's value belongs to. */
export type CellScope = "client" | "server" | "react-render" | "async-resource";

/** What [`subscribe`] hands back. Calling it twice is a no-op. */
export type Unsubscribe = () => void;

/** A subscriber on the application channel. */
export type Listener = () => void;

/** How far along a [`resource`]'s load is. */
export type ResourceStatus = "idle" | "pending" | "success" | "failure";

/** What a cell holds, and where that value belongs. */
export type CellSnapshot<out T> = {
  readonly scope: CellScope,
  readonly value: T,
};

/**
 * What every cell constructor accepts.
 *
 * `equals` replaces `Object.is` for this cell alone. It is the hook for a
 * derived cell that produces a fresh array each run: without it, a selector
 * returning `rows.filter(...)` wakes its readers on every recompute because a
 * new array is never `Object.is` to the previous one.
 *
 * `onMount` runs when the cell gains its first watcher and its return value
 * runs when it loses its last, which is the only lifecycle a cell has. It is
 * given the cell itself, so a mount that has to feed values in — a socket, an
 * interval, a media query — can write to what it mounted.
 */
export type CellOptions<T> = {
  readonly equals?: (previous: T, next: T) => boolean,
  readonly onMount?: (self: Cell<T>) => void | (() => void),
};

/**
 * What a node does when it is stale, or `null` for a cell that holds a value
 * directly and so is never stale.
 */
type Evaluate<T> = null | ((self: Cell<T>) => T);

/** Staleness. Ordered, so `DIRTY` supersedes `CHECK` in a single comparison. */
const CLEAN = 0;
const CHECK = 1;
const DIRTY = 2;

/**
 * The absence of a thrown value.
 *
 * A sentinel rather than `null`, because `throw null` is legal JavaScript and
 * a derive that does it must still report a failure rather than a success.
 */
const NOTHING: mixed = Symbol("@uniflowed/cell nothing");

/**
 * A node whose value type is not known here.
 *
 * The graph is heterogeneous by nature: a node's dependencies and observers are
 * nodes of every type at once, and the machinery that walks them — marking,
 * pulling, linking — never looks at a value, only at versions and edges. Flow
 * has no existential to say that, and `Node<mixed>` is not it: a node's `value`
 * is mutable, so `Node` is invariant in `T` and `Node<number>` is not a
 * `Node<mixed>`.
 *
 * This is the one place `any` appears in the package. Everything that does look
 * at a value — `readNode`, `writeNode`, `commit` — is generic in `T` and never
 * sees this type.
 */
type AnyNode = Node<any>;

type Node<T> = {
  readonly kind: "source" | "derived" | "resource",
  readonly scope: CellScope,
  readonly equals: (previous: T, next: T) => boolean,
  readonly evaluate: Evaluate<T>,
  readonly onMount: null | ((self: Cell<T>) => void | (() => void)),

  /** The last committed value. For a resource, the last settled one. */
  value: T,
  /**
   * Whether `value` is a value at all. A node that runs a function has a
   * placeholder until it has run, and a custom `equals` must never be shown it.
   */
  hasValue: boolean,
  /** What the last evaluation threw, or [`NOTHING`]. */
  thrown: mixed,
  /** Where a resource is in its load; `"success"` for everything else. */
  status: ResourceStatus,
  /** The status at the last commit, so a status change bumps the version. */
  committedStatus: ResourceStatus,

  /**
   * Bumped only when the committed value, error or status actually changed.
   * Dependents compare it against what they recorded to decide whether a
   * `CHECK` mark is real work or a false alarm.
   */
  version: number,
  /**
   * How many times something superseded whatever asynchronous work this node
   * started. A settlement carrying an older generation is dropped, which is
   * what stops a slow load from overwriting the fast one that replaced it.
   */
  generation: number,
  state: 0 | 1 | 2,

  /** What the last evaluation read, and the versions it read them at. */
  deps: Array<AnyNode>,
  depVersions: Array<number>,
  /** The tracking pass that last recorded this node, for de-duplication. */
  epoch: number,

  /** Derived nodes that watch this one. Installed only while watched. */
  observers: Set<AnyNode>,
  listeners: Set<Listener>,
  /** This node's stable notification thunk, allocated on first subscribe. */
  wake: null | (() => void),
  /** The version the listeners were last told about. */
  notifiedVersion: number,

  watching: boolean,
  running: boolean,
  teardown: null | (() => void),
};

/**
 * A reactive value.
 *
 * Invariant in `T`: a `Cell<Dog>` is not a `Cell<Animal>`, because anything
 * holding the second may write a `Cat` into it. `crates/uf_lib/tests/flow`
 * keeps the fixture that records this.
 */
export opaque type Cell<T> = Node<T>;

/**
 * The dependency list the evaluation currently running is collecting.
 *
 * A module-level slot rather than an argument threaded through every read,
 * because the whole point of automatic tracking is that a derive body is plain
 * JavaScript that knows nothing about being tracked.
 */
let tracking: null | Array<AnyNode> = null;

/**
 * Which tracking pass `tracking` belongs to.
 *
 * Stamped onto each node as it is recorded, which de-duplicates a dependency
 * read twice in one pass without an `indexOf` scan, and — because epochs only
 * ever increase — lets [`relink`] tell a dependency that survived this pass
 * from one that did not.
 */
let trackingEpoch = 0;
let epochs = 0;

function defaultEquals<T>(previous: T, next: T): boolean {
  return Object.is(previous, next);
}

/**
 * Create a node. The only way one comes into existence.
 *
 * A node with an `evaluate` starts `DIRTY`: it has never run, and `CHECK`
 * would find no recorded dependencies and wrongly conclude there was nothing
 * to do.
 */
export function createNode<T>(config: {
  readonly kind: "source" | "derived" | "resource",
  readonly scope: CellScope,
  readonly value: T,
  readonly status?: ResourceStatus,
  readonly evaluate?: Evaluate<T>,
  readonly options?: void | CellOptions<T>,
}): Cell<T> {
  const evaluate = config.evaluate ?? null;
  const status = config.status ?? "success";
  const options = config.options;
  return {
    kind: config.kind,
    scope: config.scope,
    equals: options?.equals ?? defaultEquals,
    evaluate,
    onMount: options?.onMount ?? null,
    value: config.value,
    hasValue: evaluate === null,
    thrown: NOTHING,
    status,
    committedStatus: status,
    version: 0,
    generation: 0,
    state: evaluate === null ? CLEAN : DIRTY,
    deps: [],
    depVersions: [],
    epoch: -1,
    observers: new Set(),
    listeners: new Set(),
    wake: null,
    notifiedVersion: 0,
    watching: false,
    running: false,
    teardown: null,
  };
}

/**
 * Run `body` without recording anything it reads as a dependency.
 *
 * The escape hatch for a derive that wants to *look at* a cell without waking
 * when it changes. Without it the only way to break a dependency is to not
 * read the value, which pushes the problem into the caller's data flow.
 */
export function untracked<T>(body: () => T): T {
  const outerFrame = tracking;
  const outerEpoch = trackingEpoch;
  tracking = null;
  try {
    return body();
  } finally {
    tracking = outerFrame;
    trackingEpoch = outerEpoch;
  }
}

/** Record that the evaluation currently running read `node`. */
function track<T>(node: Node<T>): void {
  const frame = tracking;
  if (frame === null || node.epoch === trackingEpoch) {
    return;
  }
  node.epoch = trackingEpoch;
  frame.push(node);
}

function isWatched<T>(node: Node<T>): boolean {
  return node.listeners.size > 0 || node.observers.size > 0;
}

/**
 * Bring `node` up to date, without ever throwing.
 *
 * A failure is committed as the node's value the same way a success is — see
 * [`commit`] — because this runs during a flush, and a flush that unwound on
 * one broken derive would leave every subscriber after it un-notified. The
 * error surfaces at [`readNode`], where a caller is in a position to handle it.
 */
function pull<T>(node: Node<T>): void {
  if (node.evaluate === null || node.state === CLEAN) {
    return;
  }
  if (node.state === CHECK) {
    let stale = false;
    for (let index = 0; index < node.deps.length; index += 1) {
      const dependency = node.deps[index];
      pull(dependency);
      if (dependency.version !== node.depVersions[index]) {
        stale = true;
        break;
      }
    }
    if (!stale) {
      // Never `CLEAN` while unwatched: nothing marks an unwatched node, so the
      // next read has to check its dependencies again. See the module docs.
      node.state = node.watching ? CLEAN : CHECK;
      return;
    }
  }
  evaluateNode(node);
}

function evaluateNode<T>(node: Node<T>): void {
  const evaluate = node.evaluate;
  if (evaluate === null) {
    return;
  }
  if (node.running) {
    // Without this the stack overflows somewhere inside user code and the
    // report names a frame that has nothing to do with the mistake.
    throw Error("@uniflowed/cell computed cell depends on itself");
  }

  const outerFrame = tracking;
  const outerEpoch = trackingEpoch;
  const found: Array<AnyNode> = [];
  epochs += 1;
  const epoch = epochs;

  // An evaluation restructures the graph, and can cause a write on the way: an
  // `onMount` that already has a value feeds it in as it is linked up. Holding
  // notifications until the evaluation is finished is what stops a subscriber
  // being woken into a node that is halfway through committing an older value,
  // and its dependency list halfway through being rebuilt.
  hold();
  try {
    tracking = found;
    trackingEpoch = epoch;
    node.running = true;
    node.generation += 1;

    let value = node.value;
    let thrown: mixed = NOTHING;
    try {
      value = evaluate(node);
    } catch (error) {
      thrown = error;
    } finally {
      node.running = false;
      tracking = outerFrame;
      trackingEpoch = outerEpoch;
    }

    // Clean as of the value just produced. Anything that marks the node from
    // here on is real work: the body has finished reading, so a write it did
    // not see — an `onMount` firing as a dependency below is linked, say — has
    // to survive as a mark rather than be stamped over on the way out.
    node.state = CLEAN;
    // The dependencies are recorded even when the evaluation threw: a derive
    // that failed on the value it read must run again when that value changes,
    // and a node with no recorded dependencies would never hear about it.
    relink(node, found, epoch);
    commit(node, value, thrown);
    if (node.state === CLEAN && !node.watching) {
      node.state = CHECK;
    }
  } finally {
    release();
  }
}

/**
 * Replace the node's dependency edges with what it just read.
 *
 * New edges are attached before stale ones are dropped. The other order tears
 * down a dependency that both passes share — running its `onMount` teardown
 * and restarting whatever it had subscribed to — for no reason at all.
 */
function relink<T>(node: Node<T>, found: Array<AnyNode>, epoch: number): void {
  // Versions are read before any edge is installed. Attaching can mount a
  // dependency, and a mount may write; recording afterwards would file that
  // write as "already accounted for" in a value computed before it happened.
  //
  // The epoch is stamped again here, and that is not redundant. Tracking
  // stamped it during the body, but an evaluation nested inside the body — a
  // derived dependency being pulled — runs its own pass with a higher epoch and
  // stamps anything *it* read, including cells this node read first. Deciding
  // what to unlink from a stamp that a nested pass has since overwritten drops
  // an edge this node is still using, and nothing marks it again. Re-stamping
  // after every nested pass has finished is what makes the test below the loop
  // mean "read by this node, this time".
  const versions: Array<number> = [];
  for (const dependency of found) {
    versions.push(dependency.version);
    dependency.epoch = epoch;
  }
  if (node.watching) {
    for (const dependency of found) {
      // A derive that read itself has already thrown; linking it to itself
      // would additionally leave a self-observer that marks forever.
      if (dependency !== node) {
        attach(dependency, node);
      }
    }
    for (const dependency of node.deps) {
      // Anything still read this pass carries this pass's epoch. Anything that
      // does not was read by an earlier pass and is no longer a dependency:
      // this is where `flag ? read(x) : read(y)` stops depending on `y`.
      if (dependency.epoch !== epoch) {
        detach(dependency, node);
      }
    }
  }
  node.deps = found;
  node.depVersions = versions;
}

/**
 * Record a new settled state, and stamp the graph if it differs from the last.
 *
 * The version is what dependents compare, so it must move exactly when
 * something observable moved: the value under this cell's own equality, the
 * error it threw, or — for a resource — the status of its load, which is
 * observable through [`status`] even when the value it holds is unchanged.
 */
function commit<T>(node: Node<T>, value: T, thrown: mixed): void {
  const settled = thrown === NOTHING;
  const changed =
    !node.hasValue ||
    !Object.is(node.thrown, thrown) ||
    node.status !== node.committedStatus ||
    (settled && !node.equals(node.value, value));

  if (!changed) {
    // The value is not adopted either. Under a custom `equals` the two are
    // different objects that the cell has been told to treat as one, and
    // keeping the first is what gives its readers a stable reference: a
    // selector that rebuilds an array of the same rows must hand back the
    // array it handed back last time, or every memoised consumer below it
    // re-renders for a change the cell just said did not happen.
    return;
  }
  node.hasValue = true;
  node.thrown = thrown;
  node.committedStatus = node.status;
  if (settled) {
    node.value = value;
  }
  node.version += 1;
  propagate(node);
}

/** Stamp everything downstream of a node whose value just changed. */
function propagate<T>(node: Node<T>): void {
  for (const observer of Array.from(node.observers)) {
    markStale(observer, DIRTY);
  }
  if (node.listeners.size > 0) {
    enqueue(wakeFor(node));
  }
}

/**
 * Mark a node, and everything below it, as possibly out of date.
 *
 * Descendants get `CHECK` rather than `DIRTY` because that is all that is
 * known about them: whether they have real work to do depends on whether this
 * node's value actually changes when it runs, which has not happened yet.
 *
 * The `state >= level` test is what keeps this linear. A node already marked
 * has already marked its own descendants, so a diamond stamps each node once
 * however many paths reach it.
 */
function markStale<T>(node: Node<T>, level: 1 | 2): void {
  if (node.running) {
    // A dependency committing while this node's body is running is the normal
    // case — the body pulled it and is reading the value it just produced.
    // Marking the reader for a value it already has is what turns one write
    // into two evaluations of the same node.
    return;
  }
  if (node.state >= level) {
    return;
  }
  const wasClean = node.state === CLEAN;
  node.state = level;
  if (!wasClean) {
    return;
  }
  for (const observer of Array.from(node.observers)) {
    markStale(observer, CHECK);
  }
  if (node.listeners.size > 0) {
    enqueue(wakeFor(node));
  }
}

/**
 * The node's own notification thunk, allocated once and reused.
 *
 * The batch queue is a `Set`, so it can only collapse repeated notifications
 * of one node if the thing queued has a stable identity. A fresh closure per
 * write would queue the same node twice for two writes and wake React twice.
 */
function wakeFor<T>(node: Node<T>): () => void {
  const existing = node.wake;
  if (existing !== null) {
    return existing;
  }
  const created = () => {
    notifyNode(node);
  };
  node.wake = created;
  return created;
}

/**
 * Wake a node's listeners, if what they were last told is out of date.
 *
 * The value is pulled first, so a listener that reads during its callback sees
 * a settled graph, and the version comparison happens against the value the
 * pull produced — that comparison is the whole equality cutoff as far as
 * subscribers are concerned.
 *
 * Iterating a copy, because a listener may subscribe or unsubscribe as it
 * runs and a `Set` mutated mid-iteration skips entries. The membership test is
 * against the live set, so one that unsubscribed a moment ago is not called:
 * React unmounts a `useSyncExternalStore` subscriber by calling exactly that
 * unsubscribe, and an unmount can happen inside a batch.
 */
function notifyNode<T>(node: Node<T>): void {
  pull(node);
  if (node.version === node.notifiedVersion) {
    return;
  }
  node.notifiedVersion = node.version;
  for (const listener of Array.from(node.listeners)) {
    if (node.listeners.has(listener)) {
      listener();
    }
  }
}

/**
 * Install the back edges that let a change reach `node`, and run its mount.
 *
 * `watching` is set before anything else runs, so a mount that subscribes to
 * the cell it was given — the ordinary way to persist a value, or to mirror it
 * somewhere — does not re-enter this function.
 */
function mount<T>(node: Node<T>): void {
  node.watching = true;
  pull(node);
  for (const dependency of node.deps) {
    attach(dependency, node);
  }
  const onMount = node.onMount;
  if (onMount === null) {
    return;
  }
  const teardown = onMount(node);
  node.teardown = typeof teardown === "function" ? teardown : null;
}

/**
 * Drop the back edges and run the mount's teardown.
 *
 * `watching` is cleared first for the same reason it is set first in
 * [`mount`]: a teardown that unsubscribes the listener its mount installed
 * would otherwise arrive back here through [`subscribeNode`]'s unsubscribe.
 */
function unmount<T>(node: Node<T>): void {
  node.watching = false;
  const teardown = node.teardown;
  node.teardown = null;
  if (teardown !== null) {
    teardown();
  }
  for (const dependency of node.deps) {
    detach(dependency, node);
  }
  if (node.state === CLEAN) {
    node.state = CHECK;
  }
}

function attach<T>(dependency: AnyNode, observer: Node<T>): void {
  const watched = isWatched(dependency);
  dependency.observers.add(observer);
  if (!watched) {
    mount(dependency);
  }
}

function detach<T>(dependency: AnyNode, observer: Node<T>): void {
  dependency.observers.delete(observer);
  if (!isWatched(dependency)) {
    unmount(dependency);
  }
}

/** Read a cell, recording it as a dependency of whatever is evaluating. */
export function readNode<T>(node: Cell<T>): T {
  // Recorded before the pull, so a dependency that fails is still a
  // dependency: the derive that read it must run again when it recovers.
  track(node);
  pull(node);
  if (node.thrown !== NOTHING) {
    throw node.thrown;
  }
  return node.value;
}

/** Read a cell without recording it, and without subscribing to it. */
export function peekNode<T>(node: Cell<T>): T {
  return untracked(() => readNode(node));
}

/** What a cell holds, and where that value belongs. */
export function snapshotNode<T>(node: Cell<T>): CellSnapshot<T> {
  return { scope: node.scope, value: readNode(node) };
}

/** Where a resource is in its load; `"success"` for every other cell. */
export function statusOf<T>(node: Cell<T>): ResourceStatus {
  return node.status;
}

/**
 * What the node holds right now, without pulling it.
 *
 * For an evaluation that needs its own previous value: a node is mid-run at
 * that point, and pulling it would report the self-reference this is not.
 */
export function currentValue<T>(node: Cell<T>): T {
  return node.value;
}

/**
 * The count of supersessions. A settlement carrying an older one is stale.
 *
 * Bumped by every evaluation and every write, because both replace whatever
 * the node was going to become.
 */
export function generationOf<T>(node: Cell<T>): number {
  return node.generation;
}

/**
 * Write a value into a cell that holds one.
 *
 * Derived cells refuse: their value is a function of their dependencies, and a
 * write that stood would be silently undone by the next recompute.
 */
export function writeNode<T>(node: Cell<T>, value: T): void {
  if (node.kind === "derived") {
    throw Error(`@uniflowed/cell ${node.scope} cells are read-only`);
  }
  node.generation += 1;
  node.status = "success";
  commit(node, value, NOTHING);
  settleQueue();
}

/**
 * Record a value that arrived asynchronously.
 *
 * Separate from [`writeNode`] because the caller has already decided the
 * settlement is not stale, and because a failed load is committed as a thrown
 * value rather than as a value — see [`readNode`].
 */
export function settleNode<T>(node: Cell<T>, value: T, status: ResourceStatus, error: mixed): void {
  node.status = status;
  commit(node, value, status === "failure" ? error : NOTHING);
  settleQueue();
}

/** Move a resource's status without committing a new value. */
export function setStatus<T>(node: Cell<T>, status: ResourceStatus): void {
  node.status = status;
}

/**
 * Mark a node as definitely out of date, and wake anything watching it.
 *
 * What `refresh` is built from: a resource's inputs have not changed, so
 * nothing would mark it, but the caller knows the answer may have.
 */
export function invalidateNode<T>(node: Cell<T>): void {
  markStale(node, DIRTY);
  settleQueue();
}

/**
 * Subscribe to a cell, mounting it if this is the first watcher.
 *
 * `notifiedVersion` is only reset for the *first* listener. Resetting it for
 * every subscriber would let a component mounting between a write and the
 * flush swallow the notification the earlier subscribers were owed.
 */
export function subscribeNode<T>(node: Cell<T>, listener: Listener): Unsubscribe {
  const first = !isWatched(node);
  const firstListener = node.listeners.size === 0;
  node.listeners.add(listener);
  hold();
  try {
    if (first) {
      mount(node);
    } else {
      pull(node);
    }
    if (firstListener) {
      node.notifiedVersion = node.version;
    }
  } finally {
    // Mounting can evaluate, and evaluating can commit a first value; a
    // subscriber that has just arrived is not owed a notification for the
    // value it is about to read. Releasing after `notifiedVersion` is set is
    // what makes subscribing quiet.
    release();
  }

  return () => {
    if (!node.listeners.delete(listener)) {
      return;
    }
    if (isWatched(node)) {
      return;
    }
    hold();
    try {
      unmount(node);
    } finally {
      release();
    }
  };
}
