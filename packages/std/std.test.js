// @flow
//
// `@uniflowed/std`: the Go standard library modules that JavaScript is missing.
//
// Thirteen modules, and what is asserted here is the property that makes each one
// worth importing rather than the fact that it returns something. A `heap` that
// pops in the wrong order is a heap; an `errors.is` that hangs on a cycle
// answers every question correctly until the one that matters; a `Group` that
// leaves an unhandled rejection behind exits the process on Node. Each of those
// is a test below.
//
// The negative type tests — that `as` narrows, that a `Key<string>` cannot read
// a `number` — are not here, because no amount of running proves a program
// would have been refused. They are in `tests/type-tests/std-inference.js`, and
// the last `describe` runs them.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "@uniflowed/test";

import {
  Builder,
  compare,
  concat,
  contains,
  equal,
  fromUtf8,
  hasPrefix,
  hasSuffix,
  indexOf,
  join,
  lastIndexOf,
  repeat,
  split,
  toUtf8,
  trimPrefix,
  trimSuffix,
} from "@uniflowed/std/bytes";
import {
  InvalidBase32Error,
  decode as decodeBase32,
  decodedLength as decodedBase32Length,
  encode as encodeBase32,
  encodedLength as encodedBase32Length,
  isValid as isValidBase32,
} from "@uniflowed/std/base32";
import {
  BIG_ENDIAN,
  Cursor,
  InvalidBinaryError,
  LITTLE_ENDIAN,
  putUvarint,
  putVarint,
  uvarint,
  uvarintLength,
  varint,
  varintLength,
} from "@uniflowed/std/binary";
import {
  CANCELLED,
  DEADLINE_EXCEEDED,
  background,
  fromSignal,
  key,
  withCancel,
  withDeadline,
  withTimeout,
  withValue,
} from "@uniflowed/std/context";
import { InvalidCsvError, parse as parseCsv, stringify as stringifyCsv } from "@uniflowed/std/csv";
import { as, chain, is, join as joinErrors, unwrap, wrap } from "@uniflowed/std/errors";
import { GlobPattern, glob, matchGlob } from "@uniflowed/std/glob";
import { Heap, heapify } from "@uniflowed/std/heap";
import { InvalidHexError, decode, dump, encode, isValid } from "@uniflowed/std/hex";
import { Element, List } from "@uniflowed/std/list";
import {
  basename,
  dirname,
  extname,
  isAbsolute,
  join as joinPath,
  normalize,
  relative,
} from "@uniflowed/std/path";
import { binarySearch, binarySearchBy, search } from "@uniflowed/std/slices";
import { Group, Mutex, Semaphore, WaitGroup, once } from "@uniflowed/std/sync";

import { everyMisuseIsReported } from "../../tests/library/type-tests.js";

/** This checkout, for the last `describe`, which reads three files of it. */
const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");

/** Bytes from a list of numbers, which is what every fixture below wants. */
const b = (...values: Array<number>): Uint8Array => new Uint8Array(values);

/** A promise that settles after `ms`, for the deadline tests. */
const sleep = (ms: number): Promise<void> =>
  new Promise((resolve) => {
    setTimeout(resolve, ms);
  });

describe("errors", () => {
  class HttpError extends Error {
    status: number;
    constructor(status: number) {
      super(`HTTP ${String(status)}`);
      this.status = status;
    }
  }

  it("finds a sentinel through any depth of wrapping", () => {
    const sentinel = new Error("connection refused");
    const failure = wrap("loading user 7", wrap("querying users", sentinel));

    expect(is(failure, sentinel)).toBe(true);
    expect(is(failure, new Error("connection refused"))).toBe(false);
  });

  it("returns the wrapped error at the type of the class it was asked for", () => {
    const failure = wrap("rendering the page", new HttpError(429));

    const http = as(failure, HttpError);
    expect(http).not.toBe(null);
    // The point of `as` over a hand-written `instanceof` walk: the field is
    // reachable without a cast, and `tests/type-tests` proves it is typed.
    expect(http?.status).toBe(429);
    expect(as(failure, TypeError)).toBe(null);
  });

  it("terminates on a cycle instead of hanging", () => {
    // The failure mode the hand-written `while (e) e = e.cause` loop has, and
    // the reason this module exists rather than being three lines at each call
    // site. A retry that wraps its own last failure builds this by accident.
    const first: { cause?: mixed } = new Error("first");
    const second = new Error("second", { cause: first });
    first.cause = second;

    expect(is(first, second)).toBe(true);
    expect(is(first, new Error("elsewhere"))).toBe(false);
    expect(chain(first)).toHaveLength(2);
  });

  it("searches every branch of a join, not just the first", () => {
    const timeout = new Error("timed out");
    const failure = joinErrors(new Error("disk full"), wrap("second shard", timeout));

    expect(is(failure, timeout)).toBe(true);
    expect(failure).toBeInstanceOf(AggregateError);
  });

  it("joins nothing to null and one error to itself", () => {
    // `null` for nothing is what lets a caller collect optional failures and
    // ask one question at the end; returning an empty aggregate would make
    // "did anything fail" a length check.
    expect(joinErrors()).toBe(null);
    expect(joinErrors(null, undefined)).toBe(null);

    const only = new Error("alone");
    expect(joinErrors(only)).toBe(only);
    expect(joinErrors(null, only, undefined)).toBe(only);
  });

  it("asks an error that has an opinion about what it matches", () => {
    const SENTINEL = new Error("not found");
    class Errno extends Error {
      matches(target: mixed): boolean {
        return target === SENTINEL;
      }
    }

    expect(is(wrap("reading", new Errno("ENOENT")), SENTINEL)).toBe(true);
    expect(is(wrap("reading", new Errno("ENOENT")), new Error("other"))).toBe(false);
  });

  it("never matches a nullish target", () => {
    // Otherwise `is(e, e.cause)` on an unwrapped error would be true for every
    // error in the program, because `undefined` is in every chain's tail.
    expect(is(new Error("x"), null)).toBe(false);
    expect(is(new Error("x"), undefined)).toBe(false);
  });

  it("unwraps exactly one level, and answers undefined at the bottom", () => {
    const bottom = new Error("bottom");
    expect(unwrap(wrap("top", bottom))).toBe(bottom);
    expect(unwrap(bottom)).toBe(undefined);
    expect(unwrap("not an error at all")).toBe(undefined);
  });
});

