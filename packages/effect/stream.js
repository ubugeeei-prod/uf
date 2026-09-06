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
// `streamToReadableStream`. Consuming a web stream needs nothing but a reader,
// which is why `streamFromReadableStream` is here; producing one means
// constructing a host `ReadableStream` and deciding how the runtime is entered
// from a callback the host calls — a design question rather than a missing
// function, so it is filed as #329 rather than guessed at.
//
// `merge`, `zip`, `buffer` and `fromQueue`, which all want a bounded queue
// with back pressure. There is no `Queue` yet (#328), and these should follow
// it rather than lead it (#329).

import {
  andThen,
  as,
  effect,
  ensuring,
  flatMap,
  forEach,
  map,
  promise,
  succeed,
  suspend,
} from "./index.js";
import type { Effect, EffectGenerator } from "./index.js";

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
