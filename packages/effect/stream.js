// @flow
//
// `@uniflowed/effect/stream`.
//
// Many values, pulled one batch at a time.
//
// An `Effect<A, E, R>` produces one `A`. Everything this package had for
// producing several — `all`, `forEach` — collects them into an array first, so
// a program that reads a large file, walks a paginated API or consumes a
// server-sent-event connection either buys the whole thing into memory or
// leaves the package and writes an async iterator by hand, at which point it
// has left the error channel, the interruption protocol and `Scope` behind
// too.
//
// # Why this is a separate module, and why that was not obvious
//
// `index.js` argues that the runtime cannot be split: `Effect`, `Fiber`, `Tag`
// and `Layer` are opaque types over carriers only that file may construct, so
// moving a combinator out means handing `makeEffect` and `readKernel` to a
// sibling. `schedule.js` escapes that because a policy is arithmetic that
// never touches a carrier.
//
// A stream is not arithmetic — it produces effects and runs them — so the
// first guess was that it had to live in `index.js`, or that the runtime
// needed an internal seam for it. Both are wrong, and finding out why is the
// design of this module: **a `Stream` needs the `Effect` *interface*, not the
// `Effect` *carrier*.* Every line below is written with the public exports —
// `effect`, `map`, `flatMap`, `forEach`, `ensuring`, `suspend` — and there is
// no `makeEffect` here and no seam in `index.js` for one. The opacity is
// intact in both directions: this module cannot see inside an `Effect`, and
// nothing outside can see inside a `Stream`.
//
// That is worth stating rather than assuming, because the opposite belief is
// what usually turns a two-file package into a five-file one with an
// `internal/` directory: "this needs the runtime" is nearly always "this needs
// the runtime's public API", and the two have very different costs.
//
// # What a stream is here
//
// A pull: `open()` hands back a step that produces the next batch, or `null`
// when there is no next batch, together with the way to close whatever the
// traversal opened.
//
// Batches rather than elements, because per-element effects are the whole cost
// of a stream — one `Effect` allocated, run and settled per row is what makes
// the naive version unusable on a large file. Effect calls the batch a `Chunk`
// and gives it a module of its own; here it is a `$ReadOnlyArray<A>`, which is
// the honest equivalent in a language with no `Chunk` and no reason to grow a
// lookalike with a fraction of the behaviour behind it.
//
// An empty batch is not the end. A `streamFilter` that rejects everything in
// one batch returns `[]` and the traversal continues; only `null` ends it.
// That is what keeps a filter from having to pull until it finds something,
// which is how a filter over an infinite source stops being interruptible.
//
// # What is deliberately absent
//
// `Channel`, `Sink` and `GroupBy`. Effect defines `Stream` and `Sink` in terms
// of `Channel`, a bidirectional primitive, and a `Stream` defined directly —
// as this one is — cannot express a `Sink` as the dual of a source. That is a
// real cost and the right one to pay first: the alternative is porting the
// primitive before anything needs it. The runners below (`streamRunCollect`,
// `streamRunFold`, …) are the nearest honest shape — a consumer that produces
// an `Effect` and cannot itself be composed with another consumer.
//
// `Chunk` as a type, for the reason above.
//
// A list of transforms, which is a different kind of absence from the three
// above: `streamFlatMap`, `streamScan`, `streamMapAccum`, `streamTakeWhile`,
// `streamDrop`, `streamGrouped`, `streamAcquireRelease` (as opposed to today's
// `streamEnsuring`), `streamRetry` over a `Schedule`, `streamTimeout`,
// `streamInterruptWhen`, `streamThrottle`, `streamDebounce` and
// `streamRechunk`. None of them changes the type or needs a decision, which is
// exactly why they can wait: adding them later costs nothing that adding them
// now would save. #329 is the list.
//
// # The two combinators that own a fiber
//
// Everything here was a pull until `streamBuffer` and `streamMerge`: a
// traversal runs on whatever fiber asked it for the next batch, and there is no
// concurrency to reason about. Those two cannot be that, because letting a
// source run ahead of its consumer, and running two sources at once, is what
// they are for.
//
// Both do it the same way, and the way is worth stating once. A fiber per
// source, forked with `fork` so the fiber running the traversal owns it and a
// traversal whose fiber ends takes its pumps with it. A `Queue` whose bound is
// the back pressure, so the faster side stops rather than growing an array. The
// end of a source is a value in the queue rather than a shutdown, because a
// shutdown discards what is still buffered and an end must not; anything else —
// a failure, a defect, an interruption — does shut the queue down, which is what
// wakes a consumer that would otherwise wait for a batch nobody is going to
// produce.
//
// What went wrong then lives on the fiber rather than in the queue, and `join`
// is what puts it back on the stream's error channel with its cause intact.
// That is the reason nothing in this module takes a `Cause` apart to move a
// failure across a queue: a fiber already carries one faithfully, and the one
// place that does read a `Cause` is `streamToReadableStream`, where the channel
// on the other side genuinely has only one.