describe("sync", () => {
  describe("WaitGroup", () => {
    it("resolves when the counter empties, and immediately when it is already zero", async () => {
      const group = new WaitGroup();
      let finished = false;

      group.add(2);
      const waited = group.wait().then(() => {
        finished = true;
      });

      group.done();
      await Promise.resolve();
      expect(finished).toBe(false);
      expect(group.count()).toBe(1);

      group.done();
      await waited;
      expect(finished).toBe(true);

      // A second wait on an empty group does not park.
      await group.wait();
    });

    it("throws rather than letting the counter go negative", () => {
      // A `done` without an `add` means some other `wait` has already been told
      // the work finished. Failing here is the only place it can be seen.
      const group = new WaitGroup();
      expect(() => group.done()).toThrow();
    });
  });

  describe("Mutex", () => {
    it("serialises a read-modify-write that yields in the middle", async () => {
      // The hazard that exists on a single-threaded runtime: two async
      // functions interleave at the `await`, and one increment is lost.
      const store = { count: 0 };
      const unguarded = async () => {
        const seen = store.count;
        await sleep(1);
        store.count = seen + 1;
      };
      await Promise.all([unguarded(), unguarded()]);
      expect(store.count).toBe(1);

      store.count = 0;
      const mutex = new Mutex();
      const guarded = () =>
        mutex.withLock(async () => {
          const seen = store.count;
          await sleep(1);
          store.count = seen + 1;
        });
      await Promise.all([guarded(), guarded()]);
      expect(store.count).toBe(2);
    });

    it("releases once however many times the release is called", async () => {
      const mutex = new Mutex();
      const release = await mutex.lock();
      expect(mutex.locked()).toBe(true);

      release();
      release();
      release();

      // Three releases for one lock must not have handed out two permits: if
      // they had, both of these would be granted at once.
      const first = await mutex.lock();
      expect(mutex.locked()).toBe(true);
      let second = false;
      mutex.lock().then(() => {
        second = true;
      });
      await sleep(1);
      expect(second).toBe(false);
      first();
    });

    it("releases the lock when the body throws", async () => {
      const mutex = new Mutex();
      await expect(
        mutex.withLock(() => {
          throw new Error("no");
        }),
      ).rejects.toThrow("no");
      expect(mutex.locked()).toBe(false);
    });
  });

  describe("Semaphore", () => {
    it("never lets more than its permits run at once", async () => {
      const limit = new Semaphore(3);
      let running = 0;
      let peak = 0;

      await Promise.all(
        Array.from({ length: 20 }, () =>
          limit.withPermit(async () => {
            running += 1;
            peak = Math.max(peak, running);
            await sleep(1);
            running -= 1;
          }),
        ),
      );

      expect(peak).toBe(3);
      expect(limit.available()).toBe(3);
    });

    it("wakes waiters in arrival order", async () => {
      const limit = new Semaphore(1);
      const order: Array<number> = [];
      const held = await limit.acquire();

      const waiters = [1, 2, 3].map((n) =>
        limit.acquire().then((release) => {
          order.push(n);
          release();
        }),
      );
      expect(limit.waiting()).toBe(3);

      held();
      await Promise.all(waiters);
      // FIFO, so nobody can be starved by a stream of later arrivals.
      expect(order).toEqual([1, 2, 3]);
    });

    it("refuses a semaphore that can never admit anybody", () => {
      expect(() => new Semaphore(0)).toThrow();
    });
  });

  describe("once", () => {
    it("runs at most once and hands every caller the same value", () => {
      let calls = 0;
      const make = once(() => {
        calls += 1;
        return { id: calls };
      });

      const first = make();
      expect(make()).toBe(first);
      expect(make()).toBe(first);
      expect(calls).toBe(1);
    });

    it("gives concurrent callers the same in-flight promise", async () => {
      let calls = 0;
      const connect = once(async () => {
        calls += 1;
        await sleep(5);
        return "connection";
      });

      const [a, c] = await Promise.all([connect(), connect()]);
      expect(a).toBe("connection");
      expect(c).toBe("connection");
      // The property a hand-written `if (cached == null)` does not have: the
      // check and the assignment are separated by an `await`, so both callers
      // pass the check.
      expect(calls).toBe(1);
    });

    it("remembers a failure rather than retrying it", () => {
      let calls = 0;
      const make = once(() => {
        calls += 1;
        throw new Error("nope");
      });

      expect(() => make()).toThrow("nope");
      expect(() => make()).toThrow("nope");
      expect(calls).toBe(1);
    });
  });

  describe("Group", () => {
    it("returns results in submission order, not completion order", async () => {
      const group = new Group<number>();
      group.go(async () => {
        await sleep(10);
        return 1;
      });
      group.go(() => 2);
      group.go(async () => {
        await sleep(5);
        return 3;
      });

      expect(await group.wait()).toEqual([1, 2, 3]);
    });

    it("bounds how many run at once", async () => {
      const group = new Group<void>({ limit: 2 });
      let running = 0;
      let peak = 0;
      for (let n = 0; n < 10; n += 1) {
        group.go(async () => {
          running += 1;
          peak = Math.max(peak, running);
          await sleep(1);
          running -= 1;
        });
      }

      await group.wait();
      expect(peak).toBe(2);
    });

    it("tells the rest to stop when one fails, and reports the first failure", async () => {
      const group = new Group<string>();
      let aborted = false;

      group.go(async (signal) => {
        signal.addEventListener("abort", () => {
          aborted = true;
        });
        await sleep(20);
        return "slow";
      });
      group.go(async () => {
        await sleep(1);
        throw new Error("first");
      });

      await expect(group.wait()).rejects.toThrow("first");
      expect(aborted).toBe(true);
    });

    it("observes a failure nobody was waiting for yet", async () => {
      // The `Promise.all` hazard: a task that rejects before anything awaits
      // the group is an unhandled rejection, which on Node exits the process.
      const unhandled: Array<mixed> = [];
      const watch = (reason: mixed) => {
        unhandled.push(reason);
      };
      process.on("unhandledRejection", watch);
      try {
        const group = new Group<void>();
        group.go(() => Promise.reject(new Error("early")));
        // Two macrotasks is well past the turn Node reports an unhandled
        // rejection on.
        await sleep(5);
        await expect(group.wait()).rejects.toThrow("early");
      } finally {
        process.off("unhandledRejection", watch);
      }
      expect(unhandled).toEqual([]);
    });

    it("refuses to start work after wait", async () => {
      const group = new Group<void>();
      group.go(() => {});
      await group.wait();
      expect(() => group.go(() => {})).toThrow("after wait");
    });
  });
});

