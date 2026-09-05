// @flow
//
// `@uniflowed/cell` under the runner that ships with the toolchain.

import { describe, expect, fn, it } from "@uniflowed/test";
import {
  batch,
  cell,
  computed,
  effect,
  peek,
  read,
  refresh,
  resource,
  snapshot,
  status,
  subscribe,
  untracked,
  update,
  write,
} from "@uniflowed/cell";

describe("cell", () => {
  it("holds and replaces a value", () => {
    const count = cell(1);
    expect(read(count)).toBe(1);
    write(count, 2);
    expect(read(count)).toBe(2);
  });

  it("reduces the current value in one step", () => {
    const count = cell(0);
    update(count, (n) => n + 1);
    update(count, (n) => n + 1);
    expect(read(count)).toBe(2);
  });

  it("wakes subscribers on a change", () => {
    const count = cell(0);
    const listener = fn();
    subscribe(count, listener);
    write(count, 1);
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("drops a write of the value it already holds", () => {
    const count = cell(7);
    const listener = fn();
    subscribe(count, listener);
    write(count, 7);
    expect(listener).not.toHaveBeenCalled();
  });

  it("compares with Object.is, so NaN over NaN is not a change", () => {
    const value = cell(Number.NaN);
    const listener = fn();
    subscribe(value, listener);
    write(value, Number.NaN);
    expect(listener).not.toHaveBeenCalled();
  });

  it("stops calling a listener that unsubscribed", () => {
    const count = cell(0);
    const listener = fn();
    const unsubscribe = subscribe(count, listener);
    write(count, 1);
    unsubscribe();
    write(count, 2);
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("reports its scope and value as a snapshot", () => {
    const count = cell(3);
    expect(snapshot(count)).toEqual({ scope: "client", value: 3 });
  });
});

describe("computed", () => {
  it("derives from the cells it reads, with no dependency list", () => {
    const first = cell(1);
    const second = cell(2);
    const total = computed(() => read(first) + read(second));
    expect(read(total)).toBe(3);
    write(second, 10);
    expect(read(total)).toBe(11);
  });

  it("memoises: a derive runs once for repeated reads", () => {
    const source = cell(1);
    const derive = fn(() => read(source) * 2);
    const doubled = computed(() => Number(derive()));
    expect(read(doubled)).toBe(2);
    expect(read(doubled)).toBe(2);
    expect(derive).toHaveBeenCalledTimes(1);
  });

  it("stays lazy while nothing is watching", () => {
    const source = cell(1);
    const derive = fn(() => read(source));
    const mirror = computed(() => Number(derive()));
    read(mirror);
    write(source, 2);
    write(source, 3);
    expect(derive).toHaveBeenCalledTimes(1);
    expect(read(mirror)).toBe(3);
    expect(derive).toHaveBeenCalledTimes(2);
  });

  it("wakes its own subscribers when a dependency changes", () => {
    const source = cell(1);
    const doubled = computed(() => read(source) * 2);
    const listener = fn();
    subscribe(doubled, listener);
    write(source, 2);
    expect(listener).toHaveBeenCalledTimes(1);
    expect(read(doubled)).toBe(4);
  });

  it("does not wake anyone when the derived value is unchanged", () => {
    const source = cell(1);
    const isPositive = computed(() => read(source) > 0);
    const listener = fn();
    subscribe(isPositive, listener);
    write(source, 2);
    write(source, 3);
    expect(listener).not.toHaveBeenCalled();
    write(source, -1);
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("tracks only the branch it actually took", () => {
    const useLeft = cell(true);
    const left = cell("L");
    const right = cell("R");
    const chosen = computed(() => (read(useLeft) ? read(left) : read(right)));
    const listener = fn();
    subscribe(chosen, listener);

    write(right, "R2");
    expect(listener).not.toHaveBeenCalled();

    write(left, "L2");
    expect(listener).toHaveBeenCalledTimes(1);
    expect(read(chosen)).toBe("L2");

    write(useLeft, false);
    expect(read(chosen)).toBe("R2");
    write(left, "L3");
    expect(listener).toHaveBeenCalledTimes(2);
  });

  it("chains through several layers", () => {
    const source = cell(1);
    const doubled = computed(() => read(source) * 2);
    const labelled = computed(() => `n=${read(doubled)}`);
    const listener = fn();
    subscribe(labelled, listener);
    write(source, 5);
    expect(read(labelled)).toBe("n=10");
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("reads a diamond consistently", () => {
    const source = cell(1);
    const left = computed(() => read(source) + 1);
    const right = computed(() => read(source) * 10);
    const joined = computed(() => `${read(left)}/${read(right)}`);
    expect(read(joined)).toBe("2/10");
    write(source, 2);
    expect(read(joined)).toBe("3/20");
  });

  it("refuses a write", () => {
    const doubled = computed(() => 1);
    expect(() => write(doubled, 2)).toThrow("read-only");
  });

  it("names a self-referential derive instead of overflowing the stack", () => {
    let loop = null;
    loop = computed(() => (loop == null ? 0 : read(loop)));
    expect(() => read(loop)).toThrow("depends on itself");
  });

  it("does not record what untracked read", () => {
    const tracked = cell(1);
    const ignored = cell(100);
    const total = computed(() => read(tracked) + untracked(() => read(ignored)));
    const listener = fn();
    subscribe(total, listener);
    write(ignored, 200);
    expect(listener).not.toHaveBeenCalled();
    write(tracked, 2);
    expect(read(total)).toBe(202);
  });
});

describe("batch", () => {
  it("wakes a subscriber once for a burst of writes", () => {
    const first = cell(0);
    const second = cell(0);
    const total = computed(() => read(first) + read(second));
    const listener = fn();
    subscribe(total, listener);
    batch(() => {
      write(first, 1);
      write(second, 2);
    });
    expect(listener).toHaveBeenCalledTimes(1);
    expect(read(total)).toBe(3);
  });

  it("still reads the new value inside the batch", () => {
    const count = cell(0);
    const doubled = computed(() => read(count) * 2);
    let seen = -1;
    batch(() => {
      write(count, 21);
      seen = read(doubled);
    });
    expect(seen).toBe(42);
  });

  it("returns what the body returned", () => {
    const count = cell(1);
    expect(
      batch(() => {
        write(count, 2);
        return read(count);
      }),
    ).toBe(2);
  });

  it("flushes with the outermost batch", () => {
    const count = cell(0);
    const listener = fn();
    subscribe(count, listener);
    batch(() => {
      batch(() => {
        write(count, 1);
      });
      expect(listener).not.toHaveBeenCalled();
      write(count, 2);
    });
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("flushes even when the body throws", () => {
    const count = cell(0);
    const listener = fn();
    subscribe(count, listener);
    expect(() =>
      batch(() => {
        write(count, 1);
        throw Error("boom");
      }),
    ).toThrow("boom");
    expect(listener).toHaveBeenCalledTimes(1);
    expect(read(count)).toBe(1);
  });
});

describe("resource", () => {
  it("reads null until the load settles, then the value", async () => {
    let settle = (value: string) => {};
    const loaded = resource<string>(() => new Promise((done) => (settle = done)));
    expect(read(loaded)).toBe(null);
    expect(status(loaded)).toBe("pending");
    settle("ready");
    await Promise.resolve();
    expect(read(loaded)).toBe("ready");
    expect(status(loaded)).toBe("success");
  });

  it("does not start loading until something asks", () => {
    const load = fn(() => Promise.resolve(1));
    const loaded = resource(() => load() as $FlowFixMe);
    expect(status(loaded)).toBe("idle");
    expect(load).not.toHaveBeenCalled();
    read(loaded);
    expect(load).toHaveBeenCalledTimes(1);
  });

  it("wakes subscribers when the load settles", async () => {
    const loaded = resource(() => Promise.resolve("value"));
    const listener = fn();
    subscribe(loaded, listener);
    await Promise.resolve();
    await Promise.resolve();
    expect(listener).toHaveBeenCalledTimes(1);
    expect(read(loaded)).toBe("value");
  });

  it("re-throws a rejected load on every read", async () => {
    const loaded = resource(() => Promise.reject(Error("offline")));
    read(loaded);
    await Promise.resolve();
    await Promise.resolve();
    expect(status(loaded)).toBe("failure");
    expect(() => read(loaded)).toThrow("offline");
    expect(() => read(loaded)).toThrow("offline");
  });

  it("reports a plain cell as already settled", () => {
    expect(status(cell(1))).toBe("success");
  });
});

describe("unsubscribing inside a batch", () => {
  it("does not call a listener that was torn down before the flush", () => {
    const value = cell(0);
    const calls = [];
    const stop = subscribe(value, () => calls.push("listener"));

    batch(() => {
      write(value, 1);
      // React unmounts a `useSyncExternalStore` subscriber by calling exactly
      // this, and an unmount can happen inside a batch.
      stop();
    });

    expect(calls).toEqual([]);
  });

  it("still calls the listeners that remain", () => {
    const value = cell(0);
    const calls = [];
    const stopFirst = subscribe(value, () => calls.push("first"));
    subscribe(value, () => calls.push("second"));

    batch(() => {
      write(value, 1);
      stopFirst();
    });

    expect(calls).toEqual(["second"]);
  });
});

// The properties below are what separates a reactive graph that is correct
// from one that merely produces the right value if you read it at the right
// moment. Every one of them is a count: a value assertion cannot tell a graph
// that computed once from a graph that computed three times and settled on the
// same answer, and the difference is the whole cost model.

describe("glitch freedom", () => {
  it("recomputes a diamond join once per write, and wakes its subscriber once", () => {
    const source = cell(1);
    const left = computed(() => read(source) + 1);
    const right = computed(() => read(source) * 10);
    const join = fn(() => `${read(left)}/${read(right)}`);
    const joined = computed(() => String(join()));
    const listener = fn();
    subscribe(joined, listener);
    expect(read(joined)).toBe("2/10");

    join.mockClear();
    listener.mockClear();
    write(source, 2);

    expect(read(joined)).toBe("3/20");
    // The eager implementation this replaced ran the join twice and woke the
    // listener twice: once against the new left and the *old* right, once
    // against both. The first of those values never existed.
    expect(join).toHaveBeenCalledTimes(1);
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("recomputes each side of the diamond once per write", () => {
    const source = cell(1);
    const deriveLeft = fn(() => read(source) + 1);
    const deriveRight = fn(() => read(source) * 10);
    const left = computed(() => Number(deriveLeft()));
    const right = computed(() => Number(deriveRight()));
    const joined = computed(() => `${read(left)}/${read(right)}`);
    subscribe(joined, () => {});

    deriveLeft.mockClear();
    deriveRight.mockClear();
    write(source, 2);

    expect(deriveLeft).toHaveBeenCalledTimes(1);
    expect(deriveRight).toHaveBeenCalledTimes(1);
  });

  it("never shows the join a half-updated pair", () => {
    const source = cell(1);
    const left = computed(() => read(source) + 1);
    const right = computed(() => read(source) * 10);
    const seen = [];
    const joined = computed(() => {
      const pair = [read(left), read(right)];
      seen.push(pair);
      return pair.join("/");
    });
    subscribe(joined, () => {});
    write(source, 2);
    write(source, 3);

    // Every pair the join was ever given is a function of one state of the
    // source: right is always ten times what left minus one is.
    expect(seen.every(([one, ten]) => (one - 1) * 10 === ten)).toBe(true);
    expect(seen).toHaveLength(3);
  });

  it("stops at a layer whose value did not change", () => {
    const count = cell(2);
    const isEven = computed(() => read(count) % 2 === 0);
    const label = fn(() => (read(isEven) ? "even" : "odd"));
    const shown = computed(() => String(label()));
    const listener = fn();
    subscribe(shown, listener);

    label.mockClear();
    write(count, 4);

    // `isEven` recomputed and produced the value it already had, so nothing
    // below it ran. The cutoff propagates; it is not only about the cell that
    // was written.
    expect(label).not.toHaveBeenCalled();
    expect(listener).not.toHaveBeenCalled();

    write(count, 5);
    expect(label).toHaveBeenCalledTimes(1);
    expect(listener).toHaveBeenCalledTimes(1);
    expect(read(shown)).toBe("odd");
  });

  it("wakes a subscriber once for a write that reaches it by two paths", () => {
    const source = cell(0);
    const left = computed(() => read(source) + 1);
    const right = computed(() => read(source) + 2);
    const listener = fn();
    subscribe(
      computed(() => read(left) + read(right)),
      listener,
    );
    write(source, 1);
    expect(listener).toHaveBeenCalledTimes(1);
  });
});

describe("dynamic dependencies", () => {
  it("stops recomputing for the branch it stopped reading", () => {
    const useLeft = cell(true);
    const left = cell("L");
    const right = cell("R");
    const derive = fn(() => (read(useLeft) ? read(left) : read(right)));
    const chosen = computed(() => String(derive()));
    subscribe(chosen, () => {});

    derive.mockClear();
    write(right, "R2");
    // It never read `right`, so writing it is not a change to anything.
    expect(derive).not.toHaveBeenCalled();

    write(useLeft, false);
    expect(derive).toHaveBeenCalledTimes(1);
    expect(read(chosen)).toBe("R2");

    derive.mockClear();
    write(left, "L2");
    // The dependency on `left` is gone, proven by writing it.
    expect(derive).not.toHaveBeenCalled();
    write(right, "R3");
    expect(derive).toHaveBeenCalledTimes(1);
    expect(read(chosen)).toBe("R3");
  });

  it("unmounts the dependency it dropped", () => {
    const events = [];
    const useLeft = cell(true);
    const left = cell("L", {
      onMount: () => {
        events.push("left on");
        return () => events.push("left off");
      },
    });
    const right = cell("R", {
      onMount: () => {
        events.push("right on");
        return () => events.push("right off");
      },
    });
    const chosen = computed(() => (read(useLeft) ? read(left) : read(right)));
    subscribe(chosen, () => {});
    expect(events).toEqual(["left on"]);

    write(useLeft, false);
    // The order matters: the new dependency is linked before the old one is
    // dropped, so a cell both branches shared would not be torn down and
    // restarted for nothing.
    expect(events).toEqual(["left on", "right on", "left off"]);
  });
});

describe("liveness", () => {
  it("stops recomputing once the last subscriber leaves", () => {
    const source = cell(0);
    const derive = fn(() => read(source));
    const mirror = computed(() => Number(derive()));
    const stop = subscribe(mirror, () => {});

    write(source, 1);
    expect(derive).toHaveBeenCalledTimes(2);

    stop();
    derive.mockClear();
    write(source, 2);
    write(source, 3);
    expect(derive).not.toHaveBeenCalled();

    // Still correct on demand, and still memoised: one recompute for two
    // writes it slept through.
    expect(read(mirror)).toBe(3);
    expect(derive).toHaveBeenCalledTimes(1);
    expect(read(mirror)).toBe(3);
    expect(derive).toHaveBeenCalledTimes(1);
  });

  it("runs onMount for the first subscriber and its teardown for the last", () => {
    const events = [];
    const source = cell(0, {
      onMount: () => {
        events.push("start");
        return () => {
          events.push("stop");
        };
      },
    });

    const first = subscribe(source, () => {});
    const second = subscribe(source, () => {});
    expect(events).toEqual(["start"]);

    first();
    expect(events).toEqual(["start"]);
    second();
    expect(events).toEqual(["start", "stop"]);

    subscribe(source, () => {});
    expect(events).toEqual(["start", "stop", "start"]);
  });

  it("does not mount a cell that is only read", () => {
    const events = [];
    const source = cell(0, {
      onMount: () => {
        events.push("start");
      },
    });
    expect(read(source)).toBe(0);
    write(source, 1);
    expect(events).toEqual([]);
  });

  it("mounts what a subscribed derive reads, and unmounts it again", () => {
    const events = [];
    const source = cell(0, {
      onMount: () => {
        events.push("start");
        return () => events.push("stop");
      },
    });
    const doubled = computed(() => read(source) * 2);
    const stop = subscribe(doubled, () => {});
    expect(events).toEqual(["start"]);
    stop();
    expect(events).toEqual(["start", "stop"]);
  });

  it("delivers a value the mount wrote, even to a read already in flight", () => {
    // The mount runs while the derive that caused it is being linked up, which
    // means it writes after that derive read the old value. The write has to
    // survive as a mark rather than be stamped over by the evaluation that is
    // finishing.
    const feed = cell(0, {
      onMount: (self) => {
        write(self, 21);
      },
    });
    const doubled = computed(() => read(feed) * 2);
    const seen = [];
    subscribe(doubled, () => seen.push(read(doubled)));
    expect(read(doubled)).toBe(42);
    expect(seen).toEqual([42]);
  });
});

describe("effect", () => {
  it("runs now, and again when what it read changes", () => {
    const source = cell(1);
    const seen = [];
    const stop = effect(() => {
      seen.push(read(source));
    });
    expect(seen).toEqual([1]);
    write(source, 2);
    expect(seen).toEqual([1, 2]);
    stop();
    write(source, 3);
    expect(seen).toEqual([1, 2]);
  });

  it("runs its teardown before each re-run and once on stop", () => {
    const source = cell(1);
    const events = [];
    const stop = effect(() => {
      const value = read(source);
      events.push(`run ${value}`);
      return () => events.push(`clean ${value}`);
    });
    write(source, 2);
    stop();
    expect(events).toEqual(["run 1", "clean 1", "run 2", "clean 2"]);
  });

  it("runs once for a batch of writes", () => {
    const first = cell(0);
    const second = cell(0);
    const body = fn(() => {
      read(first);
      read(second);
    });
    effect(() => body());
    body.mockClear();
    batch(() => {
      write(first, 1);
      write(second, 2);
      write(first, 3);
    });
    expect(body).toHaveBeenCalledTimes(1);
  });

  it("does not run for a write that changes nothing", () => {
    const source = cell(1);
    const body = fn(() => read(source));
    effect(() => {
      body();
    });
    body.mockClear();
    write(source, 1);
    expect(body).not.toHaveBeenCalled();
  });
});

describe("peek", () => {
  it("reads without creating a dependency", () => {
    const tracked = cell(1);
    const ignored = cell(100);
    const derive = fn(() => read(tracked) + peek(ignored));
    const total = computed(() => Number(derive()));
    subscribe(total, () => {});

    derive.mockClear();
    write(ignored, 200);
    expect(derive).not.toHaveBeenCalled();

    write(tracked, 2);
    expect(read(total)).toBe(202);
  });
});

describe("equals", () => {
  it("uses the cell's own comparison to decide what changed", () => {
    const sameLength = (previous, next) => previous.length === next.length;
    const rows = cell(["a", "b"], { equals: sameLength });
    const listener = fn();
    subscribe(rows, listener);
    write(rows, ["c", "d"]);
    expect(listener).not.toHaveBeenCalled();
    expect(read(rows)).toEqual(["a", "b"]);
    write(rows, ["c"]);
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("stops a derive that rebuilds an array from waking anyone", () => {
    const numbers = cell([1, 2, 3, 4]);
    const evens = computed(() => read(numbers).filter((value) => value % 2 === 0), {
      equals: (previous, next) =>
        previous.length === next.length && previous.every((value, index) => value === next[index]),
    });
    const listener = fn();
    subscribe(evens, listener);
    write(numbers, [1, 2, 3, 4, 5]);
    expect(listener).not.toHaveBeenCalled();
    write(numbers, [1, 2, 3, 4, 6]);
    expect(listener).toHaveBeenCalledTimes(1);
  });
});

describe("an asynchronous cell that reloads", () => {
  /** A load whose promises are settled by the test, one per key. */
  function controlled() {
    const settle = new Map();
    return {
      settle,
      load: (key) =>
        new Promise((resolve) => {
          settle.set(key, resolve);
        }),
    };
  }

  it("reloads when what the load read changes", async () => {
    const id = cell("a");
    const { settle, load } = controlled();
    const loaded = resource(() => load(read(id)));
    subscribe(loaded, () => {});

    settle.get("a")("value a");
    await Promise.resolve();
    await Promise.resolve();
    expect(read(loaded)).toBe("value a");

    write(id, "b");
    expect(status(loaded)).toBe("pending");
    settle.get("b")("value b");
    await Promise.resolve();
    await Promise.resolve();
    expect(read(loaded)).toBe("value b");
  });

  it("does not let a slow load overtaken by a fast one deliver its value", async () => {
    const id = cell("slow");
    const { settle, load } = controlled();
    const loaded = resource(() => load(read(id)));
    const listener = fn();
    subscribe(loaded, listener);

    // The second load starts while the first is still in flight.
    write(id, "fast");
    settle.get("fast")("fast value");
    await Promise.resolve();
    await Promise.resolve();
    expect(read(loaded)).toBe("fast value");

    // The one it superseded settles last, which is exactly the race this
    // exists to lose safely.
    settle.get("slow")("slow value");
    await Promise.resolve();
    await Promise.resolve();
    expect(read(loaded)).toBe("fast value");
    expect(status(loaded)).toBe("success");
  });

  it("does not reload for a write to something the load never read", async () => {
    const id = cell("a");
    const unrelated = cell(0);
    const load = fn((key) => Promise.resolve(`value ${key}`));
    const loaded = resource(() => load(read(id)));
    subscribe(loaded, () => {});
    await Promise.resolve();
    await Promise.resolve();

    load.mockClear();
    write(unrelated, 1);
    expect(load).not.toHaveBeenCalled();
    write(id, "b");
    expect(load).toHaveBeenCalledTimes(1);
  });

  it("lets a write supersede the load in flight", async () => {
    const { settle, load } = controlled();
    const loaded = resource(() => load("only"));
    subscribe(loaded, () => {});
    expect(status(loaded)).toBe("pending");

    // An optimistic update, or a value that arrived by another route.
    write(loaded, "written");
    expect(read(loaded)).toBe("written");
    expect(status(loaded)).toBe("success");

    settle.get("only")("loaded");
    await Promise.resolve();
    await Promise.resolve();
    expect(read(loaded)).toBe("written");
  });

  it("loads again on refresh, without anything it reads changing", async () => {
    let served = 0;
    const load = fn(() => {
      served += 1;
      return Promise.resolve(served);
    });
    const loaded = resource(() => load());
    subscribe(loaded, () => {});
    await Promise.resolve();
    await Promise.resolve();
    expect(read(loaded)).toBe(1);

    refresh(loaded);
    await Promise.resolve();
    await Promise.resolve();
    expect(read(loaded)).toBe(2);
    expect(load).toHaveBeenCalledTimes(2);
  });
});

describe("a dependency a nested derive also read", () => {
  it("keeps its edge to the outer derive", () => {
    // The inner evaluation stamps the shared cell as *its* dependency. If the
    // outer relink decides what to unlink from that stamp, it drops an edge it
    // is still using — and the second write after that reaches nobody.
    const source = cell(1);
    const other = computed(() => read(source) * 10);
    const derive = fn(() => read(source) + untracked(() => read(other)));
    const total = computed(() => Number(derive()));
    const listener = fn();
    subscribe(total, listener);

    write(source, 2);
    expect(listener).toHaveBeenCalledTimes(1);
    expect(read(total)).toBe(22);

    write(source, 3);
    expect(listener).toHaveBeenCalledTimes(2);
    expect(read(total)).toBe(33);
  });
});