import {
  andThen,
  as,
  effect,
  ensuring,
  exit,
  flatMap,
  forEach,
  fork,
  interrupt,
  join,
  map,
  promise,
  queue,
  queueOffer,
  queueShutdown,
  queueTake,
  queueTakeUpTo,
  runFork,
  runPromiseExit,
  succeed,
  suspend,
} from "./index.js";
import type { Cause, Effect, EffectGenerator, Fiber, Queue } from "./index.js";

/**
 * One traversal of a stream.
 *
 * `pull` is run repeatedly and is stateful — it closes over whatever cursor
 * the source keeps — so a stream is described once and traversed many times,
 * each traversal getting its own `open()`.
 *
 * `close` takes no requirements and its failure is nobody's typed error, which
 * is the same shape `acquireRelease`'s release has and for the same reason: a
 * cleanup that could fail in a way the caller was supposed to handle is not a
 * cleanup.
 */
type StreamStep<out A, out E, out R> = {
  readonly pull: Effect<?$ReadOnlyArray<A>, E, R>,
  readonly close: Effect<mixed, mixed, empty>,
};

type StreamCarrier<out A, out E, out R> = {
  readonly __kind: "Stream",
  readonly open: () => StreamStep<A, E, R>,
};

/**
 * Many `A`s, which may fail with an `E`, and need an `R`.
 *
 * The same three channels an `Effect` has, and they compose the same way: a
 * `streamMapEffect` over a step that can fail widens `E` to the union, and a
 * traversal ends where the rest of the package begins — every runner produces
 * an ordinary `Effect`.
 */
export opaque type Stream<out A, out E = empty, out R = empty> = StreamCarrier<A, E, R>;

function makeStream<A, E, R>(open: () => StreamStep<A, E, R>): Stream<A, E, R> {
  return { __kind: "Stream", open };
}

function openStream<A, E, R>(self: Stream<A, E, R>): StreamStep<A, E, R> {
  return self.open();
}

/** Nothing to close. Shared, because most sources have nothing to close. */
const NOTHING_TO_CLOSE: Effect<mixed, mixed, empty> = succeed(undefined);

/** The default batch size for a source that has no opinion of its own. */
const DEFAULT_CHUNK_SIZE = 64;

function chunkSizeOf(options: ?{ readonly chunkSize?: number }): number {
  const requested = options == null ? null : options.chunkSize;
  return requested == null ? DEFAULT_CHUNK_SIZE : Math.max(1, Math.floor(requested));
}

/**
 * Every element of an array, in batches.
 *
 * The batch size is the throughput knob and nothing else depends on it: a
 * `streamTake(3)` takes three elements whatever the batches were, because the
 * transforms below truncate batches rather than assuming them.
 */
export function streamFromArray<A>(
  items: $ReadOnlyArray<A>,
  options?: { readonly chunkSize?: number },
): Stream<A> {
  const size = chunkSizeOf(options);
  return makeStream(() => {
    let cursor = 0;
    const pull: Effect<?$ReadOnlyArray<A>, empty, empty> = suspend(() => {
      if (cursor >= items.length) {
        return succeed(null);
      }
      const batch = items.slice(cursor, cursor + size);
      cursor += batch.length;
      return succeed(batch);
    });
    return { pull, close: NOTHING_TO_CLOSE };
  });
}

/**
 * Every element an iterator produces, in batches.
 *
 * `open` rather than an `Iterator` value, because a stream is traversable more
 * than once and an iterator is not: handing the same exhausted iterator to a
 * second traversal would make the second one empty, which is the kind of bug
 * that shows up a long way from here.
 *
 * The source may be infinite. `streamTake` and a failing step both end a
 * traversal without draining it, and the batch size bounds how far past the
 * last wanted element the source is asked to go.
 */
export function streamFromIterator<A>(
  open: () => Iterator<A>,
  options?: { readonly chunkSize?: number },
): Stream<A> {
  const size = chunkSizeOf(options);
  return makeStream(() => {
    const iterator = open();
    let drained = false;
    const pull: Effect<?$ReadOnlyArray<A>, empty, empty> = suspend(() => {
      if (drained) {
        return succeed(null);
      }
      const batch: Array<A> = [];
      while (batch.length < size) {
        const step = iterator.next();
        if (step.done === true) {
          drained = true;
          break;
        }
        batch.push(step.value);
      }
      return succeed(batch.length === 0 ? null : batch);
    });
    return { pull, close: NOTHING_TO_CLOSE };
  });
}