describe("context", () => {
  it("cancels the whole subtree from the top", () => {
    const [root, cancel] = withCancel(background());
    const [child] = withCancel(root);
    const [grandchild] = withCancel(child);

    expect(grandchild.err()).toBe(null);
    cancel();

    expect(root.err()).toBe(CANCELLED);
    expect(child.err()).toBe(CANCELLED);
    expect(grandchild.err()).toBe(CANCELLED);
    expect(grandchild.signal().aborted).toBe(true);
  });

  it("keeps the first ending rather than the last", () => {
    const [ctx, cancel] = withCancel(background());
    const reason = new Error("client went away");

    cancel(reason);
    cancel();

    expect(ctx.err()).toBe(reason);
  });

  it("resolves done when the scope ends, and reports the reason on the signal too", async () => {
    const [ctx, cancel] = withCancel(background());
    let done = false;
    const waiting = ctx.done().then(() => {
      done = true;
    });

    expect(done).toBe(false);
    cancel();
    await waiting;
    expect(done).toBe(true);
    expect(ctx.signal().reason).toBe(CANCELLED);
  });

  it("expires on its deadline", async () => {
    const [ctx, cancel] = withTimeout(background(), 5);
    try {
      await ctx.done();
      expect(ctx.err()).toBe(DEADLINE_EXCEEDED);
    } finally {
      cancel();
    }
  });

  it("lets a child shorten a deadline and not extend one", () => {
    const at = Date.now() + 10_000;
    const [outer, cancelOuter] = withDeadline(background(), at);
    const [shorter, cancelShorter] = withDeadline(outer, at - 5_000);
    const [longer, cancelLonger] = withDeadline(outer, at + 5_000);

    try {
      expect(outer.deadline()).toBe(at);
      expect(shorter.deadline()).toBe(at - 5_000);
      // The whole point: a library cannot buy itself more time than its caller
      // allowed by asking for it.
      expect(longer.deadline()).toBe(at);
    } finally {
      cancelOuter();
      cancelShorter();
      cancelLonger();
    }
  });

  it("ends immediately on a deadline already in the past", () => {
    const [ctx, cancel] = withDeadline(background(), Date.now() - 1);
    try {
      // Synchronously, so the line after the constructor sees a dead context
      // rather than one that dies on the next tick.
      expect(ctx.err()).toBe(DEADLINE_EXCEEDED);
    } finally {
      cancel();
    }
  });

  it("is born cancelled under a parent that already ended", () => {
    const [parent, cancel] = withCancel(background());
    cancel();
    const [child] = withCancel(parent);

    expect(child.err()).toBe(CANCELLED);
  });

  it("reads a value from the nearest scope that stored one", () => {
    const TRACE = key<string>("trace");
    const USER = key<string>("user");

    const base = withValue(background(), TRACE, "abc");
    const inner = withValue(withValue(base, USER, "ada"), TRACE, "def");

    expect(inner.value(TRACE)).toBe("def");
    expect(inner.value(USER)).toBe("ada");
    // Shadowing is one-way: what the caller sees is untouched.
    expect(base.value(TRACE)).toBe("abc");
    expect(base.value(USER)).toBe(undefined);
    expect(background().value(TRACE)).toBe(undefined);
  });

  it("keys two values apart even when they share a name", () => {
    // A key is compared by identity, which is what makes a key a library did
    // not export unreadable by anybody else.
    const mine = key<string>("id");
    const theirs = key<string>("id");
    const ctx = withValue(background(), mine, "mine");

    expect(ctx.value(mine)).toBe("mine");
    expect(ctx.value(theirs)).toBe(undefined);
  });

  it("gives a value scope its parent's cancellation rather than one of its own", async () => {
    // A value scope allocates no controller and registers no listener, because
    // a listener on a long-lived parent with nobody to remove it is the leak
    // that makes a per-request tree a staircase. What it costs instead is that
    // `signal`, `err` and `done` have to be the parent's, exactly — and a
    // second signal kept in step by hand is what this avoids having.
    const TAG = key<string>("tag");
    const [parent, cancel] = withCancel(background());
    const tagged = withValue(parent, TAG, "v");

    expect(tagged.signal()).toBe(parent.signal());
    expect(tagged.err()).toBe(null);

    let done = false;
    const waiting = tagged.done().then(() => {
      done = true;
    });
    cancel();
    await waiting;

    expect(done).toBe(true);
    expect(tagged.err()).toBe(CANCELLED);
    expect(tagged.signal().aborted).toBe(true);
    // And the value survives the ending, because a reporter reads it after.
    expect(tagged.value(TAG)).toBe("v");
  });

  it("adopts a signal somebody else owns", () => {
    const controller = new AbortController();
    const ctx = fromSignal(controller.signal);
    const [child] = withCancel(ctx);

    const reason = new Error("request closed");
    controller.abort(reason);

    expect(ctx.err()).toBe(reason);
    expect(child.err()).toBe(reason);
  });

  it("adopts a signal that is already aborted", () => {
    const controller = new AbortController();
    controller.abort();
    expect(fromSignal(controller.signal).err()).not.toBe(null);
  });

  it("never ends the root", () => {
    expect(background().err()).toBe(null);
    expect(background().deadline()).toBe(null);
    expect(background().signal().aborted).toBe(false);
    expect(background()).toBe(background());
  });
});