/** One element: the value the effect produces, or its failure. */
export function streamFromEffect<A, E, R>(self: Effect<A, E, R>): Stream<A, E, R> {
  return makeStream(() => {
    let taken = false;
    const pull: Effect<?$ReadOnlyArray<A>, E, R> = suspend(() => {
      if (taken) {
        return succeed(null);
      }
      taken = true;
      return map(self, (value) => [value]);
    });
    return { pull, close: NOTHING_TO_CLOSE };
  });
}

/**
 * A page at a time, until a page says there is no next one.
 *
 * The shape a paginated API actually has, and the reason a stream earns its
 * place here rather than a `forEach` over a list of page numbers: the number
 * of pages is not known until the last one says so.
 */
export function streamPaginate<A, S, E, R>(
  initial: S,
  page: (cursor: S) => Effect<{ readonly items: $ReadOnlyArray<A>, readonly next: ?S }, E, R>,
): Stream<A, E, R> {
  return makeStream(() => {
    let cursor = initial;
    let drained = false;
    const pull: Effect<?$ReadOnlyArray<A>, E, R> = suspend(() => {
      if (drained) {
        return succeed(null);
      }
      return map(page(cursor), (answer) => {
        const next = answer.next;
        if (next == null) {
          drained = true;
        } else {
          cursor = next;
        }
        return answer.items;
      });
    });
    return { pull, close: NOTHING_TO_CLOSE };
  });
}

/**
 * The reader half of a web `ReadableStream`, described structurally.
 *
 * Structural rather than the DOM `ReadableStream` type, because this package
 * has to work on Node, Deno, Bun and an edge runtime, and the four of them do
 * not agree on which library file that name lives in. What they do agree on is
 * the shape below, which is all a source needs — and taking it structurally
 * also accepts Node's `stream/web` reader and a test double.
 */
type ChunkReader<A> = {
  readonly read: () => Promise<{ readonly done?: boolean, readonly value?: A, ... }>,
  readonly cancel: () => mixed,
  ...
};

type ReadableChunks<A> = {
  readonly getReader: () => ChunkReader<A>,
  ...
};

/**
 * A web `ReadableStream`, as an effect stream.
 *
 * `Response.body` on every host uf targets, which is the concrete reason this
 * module exists rather than the checklist one. A batch here is one chunk from
 * the reader, because that is the batching the host already chose.
 *
 * Cancelling the traversal cancels the reader, so a `streamTake` over a
 * response body closes the connection rather than reading it to the end.
 */
export function streamFromReadableStream<A>(open: () => ReadableChunks<A>): Stream<A> {
  return makeStream(() => {
    let reader: ?ChunkReader<A> = null;
    const readerFor = (): ChunkReader<A> => {
      const already = reader;
      if (already != null) {
        return already;
      }
      const started = open().getReader();
      reader = started;
      return started;
    };
    const pull: Effect<?$ReadOnlyArray<A>, empty, empty> = promise(async () => {
      const step = await readerFor().read();
      return step.done === true ? null : [readValue(step)];
    });
    // A reader that has already been cancelled rejects on `cancel`, and a
    // traversal that ended because its fiber was interrupted is exactly when
    // that happens. Nothing useful can be done with it, and turning it into a
    // defect would replace a real outcome with a cleanup detail.
    const close: Effect<mixed, mixed, empty> = promise(async () => {
      const started = reader;
      if (started != null) {
        try {
          await started.cancel();
        } catch (error) {
          return null;
        }
      }
      return null;
    });
    return { pull, close };
  });
}

/**
 * The chunk a reader step carries, once `done` has said there is one.
 *
 * The web streams contract types `value` as optional because a `done` step
 * carries none, and nothing narrows an optional property by a check on a
 * sibling. This is the same shape of narrow, local claim as the four in
 * `index.js`: provable from the line above the call rather than from a belief
 * about the world.
 */
function readValue<A>(step: { readonly value?: A, ... }): A {
  // $FlowFixMe[incompatible-type] the `done` check above excluded the other arm
  return step.value;
}

/**
 * Change every element.
 *
 * A transform that throws is a defect, because `map` over an `Effect` already
 * says so and a stream should not quietly disagree with the package it is
 * built on.
 */
export function streamMap<A, B, E, R>(
  self: Stream<A, E, R>,
  transform: (value: A) => B,
): Stream<B, E, R> {
  return makeStream(() => {
    const source = openStream(self);
    return {
      pull: map(source.pull, (batch) => (batch == null ? null : batch.map(transform))),
      close: source.close,
    };
  });
}

/** Keep the elements that pass. An emptied batch is not the end. */
export function streamFilter<A, E, R>(
  self: Stream<A, E, R>,
  keep: (value: A) => boolean,
): Stream<A, E, R> {
  return makeStream(() => {
    const source = openStream(self);
    return {
      pull: map(source.pull, (batch) => (batch == null ? null : batch.filter(keep))),
      close: source.close,
    };
  });
}

/**
 * The first `count` elements, and then stop pulling.
 *
 * The traversal ends without draining the source, and the runner still closes
 * it — which is what makes `streamTake(3)` of an infinite source release what
 * the source opened rather than leaking it.
 */
export function streamTake<A, E, R>(self: Stream<A, E, R>, count: number): Stream<A, E, R> {
  const wanted = Math.max(0, Math.floor(count));
  return makeStream(() => {
    const source = openStream(self);
    let taken = 0;
    const pull: Effect<?$ReadOnlyArray<A>, E, R> = suspend(() => {
      if (taken >= wanted) {
        return succeed(null);
      }
      return map(source.pull, (batch) => {
        if (batch == null) {
          return null;
        }
        const room = wanted - taken;
        const kept = batch.length <= room ? batch : batch.slice(0, room);
        taken += kept.length;
        return kept;
      });
    });
    return { pull, close: source.close };
  });
}

/** Look at every element without changing it. */
export function streamTap<A, E1, E2, R1, R2>(
  self: Stream<A, E1, R1>,
  body: (value: A) => Effect<mixed, E2, R2>,
): Stream<A, E1 | E2, R1 | R2> {
  return makeStream(() => {
    const source = openStream(self);
    const pull: Effect<?$ReadOnlyArray<A>, E1 | E2, R1 | R2> = flatMap(source.pull, (batch) =>
      batch == null
        ? succeed(null)
        : as(
            forEach(batch, (item) => body(item)),
            batch,
          ),
    );
    return { pull, close: source.close };
  });
}

/**
 * Change every element with an effect, up to `concurrency` at a time.
 *
 * Order is preserved: the results come back in the order the elements arrived,
 * whatever order the effects finished in, because `forEach` already promises
 * that and this is a `forEach` over a window.
 *
 * The window is filled across batch boundaries, so the concurrency a caller
 * asks for is the concurrency they get whatever the source's batching was.
 * Without that, a source handing out one element at a time would silently make
 * every concurrency limit mean one.
 *
 * There is no `"unbounded"`, unlike `all` and `forEach`. A stream has no
 * length, so unbounded here would mean buffering the whole of it — which is
 * the thing a stream exists to avoid, and an option that silently means
 * something else is worse than one that is not offered.
 */
export function streamMapEffect<A, B, E1, E2, R1, R2>(
  self: Stream<A, E1, R1>,
  body: (value: A) => Effect<B, E2, R2>,
  options?: { readonly concurrency?: number },
): Stream<B, E1 | E2, R1 | R2> {
  const requested = options == null ? null : options.concurrency;
  const width = requested == null ? 1 : Math.max(1, Math.floor(requested));
  return makeStream(() => {
    const source = openStream(self);
    const window: Array<A> = [];
    let drained = false;
    const pull = effect(function* (): EffectGenerator<?$ReadOnlyArray<B>, E1 | E2, R1 | R2> {
      while (!drained && window.length < width) {
        const batch = yield* source.pull;
        if (batch == null) {
          drained = true;
          break;
        }
        for (const item of batch) {
          window.push(item);
        }
      }
      if (window.length === 0) {
        return null;
      }
      return yield* forEach(window.splice(0, width), (item) => body(item), {
        concurrency: width,
      });
    });
    return { pull, close: source.close };
  });
}

/**
 * Run `finalizer` when a traversal ends, however it ends.
 *
 * After the stream's own close, so finalizers run outermost last — the order
 * `ensuring` gives an effect, said again for a traversal.
 */
export function streamEnsuring<A, E, R>(
  self: Stream<A, E, R>,
  finalizer: () => Effect<mixed, mixed, empty>,
): Stream<A, E, R> {
  return makeStream(() => {
    const source = openStream(self);
    return { pull: source.pull, close: andThen(source.close, suspend(finalizer)) };
  });
}

/**
 * Consume the whole stream into one value.
 *
 * The runner every other runner is written in terms of, and the one place a
 * traversal is opened and closed. `ensuring` is what closes it: the traversal
 * ends on success, on a failure from a pull, on a defect from `step`, and on
 * the fiber being interrupted, and the source is closed in all four — the same
 * guarantee `acquireRelease` gives, reached the same way.
 *
 * The interruption checkpoint is `effect`'s own, between steps, so a fiber
 * draining a stream stops between pulls rather than in the middle of one.
 */