describe("bytes", () => {
  it("compares by byte and then by length", () => {
    expect(equal(b(1, 2, 3), b(1, 2, 3))).toBe(true);
    expect(equal(b(1, 2, 3), b(1, 2))).toBe(false);
    expect(equal(b(), b())).toBe(true);

    expect(compare(b(1, 2), b(1, 2))).toBe(0);
    expect(compare(b(1, 2), b(1, 3))).toBe(-1);
    // A prefix sorts before what it is a prefix of.
    expect(compare(b(1, 2), b(1, 2, 0))).toBe(-1);
    // Unsigned, so 0xff is the largest byte and not -1.
    expect(compare(b(0xff), b(0x01))).toBe(1);

    const sorted = [b(2), b(1, 1), b(1)].sort(compare);
    expect(sorted.map((each) => Array.from(each))).toEqual([[1], [1, 1], [2]]);
  });

  it("finds a needle that restarts inside a partial match", () => {
    // The bug the first-byte skip could have introduced: `aab` inside `aaab`
    // begins at 1, which a scan that resumed after the failed match would miss.
    expect(indexOf(b(0x61, 0x61, 0x61, 0x62), b(0x61, 0x61, 0x62))).toBe(1);
    expect(indexOf(b(1, 2, 3), b(2, 3))).toBe(1);
    expect(indexOf(b(1, 2, 3), b(3, 4))).toBe(-1);
    expect(indexOf(b(1, 2, 3), b(1, 2, 3, 4))).toBe(-1);
    expect(indexOf(b(1, 2, 1, 2), b(1, 2), 1)).toBe(2);
    expect(lastIndexOf(b(1, 2, 1, 2), b(1, 2))).toBe(2);
    expect(lastIndexOf(b(1, 2), b(3))).toBe(-1);

    expect(contains(b(1, 2, 3), b(2))).toBe(true);
    expect(hasPrefix(b(1, 2, 3), b(1, 2))).toBe(true);
    expect(hasPrefix(b(1), b(1, 2))).toBe(false);
    expect(hasSuffix(b(1, 2, 3), b(2, 3))).toBe(true);
  });

  it("treats an empty needle the way String.indexOf does", () => {
    expect(indexOf(b(1, 2), b())).toBe(0);
    expect(indexOf(b(1, 2), b(), 1)).toBe(1);
    expect(indexOf(b(1, 2), b(), 9)).toBe(-1);
    expect(lastIndexOf(b(1, 2), b())).toBe(2);
  });

  it("round-trips through split and join", () => {
    const line = fromUtf8("alpha,beta,,gamma");
    const comma = fromUtf8(",");
    const pieces = split(line, comma);

    expect(pieces.map(toUtf8)).toEqual(["alpha", "beta", "", "gamma"]);
    expect(equal(join(pieces, comma), line)).toBe(true);
    // Separators at either end produce the empty pieces around them, which is
    // what makes the round trip exact rather than approximately right.
    expect(split(fromUtf8(",a,"), comma).map(toUtf8)).toEqual(["", "a", ""]);
    expect(split(fromUtf8("none"), comma).map(toUtf8)).toEqual(["none"]);
  });

  it("refuses to split on nothing", () => {
    expect(() => split(b(1, 2), b())).toThrow("non-empty separator");
  });

  it("hands back views, not copies", () => {
    const source = b(1, 2, 3, 4);
    const trimmed = trimPrefix(source, b(1));

    // Writing through the view writes through to the source, which is the
    // documented consequence of not copying — and the reason the docs say to
    // call `.slice()` when a copy is wanted.
    trimmed[0] = 9;
    expect(source[1]).toBe(9);

    // No prefix means the same object, so identity answers "was there one".
    expect(trimPrefix(source, b(7))).toBe(source);
    expect(trimSuffix(source, b(7))).toBe(source);
    expect(Array.from(trimSuffix(b(1, 2, 3), b(3)))).toEqual([1, 2]);
  });

  it("concatenates and repeats", () => {
    expect(Array.from(concat([b(1), b(2, 3), b()]))).toEqual([1, 2, 3]);
    expect(Array.from(concat([]))).toEqual([]);
    expect(Array.from(repeat(b(1, 2), 3))).toEqual([1, 2, 1, 2, 1, 2]);
    expect(Array.from(repeat(b(1), 0))).toEqual([]);
    expect(() => repeat(b(1), -1)).toThrow();
  });

  it("round-trips UTF-8 including outside the basic plane", () => {
    for (const text of ["", "ascii", "日本語", "🌱 seed"]) {
      expect(toUtf8(fromUtf8(text))).toBe(text);
    }
  });

  describe("Builder", () => {
    it("grows past its capacity and keeps everything", () => {
      const out = new Builder(2);
      for (let n = 0; n < 100; n += 1) {
        out.writeByte(n);
      }
      out.write(b(200, 201));
      out.writeUtf8("hi");

      const written = out.bytes();
      expect(written).toHaveLength(104);
      expect(Array.from(written.subarray(0, 3))).toEqual([0, 1, 2]);
      expect(Array.from(written.subarray(100))).toEqual([200, 201, 0x68, 0x69]);
      expect(out.length()).toBe(104);
    });

    it("hands back a copy from bytes and a view from view", () => {
      const out = new Builder();
      out.write(b(1, 2, 3));

      const copied = out.bytes();
      const viewed = out.view();
      out.writeByte(4);

      // The copy is what it was when it was taken; the view has moved on. This
      // is why `bytes()` is the default and `view()` carries the warning.
      expect(copied).toHaveLength(3);
      expect(viewed).toHaveLength(3);
      expect(out.length()).toBe(4);
    });

    it("keeps its capacity across a reset", () => {
      const out = new Builder();
      out.write(repeat(b(0), 500));
      out.reset();

      expect(out.length()).toBe(0);
      expect(out.bytes()).toHaveLength(0);
      out.writeByte(1);
      expect(Array.from(out.bytes())).toEqual([1]);
    });
  });
});