export function streamRunFold<A, B, E, R>(
  self: Stream<A, E, R>,
  initial: B,
  step: (state: B, value: A) => B,
): Effect<B, E, R> {
  return suspend(() => {
    const source = openStream(self);
    return ensuring(
      effect(function* (): EffectGenerator<B, E, R> {
        let state = initial;
        let batch = yield* source.pull;
        while (batch != null) {
          for (const item of batch) {
            state = step(state, item);
          }
          batch = yield* source.pull;
        }
        return state;
      }),
      () => source.close,
    );
  });
}

/** Every element, in order, as an array. */
export function streamRunCollect<A, E, R>(self: Stream<A, E, R>): Effect<$ReadOnlyArray<A>, E, R> {
  // `suspend`, so two runs of the same effect do not share one array.
  return suspend(() => {
    const collected: Array<A> = [];
    return streamRunFold(self, collected, (into, item) => {
      into.push(item);
      return into;
    });
  });
}

/** Run the stream for its effects and discard its elements. */
export function streamRunDrain<A, E, R>(self: Stream<A, E, R>): Effect<void, E, R> {
  return as(
    streamRunFold(self, 0, (count) => count + 1),
    undefined,
  );
}

/** Run `body` for every element, in order. */
export function streamRunForEach<A, E1, E2, R1, R2>(
  self: Stream<A, E1, R1>,
  body: (value: A) => Effect<mixed, E2, R2>,
): Effect<void, E1 | E2, R1 | R2> {
  return streamRunDrain(streamTap(self, body));
}

/**
 * The first element, or `null` for a stream that had none.
 *
 * Stops after one: the source is closed without being drained, which is the
 * whole difference between this and `streamRunCollect(...)[0]`.
 */
export function streamRunHead<A, E, R>(self: Stream<A, E, R>): Effect<?A, E, R> {
  return map(streamRunCollect(streamTake(self, 1)), (collected) =>
    collected.length === 0 ? null : collected[0],
  );
}

/**
 * Everything a queue is handed, until it is shut down.
 *
 * A queue has no end of its own, so one has to be agreed: `queueShutdown` is
 * it, and a traversal that finds the queue shut down ends rather than failing.
 * That is the same decision `Queue` already took for a shutdown — it is an
 * interruption and not a typed failure — read from the consumer's side.
 *
 * A batch is up to `chunkSize` of whatever is waiting, so a fast producer is
 * consumed in batches rather than one value at a time, and a slow one does not
 * make the traversal wait for a batch to fill.
 */
export function streamFromQueue<A>(
  source: Queue<A>,
  options?: { readonly chunkSize?: number },
): Stream<A> {
  const size = chunkSizeOf(options);
  return makeStream(() => {
    let drained = false;
    const pull = effect(function* (): EffectGenerator<?$ReadOnlyArray<A>, empty, empty> {
      if (drained) {
        return null;
      }
      const first = yield* exit(queueTake(source));
      if (first.kind === "failure") {
        // A take stops for exactly two reasons, and only one of them is this
        // stream's news. The queue having been shut down is the end of it. This
        // fiber having been interrupted is not — and needs no saying here,
        // because `effect` checks interruption between steps, so the traversal's
        // own next step reports it before anything else can.
        drained = true;
        return null;
      }
      // The second take is asked the same way, because a queue shut down
      // between the two is a stream that ends here rather than one that reports
      // an interruption for the batch it had already taken a value for.
      const rest = yield* exit(queueTakeUpTo(source, size - 1));
      return rest.kind === "success" ? [first.value, ...rest.value] : [first.value];
    });
    return { pull, close: NOTHING_TO_CLOSE };
  });
}

/**
 * Let the source run ahead of the consumer, up to `capacity` batches.
 *
 * The first combinator here that needs a fiber of its own: the source is
 * drained into a bounded queue by a fiber the traversal owns, and the pull takes
 * from the queue. A consumer that is slower than the source stops the source at
 * the queue's bound rather than at its own speed, which is the difference
 * between a pipeline that overlaps and one that alternates.
 *
 * `bounded` and not `dropping`: a buffer that silently lost elements would make
 * `streamBuffer` change what a stream contains rather than when it arrives.
 */