describe("heap", () => {
  it("pops in the comparator's order, not insertion order", () => {
    const queue = new Heap<number>((x, y) => x - y);
    for (const value of [5, 3, 8, 1, 9, 2]) {
      queue.push(value);
    }

    expect(queue.size()).toBe(6);
    expect(queue.peek()).toBe(1);
    expect([...queue.drain()]).toEqual([1, 2, 3, 5, 8, 9]);
    expect(queue.isEmpty()).toBe(true);
    expect(queue.pop()).toBe(undefined);
    expect(queue.peek()).toBe(undefined);
  });

  it("is a max-heap when the comparator is reversed", () => {
    const queue = heapify([1, 4, 2, 8], (x, y) => y - x);
    expect([...queue.drain()]).toEqual([8, 4, 2, 1]);
  });

  it("heapifies an existing array without disturbing it", () => {
    const source = [9, 4, 7, 1, 8, 2, 6];
    const queue = heapify(source, (x, y) => x - y);

    expect([...queue.drain()]).toEqual([1, 2, 4, 6, 7, 8, 9]);
    expect(source).toEqual([9, 4, 7, 1, 8, 2, 6]);
  });

  it("agrees with a full sort on a shuffled input", () => {
    // The property test that would have caught a sift written the wrong way
    // round: the heap and `Array.sort` are two independent implementations of
    // the same order.
    const values = Array.from({ length: 500 }, (_unused, index) => (index * 37) % 500);
    const queue = heapify(values, (x, y) => x - y);
    expect([...queue.drain()]).toEqual([...values].sort((x, y) => x - y));
  });

  it("replaces the top in one sift", () => {
    const queue = heapify([1, 2, 3], (x, y) => x - y);

    expect(queue.replace(10)).toBe(1);
    expect(queue.size()).toBe(3);
    expect(queue.peek()).toBe(2);
    // On an empty heap it is a plain push.
    const empty = new Heap<number>((x, y) => x - y);
    expect(empty.replace(4)).toBe(undefined);
    expect(empty.peek()).toBe(4);
  });

  it("stops where a caller stops, leaving the rest in the heap", () => {
    const queue = heapify([1, 2, 3, 4, 5], (x, y) => x - y);
    const taken: Array<number> = [];
    for (const value of queue.drain()) {
      if (value > 2) {
        break;
      }
      taken.push(value);
    }

    expect(taken).toEqual([1, 2]);
    // The one that ended the loop was taken; the rest were never touched.
    expect(queue.size()).toBe(2);
    expect(queue.peek()).toBe(4);
  });

  it("clears and reports what it holds", () => {
    const queue = heapify([3, 1, 2], (x, y) => x - y);
    expect([...queue.toArray()].sort((x, y) => x - y)).toEqual([1, 2, 3]);
    queue.clear();
    expect(queue.size()).toBe(0);
  });
});

describe("list", () => {
  it("keeps stable element handles while values move", () => {
    const recent = new List<string>();
    const inbox = recent.pushBack("inbox");
    const home = recent.pushBack("home");
    const searchItem = recent.pushFront("search");

    expect(recent.size()).toBe(3);
    expect(recent.front()).toBe(searchItem);
    expect(recent.back()).toBe(home);
    expect(recent.toArray()).toEqual(["search", "inbox", "home"]);

    recent.moveToFront(home);
    expect(recent.front()).toBe(home);
    expect(searchItem.previous()).toBe(home);
    expect(searchItem.next()).toBe(inbox);
    expect([...recent.values()]).toEqual(["home", "search", "inbox"]);
  });

  it("inserts before and after existing elements in constant time", () => {
    const queue = new List<number>([2, 4]);
    const four = queue.back();
    if (four == null) throw new Error("expected a back element");

    const one = queue.insertBefore(1, queue.front() ?? four);
    const three = queue.insertBefore(3, four);
    const five = queue.insertAfter(5, four);

    expect(queue.toArray()).toEqual([1, 2, 3, 4, 5]);
    expect(one.next()?.value()).toBe(2);
    expect(three.previous()?.value()).toBe(2);
    expect(five.previous()).toBe(four);
  });

  it("moves elements relative to other elements without changing handles", () => {
    const list = new List<string>(["a", "b", "c", "d"]);
    const a = list.front();
    const d = list.back();
    if (a == null || d == null) throw new Error("expected both ends");
    const b = a.next();
    const c = d.previous();
    if (b == null || c == null) throw new Error("expected middle elements");

    list.moveAfter(a, c);
    expect(list.toArray()).toEqual(["b", "c", "a", "d"]);
    expect(a.previous()).toBe(c);

    list.moveBefore(d, b);
    expect(list.toArray()).toEqual(["d", "b", "c", "a"]);
    expect(list.front()).toBe(d);
    expect(list.back()).toBe(a);
  });

  it("removes and clears by detaching elements", () => {
    const list = new List<number>([1, 2, 3]);
    const two = list.front()?.next();
    if (two == null) throw new Error("expected a middle element");

    expect(list.remove(two)).toBe(2);
    expect(list.toArray()).toEqual([1, 3]);
    expect(two.next()).toBe(undefined);
    expect(two.previous()).toBe(undefined);
    expect(() => list.remove(two)).toThrow(RangeError);

    const front = list.front();
    list.clear();
    expect(list.isEmpty()).toBe(true);
    expect(list.front()).toBe(undefined);
    expect(front?.next()).toBe(undefined);
  });

  it("refuses elements from another list or no list", () => {
    const left = new List<string>(["a"]);
    const right = new List<string>(["b"]);
    const alien = right.front();
    if (alien == null) throw new Error("expected an element");

    expect(() => left.insertAfter("x", alien)).toThrow(RangeError);
    expect(() => left.moveToBack(alien)).toThrow(RangeError);
    expect(() => left.remove(new Element("detached"))).toThrow(RangeError);
  });
});

describe("hex", () => {
  it("round-trips every byte value", () => {
    const all = new Uint8Array(256);
    for (let value = 0; value < 256; value += 1) {
      all[value] = value;
    }

    const text = encode(all);
    expect(text).toHaveLength(512);
    expect(text.startsWith("000102")).toBe(true);
    expect(text.endsWith("fdfeff")).toBe(true);
    expect(equal(decode(text), all)).toBe(true);
    expect(encode(new Uint8Array(0))).toBe("");
    expect(decode("")).toHaveLength(0);
  });

  it("agrees with itself on both sides of the length its two paths split on", () => {
    // `encode` appends to a string for short inputs and goes through a
    // `TextDecoder` for long ones, because the two cross at about 128 bytes.
    // A fork chosen by length is a fork that can disagree with itself, so this
    // walks across it against a reference nobody would ship.
    const reference = (input: Uint8Array): string =>
      Array.from(input, (byte) => byte.toString(16).padStart(2, "0")).join("");

    for (const size of [0, 1, 127, 128, 129, 1000]) {
      const input = new Uint8Array(size);
      for (let index = 0; index < size; index += 1) {
        input[index] = (index * 37) & 0xff;
      }
      expect(encode(input)).toBe(reference(input));
      expect(equal(decode(encode(input)), input)).toBe(true);
    }
  });

  it("accepts either case and produces lowercase", () => {
    expect(encode(b(0xde, 0xad))).toBe("dead");
    expect(Array.from(decode("DEAD"))).toEqual([0xde, 0xad]);
    expect(Array.from(decode("dEaD"))).toEqual([0xde, 0xad]);
  });

  it("names the offset of the first character that is not a hex digit", () => {
    // The failure `parseInt` swallows: it answers `NaN`, which becomes zero on
    // its way into a `Uint8Array`, so a corrupted digest silently decodes to a
    // different valid-looking one.
    try {
      // Even length, so it gets past the length check and the offset is the
      // character rather than the end.
      decode("dead  beef");
      throw new Error("decode should have refused");
    } catch (error) {
      expect(error).toBeInstanceOf(InvalidHexError);
      expect(error.offset).toBe(4);
    }

    // An odd length is reported at the end, because that is where the missing
    // digit would have been — a truncated token, which is the common case.
    try {
      decode("abc");
      throw new Error("decode should have refused");
    } catch (error) {
      expect(error).toBeInstanceOf(InvalidHexError);
      expect(error.message).toContain("odd-length");
      expect(error.offset).toBe(3);
    }

    expect(isValid("abc")).toBe(false);
    expect(isValid("dead  beef")).toBe(false);
    expect(isValid("deadbeef")).toBe(true);
    expect(isValid("")).toBe(true);
  });

  it("dumps in the format hexdump -C prints", () => {
    const dumped = dump(fromUtf8("GET / HTTP/1.1\r\nHost: a.b\r\n"));

    expect(dumped.split("\n")).toEqual([
      "00000000  47 45 54 20 2f 20 48 54  54 50 2f 31 2e 31 0d 0a  |GET / HTTP/1.1..|",
      "00000010  48 6f 73 74 3a 20 61 2e  62 0d 0a                 |Host: a.b..|",
      "",
    ]);
    expect(dump(new Uint8Array(0))).toBe("");
  });

  it("never lets a byte in a dump move the cursor", () => {
    // A dump that printed control characters could rewrite the line above it,
    // which would make it lie about the buffer it was printing.
    const dumped = dump(b(0x1b, 0x5b, 0x32, 0x4a));
    expect(dumped.includes("")).toBe(false);
    expect(dumped.trimEnd().endsWith("|.[2J|")).toBe(true);
  });
});

describe("base32", () => {
  it("round-trips the RFC 4648 test vectors", () => {
    const vectors = [
      ["", ""],
      ["f", "MY======"],
      ["fo", "MZXQ===="],
      ["foo", "MZXW6==="],
      ["foob", "MZXW6YQ="],
      ["fooba", "MZXW6YTB"],
      ["foobar", "MZXW6YTBOI======"],
    ];
    for (const [plain, encoded] of vectors) {
      const bytes = fromUtf8(plain);
      expect(encodeBase32(bytes)).toBe(encoded);
      expect(toUtf8(decodeBase32(encoded))).toBe(plain);
    }
  });

  it("encodes without padding when a caller asks for the TOTP shape", () => {
    expect(encodeBase32(fromUtf8("foobar"), { padding: "omit" })).toBe("MZXW6YTBOI");
    expect(encodedBase32Length(6, { padding: "omit" })).toBe(10);
    expect(encodedBase32Length(6)).toBe(16);
    expect(decodedBase32Length(10)).toBe(6);
  });

  it("accepts unpadded lowercase input", () => {
    expect(toUtf8(decodeBase32("mzxw6ytboi"))).toBe("foobar");
    expect(isValidBase32("mzxw6ytboi")).toBe(true);
  });

  it("names invalid characters and padding by offset", () => {
    expect(() => decodeBase32("M!======")).toThrow(InvalidBase32Error);
    let offset = -1;
    try {
      decodeBase32("M!======");
    } catch (failure) {
      expect(failure).toBeInstanceOf(InvalidBase32Error);
      if (failure instanceof InvalidBase32Error) {
        offset = failure.offset;
      }
    }
    expect(offset).toBe(1);

    expect(() => decodeBase32("MZX=6===")).toThrow(InvalidBase32Error);
    expect(isValidBase32("MZ======")).toBe(false);
  });
});

describe("csv", () => {
  it("parses quoted commas, escaped quotes and embedded newlines", () => {
    const source = 'name,note\r\nAda,"hello, ""Flow"""\r\nBob,"line 1\nline 2"';

    expect(parseCsv(source)).toEqual([
      ["name", "note"],
      ["Ada", 'hello, "Flow"'],
      ["Bob", "line 1\nline 2"],
    ]);
  });

  it("serializes only the fields that need quoting", () => {
    const rows = [
      ["plain", "needs,comma"],
      ['quote "here"', "line\nbreak"],
      [" space", "tab\t"],
    ];
    const text = stringifyCsv(rows, { lineTerminator: "\r\n" });

    expect(text).toBe('plain,"needs,comma"\r\n"quote ""here""","line\nbreak"\r\n" space","tab\t"');
    expect(parseCsv(text)).toEqual(rows);
  });

  it("checks record width by default and can allow ragged rows", () => {
    expect(() => parseCsv("a,b\nc")).toThrow(InvalidCsvError);
    expect(parseCsv("a,b\nc", { fieldsPerRecord: "variable" })).toEqual([["a", "b"], ["c"]]);
    expect(() => parseCsv("a,b", { fieldsPerRecord: 3 })).toThrow(InvalidCsvError);
  });

  it("names malformed quoted fields with a source position", () => {
    try {
      parseCsv('ok\n"unterminated');
      throw new Error("parse should have refused");
    } catch (failure) {
      expect(failure).toBeInstanceOf(InvalidCsvError);
      if (failure instanceof InvalidCsvError) {
        expect(failure.line).toBe(2);
        expect(failure.column).toBe(1);
      }
    }

    expect(() => parseCsv('a"b')).toThrow(InvalidCsvError);
  });
});