export function streamBuffer<A, E, R>(self: Stream<A, E, R>, capacity: number): Stream<A, E, R> {
  const size = Math.max(1, Math.floor(capacity));
  return makeStream(() => {
    const source = openStream(self);
    let running: ?PumpedInto<A, E> = null;
    let drained = false;
    const pull = effect(function* (): EffectGenerator<?$ReadOnlyArray<A>, E, R> {
      if (drained) {
        return null;
      }
      const started = running == null ? yield* pumping(source, size) : running;
      running = started;
      const taken = yield* exit(queueTake(started.buffer));
      if (taken.kind === "failure") {
        // The queue was shut down, which the pump does when it stopped without
        // reaching the end of the source.
        yield* whyPumpStopped(started.fiber);
        drained = true;
        return null;
      }
      const batch = taken.value;
      if (batch == null) {
        drained = true;
        return null;
      }
      return batch;
    });
    const close = effect(function* (): EffectGenerator<mixed, mixed, empty> {
      const started = running;
      if (started != null) {
        yield* interrupt(started.fiber);
        yield* queueShutdown(started.buffer);
      }
      return yield* source.close;
    });
    return { pull, close };
  });
}

/**
 * Both streams' elements, in whatever order they arrive.
 *
 * Two fibers filling one bounded queue, which is what makes this a merge rather
 * than a concatenation: neither side waits for the other, and the queue's bound
 * is what stops the faster one from running away. The traversal ends when both
 * sides have, and a failure on either side ends it with that failure.
 *
 * There is no ordering promise between the sides, which is what "merge" means.
 * `streamZip` is the combinator with one.
 */
export function streamMerge<A, E1, E2, R1, R2>(
  left: Stream<A, E1, R1>,
  right: Stream<A, E2, R2>,
  options?: { readonly capacity?: number },
): Stream<A, E1 | E2, R1 | R2> {
  const requested = options == null ? null : options.capacity;
  const size = requested == null ? DEFAULT_MERGE_CAPACITY : Math.max(1, Math.floor(requested));
  return makeStream(() => {
    const leftSource = openStream(left);
    const rightSource = openStream(right);
    let shared: ?Queue<?$ReadOnlyArray<A>> = null;
    let pumps: ?{ readonly left: Fiber<void, E1>, readonly right: Fiber<void, E2> } = null;
    let ended = 0;
    let drained = false;
    const pull = effect(function* (): EffectGenerator<?$ReadOnlyArray<A>, E1 | E2, R1 | R2> {
      if (drained) {
        return null;
      }
      let buffer = shared;
      let started = pumps;
      if (buffer == null || started == null) {
        buffer = yield* queue(size);
        started = {
          left: yield* pumpingInto(leftSource, buffer),
          right: yield* pumpingInto(rightSource, buffer),
        };
        shared = buffer;
        pumps = started;
      }
      for (;;) {
        const taken = yield* exit(queueTake(buffer));
        if (taken.kind === "failure") {
          // Whichever side stopped without reaching its end shut the queue
          // down. Both are asked, and the one that failed is the one that says
          // so — the other has been interrupted by the shutdown and has nothing
          // to report.
          yield* whyPumpStopped(started.left);
          yield* whyPumpStopped(started.right);
          drained = true;
          return null;
        }
        const batch = taken.value;
        if (batch != null) {
          return batch;
        }
        // One side reached its end. The stream does not, until both have.
        ended += 1;
        if (ended >= 2) {
          drained = true;
          return null;
        }
      }
    });
    const close = effect(function* (): EffectGenerator<mixed, mixed, empty> {
      const started = pumps;
      if (started != null) {
        yield* interrupt(started.left);
        yield* interrupt(started.right);
      }
      const buffer = shared;
      if (buffer != null) {
        yield* queueShutdown(buffer);
      }
      yield* leftSource.close;
      return yield* rightSource.close;
    });
    return { pull, close };
  });
}

/**
 * Pairs, until either side runs out.
 *
 * The one combinator here that needs no queue and no fiber: both sides are
 * pulled in step and the leftovers of the longer batch are kept until the
 * shorter one catches up. A queue would buy nothing, because a zip cannot get
 * ahead of its slower side by definition.
 *
 * The traversal ends with the shorter stream, and the longer one is closed
 * without being drained — which is what makes zipping an infinite source with a
 * finite one terminate.
 */