describe("binary", () => {
  it("advances a cursor over checked DataView reads and writes", () => {
    const bytes = new Uint8Array(16);
    const writer = new Cursor(bytes);

    writer.putUint16(0x1234, BIG_ENDIAN);
    writer.putUint32(0x89abcdef, LITTLE_ENDIAN);
    writer.putInt16(-2);
    writer.putFloat32(1.5, LITTLE_ENDIAN);

    expect(writer.offset()).toBe(12);
    expect(Array.from(bytes.slice(0, 8))).toEqual([0x12, 0x34, 0xef, 0xcd, 0xab, 0x89, 0xff, 0xfe]);

    const reader = new Cursor(bytes);
    expect(reader.getUint16()).toBe(0x1234);
    expect(reader.getUint32(LITTLE_ENDIAN)).toBe(0x89abcdef);
    expect(reader.getInt16()).toBe(-2);
    expect(reader.getFloat32(LITTLE_ENDIAN)).toBe(1.5);
    expect(reader.remaining()).toBe(4);
  });

  it("fails before a cursor can read, write or seek outside the buffer", () => {
    const reader = new Cursor(b(0, 1));
    expect(reader.getUint16()).toBe(1);
    expect(() => reader.getUint8()).toThrow(RangeError);
    expect(() => reader.seek(3)).toThrow(RangeError);

    expect(() => new Cursor(b(0)).putUint16(0x100)).toThrow(RangeError);
    expect(() => new Cursor(b(0)).putUint8(0x100)).toThrow(RangeError);
  });

  it("encodes and decodes unsigned varints with the Go wire shape", () => {
    const vectors: Array<[bigint, Array<number>]> = [
      [0n, [0x00]],
      [127n, [0x7f]],
      [128n, [0x80, 0x01]],
      [300n, [0xac, 0x02]],
      [624485n, [0xe5, 0x8e, 0x26]],
    ];

    for (const [value, encoded] of vectors) {
      const bytes = putUvarint(value);
      expect(Array.from(bytes)).toEqual(encoded);
      expect(uvarint(bytes)).toEqual({ value, read: encoded.length });
      expect(uvarintLength(value)).toBe(encoded.length);
    }
  });

  it("encodes and decodes signed zig-zag varints", () => {
    const vectors: Array<[bigint, Array<number>]> = [
      [0n, [0x00]],
      [-1n, [0x01]],
      [1n, [0x02]],
      [-2n, [0x03]],
      [-300n, [0xd7, 0x04]],
      [300n, [0xd8, 0x04]],
    ];

    for (const [value, encoded] of vectors) {
      const bytes = putVarint(value);
      expect(Array.from(bytes)).toEqual(encoded);
      expect(varint(bytes)).toEqual({ value, read: encoded.length });
      expect(varintLength(value)).toBe(encoded.length);
    }
  });

  it("lets a cursor read and write varints in sequence", () => {
    const bytes = new Uint8Array(8);
    const cursor = new Cursor(bytes);
    cursor.putVarint(-300n);
    cursor.putUvarint(624485n);
    expect(cursor.offset()).toBe(5);

    cursor.seek(0);
    expect(cursor.getVarint()).toBe(-300n);
    expect(cursor.getUvarint()).toBe(624485n);
    expect(cursor.offset()).toBe(5);
  });

  it("names truncated varints by offset", () => {
    try {
      uvarint(b(0x80));
      throw new Error("uvarint should have refused a truncated sequence");
    } catch (failure) {
      expect(failure).toBeInstanceOf(InvalidBinaryError);
      if (failure instanceof InvalidBinaryError) {
        expect(failure.offset).toBe(1);
      }
    }

    expect(() => putUvarint(-1n)).toThrow(RangeError);
  });
});

describe("slices", () => {
  it("finds the first true index of a monotone predicate", () => {
    expect(search(8, (index) => index >= 5)).toBe(5);
    expect(search(8, () => false)).toBe(8);
    expect(search(8, () => true)).toBe(0);
  });

  it("refuses a length that is not a searchable array bound", () => {
    expect(() => search(-1, () => true)).toThrow(RangeError);
    expect(() => search(1.5, () => true)).toThrow(RangeError);
    expect(() => search(Number.MAX_SAFE_INTEGER + 1, () => true)).toThrow(RangeError);
  });

  it("finds the first equal item and otherwise returns the insertion point", () => {
    const numbers = [1, 3, 3, 3, 7, 9];
    expect(binarySearch(numbers, 3, (left, right) => left - right)).toEqual({
      index: 1,
      found: true,
    });
    expect(binarySearch(numbers, 6, (left, right) => left - right)).toEqual({
      index: 4,
      found: false,
    });
    expect(binarySearch(numbers, 12, (left, right) => left - right)).toEqual({
      index: 6,
      found: false,
    });
  });

  it("searches objects through a captured target", () => {
    const rows = [{ id: "a" }, { id: "c" }, { id: "f" }];
    const wanted = "c";

    expect(binarySearchBy(rows, (row) => row.id.localeCompare(wanted))).toEqual({
      index: 1,
      found: true,
    });
  });
});