export function streamZip<A, B, E1, E2, R1, R2>(
  left: Stream<A, E1, R1>,
  right: Stream<B, E2, R2>,
): Stream<[A, B], E1 | E2, R1 | R2> {
  return makeStream(() => {
    const leftSource = openStream(left);
    const rightSource = openStream(right);
    const leftOver: Array<A> = [];
    const rightOver: Array<B> = [];
    let drained = false;
    const pull = effect(function* (): EffectGenerator<?$ReadOnlyArray<[A, B]>, E1 | E2, R1 | R2> {
      while (!drained && (leftOver.length === 0 || rightOver.length === 0)) {
        if (leftOver.length === 0) {
          const batch = yield* leftSource.pull;
          if (batch == null) {
            drained = true;
            break;
          }
          for (const item of batch) {
            leftOver.push(item);
          }
        }
        if (rightOver.length === 0) {
          const batch = yield* rightSource.pull;
          if (batch == null) {
            drained = true;
            break;
          }
          for (const item of batch) {
            rightOver.push(item);
          }
        }
      }
      const pairs = Math.min(leftOver.length, rightOver.length);
      if (pairs === 0) {
        return null;
      }
      const zipped: Array<[A, B]> = [];
      for (let index = 0; index < pairs; index += 1) {
        zipped.push([leftOver[index], rightOver[index]]);
      }
      leftOver.splice(0, pairs);
      rightOver.splice(0, pairs);
      return zipped;
    });
    // `ensuring` and not `andThen`, so a left-hand close that fails still leaves
    // the right-hand one run: two sources are two things to give back.
    const close = ensuring(leftSource.close, () => rightSource.close);
    return { pull, close };
  });
}

/** The default bound on how far `streamMerge` lets a side run ahead. */
const DEFAULT_MERGE_CAPACITY = 16;

/** A traversal being drained into a queue by a fiber, and the queue. */
type PumpedInto<A, E> = {
  readonly buffer: Queue<?$ReadOnlyArray<A>>,
  readonly fiber: Fiber<void, E>,
};

/** A queue of the right shape, and a fiber filling it from `source`. */
function pumping<A, E, R>(
  source: StreamStep<A, E, R>,
  capacity: number,
): Effect<PumpedInto<A, E>, empty, R> {
  return effect(function* (): EffectGenerator<PumpedInto<A, E>, empty, R> {
    const buffer: Queue<?$ReadOnlyArray<A>> = yield* queue(capacity);
    return { buffer, fiber: yield* pumpingInto(source, buffer) };
  });
}

/**
 * Drain a traversal into a queue, in a fiber of its own.
 *
 * The end of the source is a `null` in the queue rather than a shutdown,
 * because a shutdown discards what is still buffered and an end must not: the
 * consumer takes the batches in order and finds the `null` behind them.
 *
 * Anything else — a failure, a defect, this fiber being interrupted — *does*
 * shut the queue down, which is what wakes a consumer that would otherwise wait
 * for a batch nobody is going to produce. What went wrong is then on the fiber,
 * and `whyPumpStopped` is what puts it back on the stream's error channel.
 *
 * `fork` and not `forkDaemon`: the pump belongs to the fiber running the
 * traversal, so a traversal whose fiber ends takes its pumps with it even if
 * nothing closed the stream.
 */
function pumpingInto<A, E, R>(
  source: StreamStep<A, E, R>,
  into: Queue<?$ReadOnlyArray<A>>,
): Effect<Fiber<void, E>, empty, R> {
  return suspend(() => {
    let reachedTheEnd = false;
    const drain = effect(function* (): EffectGenerator<void, E, R> {
      for (;;) {
        const batch = yield* source.pull;
        yield* queueOffer(into, batch);
        if (batch == null) {
          reachedTheEnd = true;
          return;
        }
      }
    });
    return fork(
      ensuring(drain, () =>
        suspend(() => (reachedTheEnd ? succeed(undefined) : queueShutdown(into))),
      ),
    );
  });
}

/**
 * Stop a pump, and put back whatever it failed with.
 *
 * `join` on a settled fiber re-raises that fiber's own outcome, cause and all,
 * which is the one thing that carries a defect or a composite failure across
 * without this module taking a `Cause` apart. The `interrupt` first is for the
 * *other* pump in a merge, which is still running and has nothing to say: a
 * fiber that was interrupted is not the reason the traversal ended.
 */
function whyPumpStopped<E>(fiber: Fiber<void, E>): Effect<void, E, empty> {
  return effect(function* (): EffectGenerator<void, E, empty> {
    const stopped = yield* interrupt(fiber);
    if (stopped.kind === "failure" && stopped.cause.kind !== "interrupt") {
      yield* join(fiber);
    }
  });
}

/**
 * What a host's `ReadableStream` hands its source, structurally.
 *
 * Structural for the same reason `ChunkReader` is: the four hosts uf targets do
 * not agree on which library file declares `ReadableStreamController`, and they
 * do agree on this shape.
 */
type ChunkSink<A> = {
  readonly enqueue: (chunk: A) => mixed,
  readonly close: () => mixed,
  readonly error: (reason: mixed) => mixed,
  ...
};