describe("path", () => {
  it("cleans slash paths without asking a host file system", () => {
    expect(normalize("")).toBe(".");
    expect(normalize("./a//b/../c/")).toBe("a/c");
    expect(normalize("/a/../../b")).toBe("/b");
    expect(normalize("../../a")).toBe("../../a");
  });

  it("joins fragments and keeps absolute roots lexical", () => {
    expect(joinPath("routes", "users", "..", "settings")).toBe("routes/settings");
    expect(joinPath("/app/", "/routes", "index.js")).toBe("/app/routes/index.js");
    expect(joinPath()).toBe(".");
  });

  it("names directory, base and extension like Go path helpers", () => {
    expect(dirname("/app/routes/index.flow.js")).toBe("/app/routes");
    expect(dirname("index.js")).toBe(".");
    expect(dirname("/")).toBe("/");
    expect(basename("/app/routes/")).toBe("routes");
    expect(basename("")).toBe(".");
    expect(extname("/app/.env")).toBe(".env");
    expect(extname("archive.tar.gz")).toBe(".gz");
    expect(extname("README")).toBe("");
  });

  it("computes relative paths only inside the same root shape", () => {
    expect(relative("app/routes", "app/routes/admin/index.js")).toBe("admin/index.js");
    expect(relative("app/routes/admin", "app/assets/logo.svg")).toBe("../../assets/logo.svg");
    expect(relative("/app/routes", "/app/routes")).toBe(".");
    expect(isAbsolute("/app/routes")).toBe(true);
    expect(isAbsolute("app/routes")).toBe(false);
    expect(() => relative("/app", "app")).toThrow(RangeError);
    expect(() => relative("../a", "b")).toThrow(RangeError);
  });
});

describe("glob", () => {
  it("matches slash paths without asking a host file system", () => {
    const routes = glob("routes/**/*.flow.js");

    expect(routes).toBeInstanceOf(GlobPattern);
    expect(routes.match("routes/index.flow.js")).toBe(true);
    expect(routes.match("routes/admin/users.flow.js")).toBe(true);
    expect(routes.match("routes/admin/users.js")).toBe(false);
    expect(routes.match("/routes/index.flow.js")).toBe(false);
  });

  it("keeps wildcards inside a segment unless ** owns the segment", () => {
    expect(matchGlob("src/*.js", "src/app.js")).toBe(true);
    expect(matchGlob("src/*.js", "src/routes/app.js")).toBe(false);
    expect(matchGlob("src/**/app.js", "src/app.js")).toBe(true);
    expect(matchGlob("src/**/app.js", "src/routes/admin/app.js")).toBe(true);
  });

  it("does not recurse for deep ** matches or regex-backtrack on starry segments", () => {
    const deep = Array.from({ length: 2_000 }, (_value, index) => `d${String(index)}`).join("/");
    const separatedStars = Array.from({ length: 80 }, () => "*a").join("");

    expect(matchGlob("root/**/file.js", `root/${deep}/file.js`)).toBe(true);
    expect(matchGlob(`${separatedStars}z`, "a".repeat(80))).toBe(false);
  });

  it("supports single-character wildcards, classes and escaped specials", () => {
    expect(matchGlob("pages/[a-c]?/[!x].js", "pages/b1/y.js")).toBe(true);
    expect(matchGlob("pages/[a-c]?/[!x].js", "pages/d1/y.js")).toBe(false);
    expect(matchGlob("assets/\\*.js", "assets/*.js")).toBe(true);
  });

  it("rejects malformed patterns when they are compiled", () => {
    expect(() => glob("src/[.js")).toThrow(SyntaxError);
    expect(() => matchGlob("src/[z-a].js", "src/x.js")).toThrow(SyntaxError);
  });
});

describe("the types", () => {
  it("reports every misuse in tests/type-tests/std-inference.js", () => {
    // Running proves what the code does; only the checker can prove what a
    // different program would have been refused. The fixture is that program.
    everyMisuseIsReported({
      fixture: "tests/type-tests/std-inference.js",
      alongside: ["packages/std"],
      atLeast: 20,
    });
  });
});

describe("what ships is one list in three places", () => {
  // `crates/uf_std/src/registry.rs` is the table, `package.json#exports` is
  // what a program can import, and `docs/app/reference/std` is what a reader is
  // told. Three copies of one fact, and until ubugeeei-prod/uf#710 nothing
  // compared any two of them: the registry named forty-four subpaths, none of
  // which existed, and none of the shipped modules that do.
  //
  // The Rust side of the check is in `crates/uf_lib` — it parses each module
  // with uf's own Flow parser and holds the export lists name for name, which
  // is more than a scan of the text can do. What is left over is the pair only
  // this suite can see: the documentation table, which no cargo test reads.
  //
  // A scan of the source, like `tui.test.js`'s Unicode tables next door. The
  // discomfort of three copies is made mechanical rather than moral: edit one
  // and this fails.

  const ships = (): Array<string> => {
    const source = fs.readFileSync(path.join(REPO, "crates/uf_std/src/registry.rs"), "utf8");
    const found = [];
    for (const match of source.matchAll(/StdModule::ships\(\s*"([^"]+)"/g)) {
      found.push(match[1]);
    }
    return found.sort();
  };

  it("is the same shipped modules in the registry and the package manifest", () => {
    const manifest = JSON.parse(
      fs.readFileSync(path.join(REPO, "packages/std/package.json"), "utf8"),
    );
    // `.` is the declaration surface and `./package.json` is the manifest
    // itself; neither is one of these modules.
    const subpaths = Object.keys(manifest.exports)
      .filter((key) => key !== "." && key !== "./package.json")
      .map((key) => `@uniflowed/std/${key.slice("./".length)}`)
      .sort();

    expect(subpaths.length).toBeGreaterThan(0);
    expect(ships()).toEqual(subpaths);
  });

  it("is the same shipped modules the reference page's table names", () => {
    const page = fs.readFileSync(path.join(REPO, "docs/app/reference/std/$page.mdx"), "utf8");
    const start = page.indexOf("## What ships today");
    expect(start).toBeGreaterThan(-1);
    // To the next heading: the page names these specifiers again further down,
    // in the example and in the prose, and a scan of the whole file would pass
    // on a table that had lost a row.
    const end = page.indexOf("\n## ", start + 1);
    const table = page.slice(start, end === -1 ? page.length : end);

    const documented = [];
    for (const match of table.matchAll(/`(@uniflowed\/std\/[a-z0-9-]+)`/g)) {
      if (!documented.includes(match[1])) documented.push(match[1]);
    }
    expect(documented.sort()).toEqual(ships());
  });
});