/** What a host's `ReadableStream` constructor takes, structurally. */
type ChunkSource<A> = {
  readonly pull: (controller: ChunkSink<A>) => Promise<mixed>,
  readonly cancel: (reason: mixed) => Promise<mixed>,
};

/**
 * A stream, as something a web consumer can read.
 *
 * The other end of `streamFromReadableStream`, and the one `@uniflowed/server`
 * needs: streaming SSR and RSC produce a `ReadableStream`, and until this
 * existed an effect program had no way to be on that end of one. Three
 * decisions, none of which is about the function's body:
 *
 * **Which fiber the pull runs on.** One per host pull, started with `runFork`
 * and kept in a closure that `cancel` can reach. A `ReadableStream`'s `pull` is
 * a callback the *host* calls, so there is no fiber to inherit and no
 * interruption to propagate — the handle is the only thing that can stop the
 * work, which is exactly the situation `runFork` exists for. The traversal
 * itself spans many pulls and is opened on the first of them, so a
 * `ReadableStream` nobody reads never opens what the source would have.
 *
 * **What a typed failure becomes.** A `ReadableStream` has one `error(reason)`
 * and no channels, so the three ways an effect can fail cannot stay three. A
 * typed failure crosses as its own value, because `E` is the type the consumer
 * named and an `Error` wrapped round it would lose it. A defect and an
 * interruption cross as an `Error`, because neither has a value anybody named —
 * a defect has a message and an interruption has only the fact.
 *
 * **How the host's constructor is reached.** By being handed it:
 * `streamToReadableStream(stream, (source) => new ReadableStream(source))`. That
 * is the same avoidance `streamFromReadableStream` makes by taking `open`, for
 * the same reason — this module cannot name a global that four hosts declare in
 * four places — and it is why the return type is whatever the caller's
 * constructor produced rather than a type this module invented.
 *
 * The guarantees are the runners': a consumer that cancels stops the pull it
 * interrupted and closes the traversal, so a `ReadableStream` abandoned halfway
 * releases what its source opened, exactly as `streamTake(3)` of an infinite
 * source does. Nothing is enqueued after a cancel, because the controller is
 * closed by then and touching it would raise inside the host.
 *
 * `R` is `empty`, as it is for every runner in this package: an effect that
 * still needs a service has nowhere to get one from outside the runtime.
 */
export function streamToReadableStream<A, E, Made>(
  self: Stream<A, E>,
  make: (source: ChunkSource<A>) => Made,
): Made {
  let traversal: ?StreamStep<A, E, empty> = null;
  let pulling: ?Fiber<?$ReadOnlyArray<A>, E> = null;
  let closed = false;
  let cancelled = false;
  const opened = (): StreamStep<A, E, empty> => {
    const already = traversal;
    if (already != null) {
      return already;
    }
    const started = openStream(self);
    traversal = started;
    return started;
  };
  const closeOnce = async (): Promise<void> => {
    const started = traversal;
    if (closed || started == null) {
      closed = true;
      return;
    }
    closed = true;
    await runPromiseExit(started.close);
  };
  return make({
    pull: async (controller: ChunkSink<A>) => {
      if (cancelled) {
        return null;
      }
      const fiber = runFork(opened().pull);
      pulling = fiber;
      const settled = await runPromiseExit(join(fiber));
      pulling = null;
      if (cancelled) {
        return null;
      }
      if (settled.kind === "failure") {
        await closeOnce();
        controller.error(reasonFor(settled.cause));
        return null;
      }
      const batch = settled.value;
      if (batch == null) {
        await closeOnce();
        controller.close();
        return null;
      }
      for (const item of batch) {
        controller.enqueue(item);
      }
      return null;
    },
    cancel: async () => {
      cancelled = true;
      const running = pulling;
      if (running != null) {
        await runPromiseExit(interrupt(running));
      }
      await closeOnce();
      return null;
    },
  });
}

/**
 * The one reason a `ReadableStream` can be given, out of the three an effect
 * can end with.
 *
 * A typed failure is its own value; everything else is an `Error`, because a
 * `reason` a consumer cannot name is one it can only print.
 */
function reasonFor<E>(cause: Cause<E>): mixed {
  switch (cause.kind) {
    case "fail":
      return cause.error;
    case "die":
      return new Error(cause.defect);
    case "interrupt":
      return new Error("the fiber producing this stream was interrupted");
    case "sequential":
    case "parallel": {
      for (const inner of cause.causes) {
        if (inner.kind !== "empty") {
          return reasonFor(inner);
        }
      }
      return new Error("the stream failed");
    }
    default:
      return new Error("the stream failed");
  }
}
