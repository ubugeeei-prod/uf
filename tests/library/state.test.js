// @flow
//
// `@uniflowed/state`: the atom surface, stores, and the React binding rendered
// for real — on `react-dom/server` for markup, and in a DOM where the question
// is how many times a component rendered.
//
// The counts are the point. A test that only checks values cannot tell a store
// that recomputed once from one that recomputed three times and settled on the
// same answer, and the difference is the entire reason to use this rather than
// lifting state into a context.

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

import { describe, expect, fn, it } from "@uniflowed/test";
import * as React from "@uniflowed/react";
import { renderToStaticMarkup } from "react-dom/server";
import { act, render, waitFor } from "@uniflowed/react-testing";
import {
  derived as derivedCell,
  read as readCell,
  state as stateCell,
  write as writeCell,
} from "@uniflowed/cell";
import {
  Provider,
  RESET,
  action,
  asyncAtom,
  atom,
  atomFamily,
  atomWithAsyncStorage,
  atomWithDefault,
  atomWithReducer,
  atomWithReset,
  atomWithStorage,
  batch,
  createJSONStorage,
  createStore,
  freezeAtom,
  getDefaultStore,
  read,
  refresh,
  selectAtom,
  selector,
  subscribe,
  unwrap,
  useAtom,
  useAtomCallback,
  useAtomValue,
  useCell,
  useResetAtom,
  useSetAtom,
  useStore,
  writableSelector,
  write,
} from "@uniflowed/state";

/**
 * A `StringStorage` backed by a map, so the tests need no browser.
 *
 * It counts its reads, because "when is storage read" is the question the
 * hydration bug was an answer to and a value assertion cannot see it: a page
 * that reads storage at import and a page that reads it on mount agree about
 * every value and disagree about the only thing that matters.
 */
function memoryStorage(seed?: { [string]: string }): {
  getItem: (key: string) => null | string,
  setItem: (key: string, value: string) => void,
  removeItem: (key: string) => void,
  entries: Map<string, string>,
  reads: () => number,
} {
  const entries: Map<string, string> = new Map(Object.entries(seed ?? {}));
  let reads = 0;
  return {
    entries,
    reads: () => reads,
    getItem: (key) => {
      reads += 1;
      return entries.get(key) ?? null;
    },
    setItem: (key, value) => {
      entries.set(key, value);
    },
    removeItem: (key) => {
      entries.delete(key);
    },
  };
}

/**
 * A load whose promises the test settles by hand, one per key.
 *
 * `settle` throws rather than returning `undefined` when nothing is waiting
 * for the key: a test that settles a load which was never started is not
 * testing what its name says, and the failure it would otherwise produce —
 * a value that never arrives — points at the library rather than at itself.
 */
function controlled(): {
  settle: (key: string, value: string) => void,
  load: (key: string) => Promise<string>,
} {
  const waiting: Map<string, (value: string) => void> = new Map();
  return {
    settle: (key, value) => {
      const resolve = waiting.get(key);
      if (resolve === undefined) {
        throw Error(`no load is waiting for ${key}`);
      }
      resolve(value);
    },
    load: (key) =>
      new Promise((resolve) => {
        waiting.set(key, resolve);
      }),
  };
}

/**
 * An asynchronous storage whose reads the test settles by hand.
 *
 * The adapter is a member rather than the object itself, because
 * `AsyncStorageAdapter<T>` is exact and the controls below are not part of it.
 * `settle` and `fail` throw when nothing is waiting, for the reason
 * [`controlled`] gives: a test that settles a read which never started is not
 * testing what its name says.
 *
 * Writes take effect at once and answer a resolved promise. The two tests that
 * need a write which fails build an adapter of their own, because "the write
 * rejected" is the only thing they are about.
 */
function controlledStorage(seed?: { [string]: string }): {
  adapter: {
    getItem: (
      key: string,
      initial: string,
      context: { readonly signal: AbortSignal, ... },
    ) => Promise<string>,
    setItem: (key: string, value: string) => Promise<void>,
    removeItem: (key: string) => Promise<void>,
  },
  entries: Map<string, string>,
  reads: () => number,
  signals: Array<AbortSignal>,
  settle: () => void,
  fail: (error: mixed) => void,
} {
  const entries: Map<string, string> = new Map(Object.entries(seed ?? {}));
  const signals: Array<AbortSignal> = [];
  let waiting: Array<{
    key: string,
    initial: string,
    deliver: (value: string) => void,
    reject: (error: mixed) => void,
  }> = [];
  let reads = 0;

  const drain = () => {
    if (waiting.length === 0) {
      throw Error("no read is waiting for this storage");
    }
    const pending = waiting;
    waiting = [];
    return pending;
  };

  return {
    entries,
    signals,
    reads: () => reads,
    settle: () => {
      for (const pending of drain()) {
        pending.deliver(entries.get(pending.key) ?? pending.initial);
      }
    },
    fail: (error) => {
      for (const pending of drain()) {
        pending.reject(error);
      }
    },
    adapter: {
      getItem: (key, initial, context) => {
        reads += 1;
        signals.push(context.signal);
        return new Promise((resolve, reject) => {
          waiting.push({ key, initial, deliver: resolve, reject });
        });
      },
      setItem: (key, value) => {
        entries.set(key, value);
        return Promise.resolve();
      },
      removeItem: (key) => {
        entries.delete(key);
        return Promise.resolve();
      },
    },
  };
}

describe("atom", () => {
  it("is readable and writable with no provider", () => {
    const count = atom(1);
    expect(read(count)).toBe(1);
    write(count, 2);
    expect(read(count)).toBe(2);
  });

  it("takes a reducer, the way useState does", () => {
    const count = atom(10);
    write(count, (current) => current + 5);
    expect(read(count)).toBe(15);
  });

  it("wakes a subscriber once per change", () => {
    const count = atom(0);
    const listener = fn();
    subscribe(count, listener);
    write(count, 1);
    write(count, 1);
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("allocates nothing until a store is asked for it", () => {
    // Declaring an atom is declaring a shape. The proof is that two stores
    // asked for the same declaration disagree about its value.
    const count = atom(0);
    const first = createStore();
    const second = createStore();

    write(count, 1, first);
    expect(read(count, first)).toBe(1);
    expect(read(count, second)).toBe(0);
    expect(read(count)).toBe(0);
  });

  it("keeps a subscription in one store out of another", () => {
    const count = atom(0);
    const store = createStore();
    const listener = fn();
    subscribe(count, listener, store);
    write(count, 1);
    expect(listener).not.toHaveBeenCalled();
    write(count, 1, store);
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("hands back the same default store every time", () => {
    expect(getDefaultStore()).toBe(getDefaultStore());
  });
});

describe("selector", () => {
  it("derives from atoms and re-derives when they change", () => {
    const first = atom(2);
    const second = atom(3);
    const product = selector((get) => get(first) * get(second));
    expect(read(product)).toBe(6);
    write(first, 4);
    expect(read(product)).toBe(12);
  });

  it("does not wake readers when the derived value is unchanged", () => {
    const rows = atom<$ReadOnlyArray<number>>([1, 2, 3]);
    const count = selector((get) => get(rows).length);
    const listener = fn();
    subscribe(count, listener);
    write(rows, [4, 5, 6]);
    expect(listener).not.toHaveBeenCalled();
    write(rows, [1]);
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("recomputes a diamond once per write, and wakes its subscriber once", () => {
    const source = atom(1);
    const left = selector((get) => get(source) + 1);
    const right = selector((get) => get(source) * 10);
    const join = fn((get) => `${get(left)}/${get(right)}`);
    const joined = selector((get) => String(join(get)));
    const listener = fn();
    subscribe(joined, listener);
    expect(read(joined)).toBe("2/10");

    join.mockClear();
    listener.mockClear();
    write(source, 2);

    expect(read(joined)).toBe("3/20");
    expect(join).toHaveBeenCalledTimes(1);
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("depends on the branch it read, and stops depending on the one it did not", () => {
    const showAll = atom(true);
    const all = atom("all");
    const some = atom("some");
    const derive = fn((get) => (get(showAll) ? get(all) : get(some)));
    const shown = selector((get) => String(derive(get)));
    subscribe(shown, () => {});

    derive.mockClear();
    write(some, "some 2");
    expect(derive).not.toHaveBeenCalled();

    write(showAll, false);
    expect(read(shown)).toBe("some 2");
    derive.mockClear();

    write(all, "all 2");
    expect(derive).not.toHaveBeenCalled();
    write(some, "some 3");
    expect(derive).toHaveBeenCalledTimes(1);
  });

  it("stops recomputing once nothing is subscribed", () => {
    const source = atom(0);
    const derive = fn((get) => get(source));
    const mirror = selector((get) => Number(derive(get)));
    const stop = subscribe(mirror, () => {});
    write(source, 1);
    expect(derive).toHaveBeenCalledTimes(2);

    stop();
    derive.mockClear();
    write(source, 2);
    write(source, 3);
    expect(derive).not.toHaveBeenCalled();
    expect(read(mirror)).toBe(3);
    expect(derive).toHaveBeenCalledTimes(1);
  });

  it("refuses a write", () => {
    const source = atom(1);
    const doubled = selector((get) => get(source) * 2);
    // $FlowExpectedError[incompatible-call] a selector is not writable.
    expect(() => write(doubled, 4)).toThrow("read-only");
  });
});

describe("writableSelector", () => {
  it("reads derived and writes back to what it derived from", () => {
    const first = atom("Ada");
    const last = atom("Lovelace");
    const full = writableSelector<string, string>(
      (get) => `${get(first)} ${get(last)}`,
      (get, set, next) => {
        const [given, family] = next.split(" ");
        set(first, given);
        set(last, family);
      },
    );

    expect(read(full)).toBe("Ada Lovelace");
    write(full, "Grace Hopper");
    expect(read(first)).toBe("Grace");
    expect(read(last)).toBe("Hopper");
    expect(read(full)).toBe("Grace Hopper");
  });

  it("wakes each subscriber once for a write that touches three atoms", () => {
    const page = atom(3);
    const query = atom("");
    const sort = atom("date");
    const search = action<string>((get, set, next) => {
      set(query, next);
      set(page, 1);
      set(sort, "relevance");
    });

    const listeners = { page: fn(), query: fn(), sort: fn() };
    subscribe(page, listeners.page);
    subscribe(query, listeners.query);
    subscribe(sort, listeners.sort);

    write(search, "flow");

    expect(listeners.page).toHaveBeenCalledTimes(1);
    expect(listeners.query).toHaveBeenCalledTimes(1);
    expect(listeners.sort).toHaveBeenCalledTimes(1);
    expect(read(page)).toBe(1);
  });

  it("does not depend on what its write read", () => {
    const audit = atom(0);
    const source = atom(1);
    const derive = fn((get) => get(source));
    const mirror = writableSelector<number, number>(
      (get) => Number(derive(get)),
      (get, set, next) => {
        // A write is allowed to look at anything. Looking is not depending.
        set(source, next + get(audit));
      },
    );
    subscribe(mirror, () => {});

    derive.mockClear();
    write(audit, 100);
    expect(derive).not.toHaveBeenCalled();
  });
});

describe("action", () => {
  it("runs its write with get and set, and reads as null", () => {
    const count = atom(0);
    const bump = action<number>((get, set, by) => {
      set(count, get(count) + by);
    });
    expect(read(bump)).toBe(null);
    write(bump, 5);
    write(bump, 3);
    expect(read(count)).toBe(8);
  });
});

describe("asyncAtom", () => {
  it("is loading until the promise settles, then holds the data", async () => {
    const { settle, load } = controlled();
    const user = asyncAtom(() => load("only"));
    subscribe(user, () => {});
    expect(read(user)).toEqual({ state: "loading" });

    settle("only", "Ada");
    await Promise.resolve();
    await Promise.resolve();
    expect(read(user)).toEqual({ state: "hasData", data: "Ada" });
  });

  it("holds the failure rather than throwing it at a render", async () => {
    const user = asyncAtom(() => Promise.reject(Error("offline")));
    subscribe(user, () => {});
    await Promise.resolve();
    await Promise.resolve();

    const settled = read(user);
    expect(settled.state).toBe("hasError");
    expect(String(settled.error)).toContain("offline");
  });

  it("reloads when what it read changes", async () => {
    const id = atom("a");
    const { settle, load } = controlled();
    const user = asyncAtom((get) => load(get(id)));
    subscribe(user, () => {});

    settle("a", "value a");
    await Promise.resolve();
    await Promise.resolve();
    expect(read(user)).toEqual({ state: "hasData", data: "value a" });

    write(id, "b");
    expect(read(user)).toEqual({ state: "loading" });
    settle("b", "value b");
    await Promise.resolve();
    await Promise.resolve();
    expect(read(user)).toEqual({ state: "hasData", data: "value b" });
  });

  it("does not let a slow load overtaken by a fast one deliver its value", async () => {
    const id = atom("slow");
    const { settle, load } = controlled();
    const user = asyncAtom((get) => load(get(id)));
    const listener = fn();
    subscribe(user, listener);

    // The second load starts while the first is still in flight.
    write(id, "fast");
    settle("fast", "fast value");
    await Promise.resolve();
    await Promise.resolve();
    expect(read(user)).toEqual({ state: "hasData", data: "fast value" });

    // The load it superseded settles last, which is the race this loses safely.
    settle("slow", "slow value");
    await Promise.resolve();
    await Promise.resolve();
    expect(read(user)).toEqual({ state: "hasData", data: "fast value" });
  });

  it("hands the load a signal and aborts the one it abandons", async () => {
    const id = atom("slow");
    const signals = [];
    const { settle, load } = controlled();
    const user = asyncAtom((get, context) => {
      signals.push(context.signal);
      return load(get(id));
    });
    subscribe(user, () => {});
    expect(signals[0].aborted).toBe(false);

    write(id, "fast");
    expect(signals).toHaveLength(2);
    expect(signals[0].aborted).toBe(true);
    expect(signals[1].aborted).toBe(false);

    settle("fast", "fast value");
    await Promise.resolve();
    await Promise.resolve();
    expect(read(user)).toEqual({ state: "hasData", data: "fast value" });
  });

  it("does not let an abandoned load's rejection become the atom's error", async () => {
    // The load a real `fetch` rejects when its signal fires, which is the
    // shape that makes this worth a test of its own: the rejection is folded
    // into a `Loadable` before the store ever sees it, so by then it is an
    // ordinary `hasError` value and nothing about it says "abandoned".
    const id = atom("first");
    const user = asyncAtom(
      (get, context) =>
        new Promise((resolve, reject) => {
          const key = get(id);
          context.signal.addEventListener("abort", () => {
            reject(Error("The operation was aborted"));
          });
          if (key === "second") {
            resolve("second user");
          }
        }),
    );
    subscribe(user, () => {});
    write(id, "second");
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();

    expect(read(user)).toEqual({ state: "hasData", data: "second user" });
  });

  it("aborts the load in flight when the last subscriber leaves", async () => {
    const signals = [];
    const { load } = controlled();
    const user = asyncAtom((get, context) => {
      signals.push(context.signal);
      return load("only");
    });
    const stop = subscribe(user, () => {});
    expect(signals[0].aborted).toBe(false);

    stop();
    expect(signals[0].aborted).toBe(true);
    expect(String(signals[0].reason)).toContain("unwatched");
  });

  it("starts a load the last unsubscribe abandoned for the next subscriber", async () => {
    let served = 0;
    const { settle, load } = controlled();
    const user = asyncAtom(() => {
      served += 1;
      return load(`load ${served}`);
    });

    const first = subscribe(user, () => {});
    expect(served).toBe(1);
    first();

    subscribe(user, () => {});
    expect(served).toBe(2);
    expect(read(user)).toEqual({ state: "loading" });
    settle("load 2", "second value");
    await Promise.resolve();
    await Promise.resolve();
    expect(read(user)).toEqual({ state: "hasData", data: "second value" });
  });

  it("keeps the load in one store out of another", async () => {
    const signals = [];
    const { load } = controlled();
    const user = asyncAtom((get, context) => {
      signals.push(context.signal);
      return load("only");
    });
    const a = createStore();
    const b = createStore();
    const stop = subscribe(user, () => {}, a);
    subscribe(user, () => {}, b);
    expect(signals).toHaveLength(2);

    stop();
    expect(signals[0].aborted).toBe(true);
    expect(signals[1].aborted).toBe(false);
  });

  it("unwraps to a fallback until the data arrives", async () => {
    const { settle, load } = controlled();
    const user = asyncAtom(() => load("only"));
    const name = unwrap(user, "anonymous");
    subscribe(name, () => {});
    expect(read(name)).toBe("anonymous");

    settle("only", "Ada");
    await Promise.resolve();
    await Promise.resolve();
    expect(read(name)).toBe("Ada");
  });
});

describe("refresh", () => {
  it("loads again with the dependencies it already has", async () => {
    // Writing the dependency the value it already holds is dropped by the
    // equality cutoff, correctly — so "ask again" has no other expression.
    const id = atom("a");
    let served = 0;
    const { settle, load } = controlled();
    const user = asyncAtom((get) => {
      served += 1;
      return load(`${get(id)}${served}`);
    });
    subscribe(user, () => {});
    settle("a1", "first answer");
    await Promise.resolve();
    await Promise.resolve();
    expect(read(user)).toEqual({ state: "hasData", data: "first answer" });

    write(id, "a");
    expect(served).toBe(1);

    refresh(user);
    expect(served).toBe(2);
    settle("a2", "second answer");
    await Promise.resolve();
    await Promise.resolve();
    expect(read(user)).toEqual({ state: "hasData", data: "second answer" });
  });

  it("passes through loading on the way, so a spinner has something to read", async () => {
    const { settle, load } = controlled();
    let served = 0;
    const user = asyncAtom(() => {
      served += 1;
      return load(`load ${served}`);
    });
    const listener = fn();
    subscribe(user, listener);
    settle("load 1", "first");
    await Promise.resolve();
    await Promise.resolve();
    expect(read(user)).toEqual({ state: "hasData", data: "first" });
    listener.mockClear();

    refresh(user);
    expect(read(user)).toEqual({ state: "loading" });
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("does not let the load it abandoned disturb the value that replaced it", async () => {
    const { settle, load } = controlled();
    let served = 0;
    const user = asyncAtom(() => {
      served += 1;
      return load(`load ${served}`);
    });
    subscribe(user, () => {});
    refresh(user);
    expect(served).toBe(2);

    settle("load 2", "second");
    await Promise.resolve();
    await Promise.resolve();
    expect(read(user)).toEqual({ state: "hasData", data: "second" });

    // The abandoned first load settles last, after the newer one already
    // answered. It has nothing to say.
    settle("load 1", "first");
    await Promise.resolve();
    await Promise.resolve();
    expect(read(user)).toEqual({ state: "hasData", data: "second" });
  });

  it("reloads in the store it was given and not in a sibling", async () => {
    let served = 0;
    const { load } = controlled();
    const user = asyncAtom(() => {
      served += 1;
      return load(`load ${served}`);
    });
    const a = createStore();
    const b = createStore();
    subscribe(user, () => {}, a);
    subscribe(user, () => {}, b);
    expect(served).toBe(2);

    refresh(user, a);
    expect(served).toBe(3);
    expect(read(user, b)).toEqual({ state: "loading" });
  });

  it("names the atom when it is asked to reload one that has no load", () => {
    // Flow rejects this at the call site: `refresh` takes an `AsyncAtom`, and
    // only `asyncAtom` makes one. The type is gone at run time, so the guard
    // stays — and says which atom rather than doing nothing.
    const settled = selector(() => ({ state: "hasData", data: 1 }));
    expect(() => refresh(settled as $FlowFixMe)).toThrow("is not an asynchronous atom");
  });
});

describe("atomWithDefault", () => {
  it("computes its value until one is written", () => {
    const base = atom(2);
    const doubled = atomWithDefault((get) => get(base) * 2);
    expect(read(doubled)).toBe(4);
    write(base, 5);
    expect(read(doubled)).toBe(10);

    write(doubled, 100);
    expect(read(doubled)).toBe(100);
    write(base, 7);
    expect(read(doubled)).toBe(100);
  });

  it("stops depending on the default once a value is written", () => {
    const base = atom(1);
    const compute = fn((get) => get(base) * 2);
    const doubled = atomWithDefault((get) => Number(compute(get)));
    subscribe(doubled, () => {});

    compute.mockClear();
    write(base, 2);
    expect(compute).toHaveBeenCalledTimes(1);

    write(doubled, 50);
    compute.mockClear();
    write(base, 3);
    // The dependency on the default is gone, proven by writing what the
    // default read.
    expect(compute).not.toHaveBeenCalled();
  });

  it("goes back to the default on RESET, and depends on it again", () => {
    const base = atom(1);
    const doubled = atomWithDefault((get) => get(base) * 2);
    write(doubled, 50);
    expect(read(doubled)).toBe(50);

    write(doubled, RESET);
    expect(read(doubled)).toBe(2);
    write(base, 4);
    expect(read(doubled)).toBe(8);
  });

  it("takes a reducer over the value it currently shows", () => {
    const base = atom(10);
    const value = atomWithDefault((get) => get(base));
    write(value, (current) => current + 1);
    expect(read(value)).toBe(11);
  });
});

describe("atomWithReset", () => {
  it("is a piece of state that also answers to RESET", () => {
    const theme = atomWithReset("light");
    expect(read(theme)).toBe("light");
    write(theme, "dark");
    expect(read(theme)).toBe("dark");
    write(theme, RESET);
    expect(read(theme)).toBe("light");
  });

  it("takes a reducer over its current value", () => {
    const count = atomWithReset(1);
    write(count, (current) => current + 1);
    expect(read(count)).toBe(2);
  });

  it("resets the store it was told to and no other", () => {
    const theme = atomWithReset("light");
    const a = createStore();
    const b = createStore();
    write(theme, "dark", a);
    write(theme, "solar", b);

    write(theme, RESET, a);
    expect(read(theme, a)).toBe("light");
    expect(read(theme, b)).toBe("solar");
  });
});

describe("atomWithReducer", () => {
  it("applies the reducer and holds what it returned", () => {
    const count = atomWithReducer<number, "increment" | "clear">(0, (current, action) =>
      action === "increment" ? current + 1 : 0,
    );
    write(count, "increment");
    write(count, "increment");
    expect(read(count)).toBe(2);
    write(count, "clear");
    expect(read(count)).toBe(0);
  });

  it("wakes its subscriber once per action", () => {
    const count = atomWithReducer<number, number>(0, (current, by) => current + by);
    const listener = fn();
    subscribe(count, listener);
    write(count, 1);
    write(count, 2);
    expect(listener).toHaveBeenCalledTimes(2);
    expect(read(count)).toBe(3);
  });

  it("does not wake a subscriber when the reducer returns what it had", () => {
    // The reducer runs through the ordinary write path, so the equality
    // cutoff applies to it like anything else.
    const clamped = atomWithReducer<number, number>(0, (current, by) => Math.min(1, current + by));
    const listener = fn();
    subscribe(clamped, listener);
    write(clamped, 5);
    write(clamped, 5);
    expect(listener).toHaveBeenCalledTimes(1);
    expect(read(clamped)).toBe(1);
  });

  it("keeps its value out of another store", () => {
    const count = atomWithReducer<number, number>(0, (current, by) => current + by);
    const store = createStore();
    write(count, 3);
    expect(read(count)).toBe(3);
    expect(read(count, store)).toBe(0);
  });
});

describe("selectAtom", () => {
  it("wakes a reader for the part it selected and not for the rest", () => {
    const user = atom({ name: "Ada", avatar: "a.png" });
    const name = selectAtom(user, (current) => current.name);
    const listener = fn();
    subscribe(name, listener);

    write(user, { name: "Ada", avatar: "b.png" });
    expect(listener).not.toHaveBeenCalled();
    expect(read(name)).toBe("Ada");

    write(user, { name: "Grace", avatar: "b.png" });
    expect(listener).toHaveBeenCalledTimes(1);
    expect(read(name)).toBe("Grace");
  });

  it("takes an equals, for a selection that builds a fresh value each time", () => {
    // What Jotai's `prevSlice` parameter was for, said directly. Without this
    // the array is a new object on every recompute and every reader wakes.
    const rows = atom([{ id: 1, label: "one" }]);
    const ids = selectAtom(
      rows,
      (all) => all.map((row) => row.id),
      (previous, next) =>
        previous.length === next.length && previous.every((id, at) => id === next[at]),
    );
    const listener = fn();
    subscribe(ids, listener);

    write(rows, [{ id: 1, label: "renamed" }]);
    expect(listener).not.toHaveBeenCalled();

    write(rows, [
      { id: 1, label: "renamed" },
      { id: 2, label: "two" },
    ]);
    expect(listener).toHaveBeenCalledTimes(1);
    expect(read(ids)).toEqual([1, 2]);
  });

  it("runs the selection once per change to the source", () => {
    let selections = 0;
    const user = atom({ name: "Ada" });
    const name = selectAtom(user, (current) => {
      selections += 1;
      return current.name;
    });
    subscribe(name, () => {});
    subscribe(name, () => {});
    expect(selections).toBe(1);

    write(user, { name: "Grace" });
    expect(read(name)).toBe("Grace");
    expect(selections).toBe(2);
  });
});

describe("freezeAtom", () => {
  it("freezes the value deeply, so a mutation in place throws", () => {
    const settings = atom({ theme: { name: "dark" }, tags: ["a"] });
    const guarded = freezeAtom(settings);

    const value = read(guarded);
    expect(Object.isFrozen(value)).toBe(true);
    expect(Object.isFrozen(value.theme)).toBe(true);
    expect(Object.isFrozen(value.tags)).toBe(true);
    // A module is strict, so this is what the mutation that would otherwise
    // have gone unnoticed does now. The point is that it happens here rather
    // than in whichever component failed to re-render.
    expect(() => {
      value.tags.push("b");
    }).toThrow();
  });

  it("freezes the atom it derives from, because there is one object", () => {
    const settings = atom({ tags: ["a"] });
    read(freezeAtom(settings));
    // Worth a test rather than a footnote: wrapping an atom other code writes
    // to freezes that atom's value too, and there is no copy to hide it.
    expect(Object.isFrozen(read(settings))).toBe(true);
  });

  it("terminates on a structure that refers to itself", () => {
    const node: { name: string, child: mixed } = { name: "root", child: null };
    node.child = node;
    read(freezeAtom(atom(node)));
    expect(Object.isFrozen(node)).toBe(true);
  });

  it("cannot guard a Map, because Object.freeze is about properties", () => {
    const entries: Map<string, number> = new Map([["a", 1]]);
    read(freezeAtom(atom(entries)));
    // Named in the doc comment rather than left to be discovered, and pinned
    // here so the doc comment stays true: a frozen `Map` still accepts `set`.
    // The limit is `Object.freeze`'s, and Jotai's `freezeAtom` has it too.
    entries.set("b", 2);
    expect(entries.size).toBe(2);
  });

  it("freezes each new value as it arrives", () => {
    const settings = atom({ tags: ["a"] });
    const guarded = freezeAtom(settings);
    subscribe(guarded, () => {});
    expect(Object.isFrozen(read(guarded))).toBe(true);

    write(settings, { tags: ["b"] });
    const next = read(guarded);
    expect(next.tags).toEqual(["b"]);
    expect(Object.isFrozen(next)).toBe(true);
    expect(Object.isFrozen(next.tags)).toBe(true);
  });
});

describe("atomWithStorage", () => {
  it("behaves the same with no storage, RESET included", () => {
    const theme = atomWithStorage("theme", "light");
    expect(read(theme)).toBe("light");
    write(theme, "dark");
    expect(read(theme)).toBe("dark");
    write(theme, RESET);
    expect(read(theme)).toBe("light");
  });

  it("reads nothing when it is declared", () => {
    // The bug this replaced: `restore(...)` was an argument to the atom, so
    // the read happened while the module was being evaluated — before any
    // store existed, before React ran, and before the server's markup could
    // be disagreed with.
    const storage = memoryStorage({ theme: '"dark"' });
    atomWithStorage(
      "theme",
      "light",
      createJSONStorage(() => storage),
    );
    expect(storage.reads()).toBe(0);
  });

  it("reads nothing for a store that only reads it", () => {
    const storage = memoryStorage({ theme: '"dark"' });
    const theme = atomWithStorage(
      "theme",
      "light",
      createJSONStorage(() => storage),
    );
    expect(read(theme)).toBe("light");
    expect(storage.reads()).toBe(0);
  });

  it("adopts the stored value when a store mounts it", () => {
    const storage = memoryStorage({ theme: '"dark"' });
    const theme = atomWithStorage(
      "theme",
      "light",
      createJSONStorage(() => storage),
    );
    subscribe(theme, () => {});
    expect(read(theme)).toBe("dark");
  });

  it("mounts once per store, and each store reads for itself", () => {
    const storage = memoryStorage({ theme: '"dark"' });
    const theme = atomWithStorage(
      "theme",
      "light",
      createJSONStorage(() => storage),
    );
    const a = createStore();
    const b = createStore();

    subscribe(theme, () => {}, a);
    expect(read(theme, a)).toBe("dark");
    // The store that has not mounted it is still on the value a server would
    // have rendered, which is the point of the isolation.
    expect(read(theme, b)).toBe("light");

    subscribe(theme, () => {}, b);
    expect(read(theme, b)).toBe("dark");
    expect(storage.reads()).toBe(2);
  });

  it("reads on first use rather than on mount when getOnInit is set", () => {
    const storage = memoryStorage({ theme: '"dark"' });
    const theme = atomWithStorage(
      "theme",
      "light",
      createJSONStorage(() => storage),
      {
        getOnInit: true,
      },
    );
    expect(read(theme)).toBe("dark");
    expect(storage.reads()).toBe(1);
  });

  it("writes back on every change", () => {
    const storage = memoryStorage();
    const theme = atomWithStorage(
      "theme",
      "light",
      createJSONStorage(() => storage),
    );
    write(theme, "dark");
    expect(storage.entries.get("theme")).toBe('"dark"');
  });

  it("takes a reducer over the value it currently shows", () => {
    const storage = memoryStorage();
    const count = atomWithStorage(
      "count",
      1,
      createJSONStorage(() => storage),
    );
    write(count, (current) => current + 1);
    expect(read(count)).toBe(2);
    expect(storage.entries.get("count")).toBe("2");
  });

  it("removes the key on RESET rather than storing the initial value", () => {
    // Storing `initial` would leave the key behind, so "clear my preferences"
    // would persist the absence of a preference and the next schema change
    // would find it.
    const storage = memoryStorage();
    const theme = atomWithStorage(
      "theme",
      "light",
      createJSONStorage(() => storage),
    );
    write(theme, "dark");
    expect(storage.entries.has("theme")).toBe(true);

    write(theme, RESET);
    expect(storage.entries.has("theme")).toBe(false);
    expect(read(theme)).toBe("light");
  });

  it("falls back to the initial value when the stored data is malformed", () => {
    const storage = memoryStorage({ theme: "{not json" });
    const theme = atomWithStorage(
      "theme",
      "light",
      createJSONStorage(() => storage),
    );
    subscribe(theme, () => {});
    expect(read(theme)).toBe("light");
  });

  it("falls back to the initial value when revive rejects the stored data", () => {
    const storage = memoryStorage({ theme: '"solar"' });
    const themes = ["light", "dark"];
    const theme = atomWithStorage(
      "theme",
      "light",
      createJSONStorage(() => storage, {
        revive: (raw) => {
          if (typeof raw !== "string" || !themes.includes(raw)) {
            throw Error(`not a theme: ${String(raw)}`);
          }
          return raw;
        },
      }),
    );
    subscribe(theme, () => {});
    expect(read(theme)).toBe("light");
  });

  it("persists once for a batch of writes", () => {
    const storage = memoryStorage();
    const count = atomWithStorage(
      "count",
      0,
      createJSONStorage(() => storage),
    );
    batch(() => {
      write(count, 1);
      write(count, 2);
    });
    expect(storage.entries.get("count")).toBe("2");
  });

  it("persists without anything being mounted", () => {
    // A route handler writes state nobody is rendering. Persistence that
    // lived in a subscription would silently do nothing here.
    const storage = memoryStorage();
    const seen = atomWithStorage(
      "seen",
      0,
      createJSONStorage(() => storage),
    );
    write(seen, (current) => current + 1);
    expect(storage.entries.get("seen")).toBe("1");
  });

  it("takes an outside write through the storage's own subscribe", () => {
    const storage = memoryStorage({ theme: '"dark"' });
    // A set rather than one callback: every mounted store subscribes for
    // itself, under the same key, and a map keyed by the key alone would have
    // silently kept only the last of them.
    const outside: Set<(value: string) => void> = new Set();
    const json = createJSONStorage<string>(() => storage);
    const theme = atomWithStorage("theme", "light", {
      getItem: json.getItem,
      setItem: json.setItem,
      removeItem: json.removeItem,
      subscribe: (key, onChange) => {
        outside.add(onChange);
        return () => {
          outside.delete(onChange);
        };
      },
    });

    const a = createStore();
    const b = createStore();
    const stop = subscribe(theme, () => {}, a);
    subscribe(theme, () => {}, b);
    expect(outside.size).toBe(2);

    for (const onChange of Array.from(outside)) {
      onChange("solar");
    }
    expect(read(theme, a)).toBe("solar");
    expect(read(theme, b)).toBe("solar");

    // Unmounting takes that store's listener with it, so a store nothing is
    // rendering stops hearing about a tab it has no part in.
    stop();
    expect(outside.size).toBe(1);
  });

  it("degrades to an unpersisted atom where there is no storage at all", () => {
    // An edge runtime has no `localStorage`, so the identifier itself is not
    // defined and evaluating it throws rather than answering `undefined`.
    const missing = createJSONStorage(() => {
      throw ReferenceError("localStorage is not defined");
    });
    const theme = atomWithStorage("theme", "light", missing);
    subscribe(theme, () => {});
    expect(read(theme)).toBe("light");
    write(theme, "dark");
    expect(read(theme)).toBe("dark");
    write(theme, RESET);
    expect(read(theme)).toBe("light");
  });

  it("degrades to an unpersisted atom where storage refuses to be written", () => {
    // Safari in private mode, and a browser with site data blocked.
    const refuses = createJSONStorage(() => ({
      getItem: () => null,
      setItem: () => {
        throw Error("QuotaExceededError");
      },
      removeItem: () => {
        throw Error("QuotaExceededError");
      },
    }));
    const theme = atomWithStorage("theme", "light", refuses);
    subscribe(theme, () => {});
    write(theme, "dark");
    expect(read(theme)).toBe("dark");
  });
});

describe("atomWithAsyncStorage", () => {
  // The three questions ubugeeei-prod/uf#317 said had to be decided rather than typed are
  // the three tests that matter here: what a write does while the first read
  // is in flight, whether a failed read is an error or a fallback, and what
  // `RESET` means when the removal has not finished.

  it("is loading until the first read settles, then holds the stored value", async () => {
    const storage = controlledStorage({ theme: "dark" });
    const theme = atomWithAsyncStorage("theme", "light", storage.adapter);
    subscribe(theme, () => {});
    expect(read(theme)).toEqual({ state: "loading" });

    storage.settle();
    await Promise.resolve();
    await Promise.resolve();
    expect(read(theme)).toEqual({ state: "hasData", data: "dark" });
  });

  it("answers the initial value for a key nothing has stored", async () => {
    const storage = controlledStorage();
    const theme = atomWithAsyncStorage("theme", "light", storage.adapter);
    subscribe(theme, () => {});
    storage.settle();
    await Promise.resolve();
    await Promise.resolve();
    // Absent is not the same fact as failed, and this is the half that keeps
    // them apart: an absent key is the initial value, delivered as data.
    expect(read(theme)).toEqual({ state: "hasData", data: "light" });
  });

  it("holds a failed read as an error rather than falling back", async () => {
    // The whole reason this is a second constructor. `atomWithStorage` falls
    // back to `initial` because it has nowhere to put the failure; here there
    // is somewhere, and "the database is locked" is not "nobody has set a
    // preference".
    const storage = controlledStorage({ theme: "dark" });
    const theme = atomWithAsyncStorage("theme", "light", storage.adapter);
    subscribe(theme, () => {});

    storage.fail(Error("database is locked"));
    await Promise.resolve();
    await Promise.resolve();

    const settled = read(theme);
    expect(settled.state).toBe("hasError");
    expect(String(settled.error)).toContain("database is locked");
  });

  it("lets a write during the first read win, and abandons the read", async () => {
    const storage = controlledStorage({ draft: "stored" });
    const draft = atomWithAsyncStorage("draft", "", storage.adapter);
    subscribe(draft, () => {});
    expect(read(draft)).toEqual({ state: "loading" });
    expect(storage.signals).toHaveLength(1);
    expect(storage.signals[0].aborted).toBe(false);

    write(draft, "typed");
    expect(read(draft)).toEqual({ state: "hasData", data: "typed" });
    // Not merely ignored. The write took the load out of the atom's
    // dependencies, so the load lost its last reader and the cell underneath
    // stopped it — which is the generation `@uniflowed/cell` already keeps,
    // rather than a second one kept here.
    expect(storage.signals[0].aborted).toBe(true);

    storage.settle();
    await Promise.resolve();
    await Promise.resolve();
    expect(read(draft)).toEqual({ state: "hasData", data: "typed" });
  });

  it("reduces over the loadable, because there may be no value yet", async () => {
    const storage = controlledStorage({ count: "7" });
    const seen: Array<string> = [];
    const count = atomWithAsyncStorage("count", "0", storage.adapter);
    subscribe(count, () => {});

    write(count, (current) => {
      seen.push(current.state);
      return current.state === "hasData" ? `${Number(current.data) + 1}` : "1";
    });
    expect(seen).toEqual(["loading"]);
    expect(read(count)).toEqual({ state: "hasData", data: "1" });

    write(count, (current) => {
      seen.push(current.state);
      return current.state === "hasData" ? `${Number(current.data) + 1}` : "1";
    });
    expect(seen).toEqual(["loading", "hasData"]);
    expect(read(count)).toEqual({ state: "hasData", data: "2" });
  });

  it("writes back on every change", () => {
    const storage = controlledStorage();
    const theme = atomWithAsyncStorage("theme", "light", storage.adapter);
    subscribe(theme, () => {});
    write(theme, "dark");
    expect(storage.entries.get("theme")).toBe("dark");
  });

  it("removes the key on RESET and goes back to initial without reading again", async () => {
    const storage = controlledStorage({ theme: "dark" });
    const theme = atomWithAsyncStorage("theme", "light", storage.adapter);
    subscribe(theme, () => {});
    storage.settle();
    await Promise.resolve();
    await Promise.resolve();
    expect(read(theme)).toEqual({ state: "hasData", data: "dark" });
    expect(storage.reads()).toBe(1);

    write(theme, RESET);
    // No second read, and that is the decision rather than an accident:
    // reading again would race a removal that has not finished and could
    // answer with the value it had just deleted.
    expect(storage.reads()).toBe(1);
    expect(read(theme)).toEqual({ state: "hasData", data: "light" });
    expect(storage.entries.has("theme")).toBe(false);
  });

  it("reads nothing when it is declared", () => {
    const storage = controlledStorage({ theme: "dark" });
    atomWithAsyncStorage("theme", "light", storage.adapter);
    expect(storage.reads()).toBe(0);
  });

  it("starts its read for a store that only reads it, unlike the synchronous one", () => {
    // `atomWithStorage` deliberately reads nothing for a store that has not
    // mounted it, because the first client render has to be the one the
    // server already sent. There is nothing to disagree with here — the
    // server and the browser both render `loading` — so the read starts as
    // soon as a store is asked for the atom, whether or not anything is
    // subscribed. Worth pinning because it is the one habit that does not
    // carry over from the synchronous constructor.
    const storage = controlledStorage({ theme: "dark" });
    const theme = atomWithAsyncStorage("theme", "light", storage.adapter);
    expect(read(theme)).toEqual({ state: "loading" });
    expect(storage.reads()).toBe(1);
  });

  it("gives each store its own read", async () => {
    const storage = controlledStorage({ theme: "dark" });
    const theme = atomWithAsyncStorage("theme", "light", storage.adapter);
    const a = createStore();
    const b = createStore();

    subscribe(theme, () => {}, a);
    expect(storage.reads()).toBe(1);
    storage.settle();
    await Promise.resolve();
    await Promise.resolve();
    expect(read(theme, a)).toEqual({ state: "hasData", data: "dark" });

    // A second store is a second read, and it is still in flight while the
    // first store already has its answer — the isolation stores exist for.
    subscribe(theme, () => {}, b);
    expect(storage.reads()).toBe(2);
    expect(read(theme, b)).toEqual({ state: "loading" });
    expect(read(theme, a)).toEqual({ state: "hasData", data: "dark" });

    storage.settle();
    await Promise.resolve();
    await Promise.resolve();
    expect(read(theme, b)).toEqual({ state: "hasData", data: "dark" });
  });

  it("takes an outside write through the storage's own subscribe", async () => {
    const storage = controlledStorage({ theme: "dark" });
    const outside: Set<(value: string) => void> = new Set();
    const theme = atomWithAsyncStorage("theme", "light", {
      getItem: storage.adapter.getItem,
      setItem: storage.adapter.setItem,
      removeItem: storage.adapter.removeItem,
      subscribe: (key, onChange) => {
        outside.add(onChange);
        return () => {
          outside.delete(onChange);
        };
      },
    });

    const stop = subscribe(theme, () => {});
    expect(outside.size).toBe(1);
    for (const onChange of Array.from(outside)) {
      onChange("solar");
    }
    // Another tab's write is newer than a read that has not landed, so it
    // wins the same way a local write does.
    expect(read(theme)).toEqual({ state: "hasData", data: "solar" });

    stop();
    expect(outside.size).toBe(0);
  });

  it("does not wake a reader for a write of the value it already shows", async () => {
    const storage = controlledStorage({ theme: "dark" });
    const theme = atomWithAsyncStorage("theme", "light", storage.adapter);
    let wakes = 0;
    subscribe(theme, () => {
      wakes += 1;
    });
    storage.settle();
    await Promise.resolve();
    await Promise.resolve();
    const afterLoad = wakes;

    // The loadable is built fresh by the read, so without an equality lifted
    // from the value it would be a new object — and a re-render — every time.
    write(theme, "dark");
    expect(wakes).toBe(afterLoad);
    write(theme, "solar");
    expect(wakes).toBe(afterLoad + 1);
  });

  it("keeps the value a failed write did not persist, and attaches the rejection", async () => {
    // Attaching it is not tidiness: a rejected promise nobody is listening to
    // is an unhandled rejection, which ends a Node process. The listener here
    // is what would see one — while it is installed, Node reports to it
    // instead of exiting — and the turn boundary below is where Node decides.
    const unhandled: Array<mixed> = [];
    const onUnhandled = (error: mixed) => {
      unhandled.push(error);
    };
    process.on("unhandledRejection", onUnhandled);
    try {
      const theme = atomWithAsyncStorage("theme", "light", {
        getItem: () => Promise.resolve("dark"),
        setItem: () => Promise.reject(Error("quota exceeded")),
        removeItem: () => Promise.resolve(),
      });
      subscribe(theme, () => {});
      write(theme, "solar");
      // The value the caller wrote is the atom's whatever storage did with
      // it; it simply will not outlive the session.
      expect(read(theme)).toEqual({ state: "hasData", data: "solar" });

      await new Promise((resolve) => {
        setImmediate(resolve);
      });
      expect(unhandled).toEqual([]);
    } finally {
      process.off("unhandledRejection", onUnhandled);
    }
  });
});

describe("atomFamily", () => {
  it("returns the same atom for the same key", () => {
    const rowAtom = atomFamily((id: string) => atom(`row ${id}`));
    expect(rowAtom("a")).toBe(rowAtom("a"));
    expect(read(rowAtom("a"))).toBe("row a");
  });

  it("keeps members independent", () => {
    const rowAtom = atomFamily((id: string) => atom(id));
    const listener = fn();
    subscribe(rowAtom("a"), listener);
    write(rowAtom("b"), "changed");
    expect(listener).not.toHaveBeenCalled();
    expect(read(rowAtom("a"))).toBe("a");
  });

  it("forgets a member on remove, so an unbounded key space is bounded", () => {
    const rowAtom = atomFamily((id: string) => atom(id));
    const first = rowAtom("a");
    expect(rowAtom.size()).toBe(1);
    rowAtom.remove("a");
    expect(rowAtom.size()).toBe(0);
    expect(rowAtom("a")).not.toBe(first);
  });
});

describe("onMount", () => {
  it("runs for the first subscriber and its teardown for the last", () => {
    const events = [];
    const clock = atom(0, {
      onMount: () => {
        events.push("start");
        return () => {
          events.push("stop");
        };
      },
    });

    const first = subscribe(clock, () => {});
    const second = subscribe(clock, () => {});
    expect(events).toEqual(["start"]);

    first();
    expect(events).toEqual(["start"]);
    second();
    expect(events).toEqual(["start", "stop"]);
  });

  it("feeds values in through the handle it is given, exactly once", () => {
    // The mount runs while the selector that caused it is being linked up, so
    // it writes after that selector read the old value. Both halves of that
    // have to hold: the mount runs once — it ran twice while a notification
    // could be delivered mid-evaluation, and the second run saw its own first
    // write — and the value it wrote reaches the selector.
    const mounts = [];
    const feed = atom(0, {
      onMount: (mount) => {
        mounts.push(mount.get());
        mount.set(mount.get() + 42);
      },
    });
    const doubled = selector((get) => get(feed) * 2);
    subscribe(doubled, () => {});
    expect(mounts).toEqual([0]);
    expect(read(doubled)).toBe(84);
  });

  it("mounts once per store", () => {
    const events = [];
    const source = atom(0, {
      onMount: (mount) => {
        events.push("start");
        return () => events.push("stop");
      },
    });
    const store = createStore();

    const stop = subscribe(source, () => {});
    expect(events).toEqual(["start"]);
    const stopElsewhere = subscribe(source, () => {}, store);
    expect(events).toEqual(["start", "start"]);

    stop();
    expect(events).toEqual(["start", "start", "stop"]);
    stopElsewhere();
    expect(events).toEqual(["start", "start", "stop", "stop"]);
  });

  it("mounts an atom because something derived from it was subscribed", () => {
    const events = [];
    const source = atom(1, {
      onMount: () => {
        events.push("start");
        return () => events.push("stop");
      },
    });
    const doubled = selector((get) => get(source) * 2);
    const stop = subscribe(doubled, () => {});
    expect(events).toEqual(["start"]);
    stop();
    expect(events).toEqual(["start", "stop"]);
  });
});

describe("the React binding, rendered to markup", () => {
  it("renders the current value of an atom", () => {
    const name = atom("uf");
    component Greeting() {
      const shown = useAtomValue(name);
      return <p>hello {shown}</p>;
    }
    expect(renderToStaticMarkup(<Greeting />)).toBe("<p>hello uf</p>");
  });

  it("renders a value written before the render", () => {
    const count = atom(0);
    write(count, 41);
    component Count() {
      const shown = useAtomValue(count);
      return <span>{shown + 1}</span>;
    }
    expect(renderToStaticMarkup(<Count />)).toBe("<span>42</span>");
  });

  it("renders a cell from the layer below through useCell", () => {
    // A route loader hands out cells, not atoms. Reading one takes no store.
    const count = stateCell(3);
    const doubled = derivedCell(() => readCell(count) * 2);
    component Doubled() {
      return <b>{useCell(doubled)}</b>;
    }
    expect(renderToStaticMarkup(<Doubled />)).toBe("<b>6</b>");
    writeCell(count, 4);
    expect(renderToStaticMarkup(<Doubled />)).toBe("<b>8</b>");
  });

  it("gives useAtom the shape useState returns", () => {
    const count = atom(1);
    let setter = null;
    component Count() {
      const [value, set] = useAtom(count);
      setter = set;
      return <i>{value}</i>;
    }
    expect(renderToStaticMarkup(<Count />)).toBe("<i>1</i>");
    expect(typeof setter).toBe("function");
  });

  it("hands out one stable setter per atom, so a memoised child never rerenders", () => {
    const count = atom(0);
    const seen = [];
    component Writer() {
      seen.push(useSetAtom(count));
      return null;
    }
    renderToStaticMarkup(<Writer />);
    renderToStaticMarkup(<Writer />);
    expect(seen).toHaveLength(2);
    expect(seen[0]).toBe(seen[1]);
  });

  it("accepts a reducer through the setter", () => {
    const count = atom(10);
    let setter = null;
    component Count() {
      setter = useSetAtom(count);
      return null;
    }
    renderToStaticMarkup(<Count />);
    setter((current: number) => current + 5);
    expect(read(count)).toBe(15);
  });
});

describe("a persisted atom, from a server render to a client mount", () => {
  it("renders the same first value on both sides, then adopts the stored one", () => {
    // The bug, in one test. Storage holds `"dark"`; the server has no
    // storage and renders `"light"`. If the atom reads storage while the
    // module is being evaluated, the browser's first render is `"dark"`
    // against markup that says `"light"` — a hydration mismatch that no
    // amount of care in `useSyncExternalStore` can undo, because the value
    // was already wrong before React was called.
    const storage = memoryStorage({ theme: '"dark"' });
    const theme = atomWithStorage(
      "theme",
      "light",
      createJSONStorage(() => storage),
    );

    // Pushed during render, which is a side effect a component may not have —
    // allowed here because the sequence of rendered values is the assertion,
    // and there is no other way to see the first one.
    const rendered = [];
    component Theme() {
      const shown = useAtomValue(theme);
      rendered.push(shown);
      return <span>{shown}</span>;
    }

    // The server. Its own store, because a request is not a browser tab.
    const markup = renderToStaticMarkup(
      <Provider store={createStore()}>
        <Theme />
      </Provider>,
    );
    expect(markup).toBe("<span>light</span>");
    // And it read nothing: a server that reached for `localStorage` would
    // have thrown rather than mismatched.
    expect(storage.reads()).toBe(0);

    // The browser, with that markup on screen and this module now evaluated.
    const { container } = render(
      <Provider store={createStore()}>
        <Theme />
      </Provider>,
    );

    expect(rendered[0]).toBe("light");
    // The one that matters: what the client rendered first is what the
    // server sent.
    expect(rendered[1]).toBe("light");
    // And the stored value arrives on the render after the commit.
    expect(rendered[rendered.length - 1]).toBe("dark");
    expect(container.textContent).toBe("dark");
    expect(storage.reads()).toBe(1);
  });
});

describe("the React binding, in a DOM", () => {
  it("re-renders exactly the components that read the atom that changed", () => {
    const left = atom("L");
    const right = atom("R");
    const renders = { left: 0, right: 0, neither: 0 };

    component Left() {
      renders.left += 1;
      return <b>{useAtomValue(left)}</b>;
    }
    component Right() {
      renders.right += 1;
      return <i>{useAtomValue(right)}</i>;
    }
    component Neither() {
      renders.neither += 1;
      return <u>·</u>;
    }

    const { container } = render(
      <div>
        <Left />
        <Right />
        <Neither />
      </div>,
    );
    expect(renders).toEqual({ left: 1, right: 1, neither: 1 });

    act(() => {
      write(left, "L2");
    });

    expect(renders).toEqual({ left: 2, right: 1, neither: 1 });
    expect(container.textContent).toBe("L2R·");
  });

  it("does not re-render a component that only writes", () => {
    const count = atom(0);
    const renders = { reader: 0, writer: 0 };

    component Reader() {
      renders.reader += 1;
      return <output>{useAtomValue(count)}</output>;
    }
    component Writer() {
      renders.writer += 1;
      const increment = useSetAtom(count);
      return (
        <button type="button" onClick={() => increment((current) => current + 1)}>
          add
        </button>
      );
    }

    render(
      <div>
        <Reader />
        <Writer />
      </div>,
    );
    act(() => {
      write(count, 1);
    });

    expect(renders).toEqual({ reader: 2, writer: 1 });
  });

  it("re-renders a reader of a selector once for a write that reaches it twice", () => {
    const source = atom(0);
    const left = selector((get) => get(source) + 1);
    const right = selector((get) => get(source) * 10);
    const joined = selector((get) => `${get(left)}/${get(right)}`);
    let renders = 0;

    component Joined() {
      renders += 1;
      return <output>{useAtomValue(joined)}</output>;
    }

    const { container } = render(<Joined />);
    expect(renders).toBe(1);

    act(() => {
      write(source, 2);
    });

    expect(renders).toBe(2);
    expect(container.textContent).toBe("3/20");
  });

  it("does not re-render when the derived value did not change", () => {
    const rows = atom<$ReadOnlyArray<string>>(["a", "b"]);
    const count = selector((get) => get(rows).length);
    let renders = 0;

    component Count() {
      renders += 1;
      return <output>{useAtomValue(count)}</output>;
    }

    render(<Count />);
    act(() => {
      write(rows, ["c", "d"]);
    });
    expect(renders).toBe(1);

    act(() => {
      write(rows, ["c"]);
    });
    expect(renders).toBe(2);
  });

  it("re-renders once for a batch of writes", () => {
    const first = atom(0);
    const second = atom(0);
    const total = selector((get) => get(first) + get(second));
    let renders = 0;

    component Total() {
      renders += 1;
      return <output>{useAtomValue(total)}</output>;
    }

    const { container } = render(<Total />);
    act(() => {
      batch(() => {
        write(first, 1);
        write(second, 2);
      });
    });

    expect(renders).toBe(2);
    expect(container.textContent).toBe("3");
  });

  it("gives a subtree its own store through Provider", () => {
    const count = atom(0);
    const scoped = createStore();

    component Show() {
      return <output>{useAtomValue(count)}</output>;
    }

    const { container } = render(
      <div>
        <Show />
        <Provider store={scoped}>
          <Show />
        </Provider>
      </div>,
    );
    expect(container.textContent).toBe("00");

    act(() => {
      write(count, 5, scoped);
    });
    expect(container.textContent).toBe("05");

    act(() => {
      write(count, 9);
    });
    expect(container.textContent).toBe("95");
  });

  it("tells a component which store its subtree is using", () => {
    const scoped = createStore();
    let seen = null;

    component Probe() {
      seen = useStore();
      return null;
    }

    render(
      <Provider store={scoped}>
        <Probe />
      </Provider>,
    );
    expect(seen).toBe(scoped);
  });

  it("gives a Provider with no store one of its own", () => {
    const count = atom(0);

    component Show() {
      return <output>{useAtomValue(count)}</output>;
    }

    const { container } = render(
      <Provider>
        <Show />
      </Provider>,
    );
    act(() => {
      write(count, 7);
    });
    // The provider owns a store, so the default store's write does not reach
    // it.
    expect(container.textContent).toBe("0");
  });

  it("mounts an atom while a component reads it and unmounts it on the way out", () => {
    const events = [];
    const source = atom(0, {
      onMount: () => {
        events.push("start");
        return () => {
          events.push("stop");
        };
      },
    });

    component Show() {
      return <output>{useAtomValue(source)}</output>;
    }

    const { unmount } = render(<Show />);
    expect(events).toEqual(["start"]);
    unmount();
    expect(events).toEqual(["start", "stop"]);
  });

  it("renders an asynchronous atom's loading state and then its data", async () => {
    let settle = (value: string) => {};
    const user = asyncAtom(
      () =>
        new Promise((resolve) => {
          settle = resolve;
        }),
    );

    component User() {
      const settled = useAtomValue(user);
      return (
        <output>
          {
            match (settled) {
              {state: "hasData", data: const data} => data,
              {state: "hasError"} => "failed",
              _ => "loading",
            }
          }
        </output>
      );
    }

    const { container } = render(<User />);
    expect(container.textContent).toBe("loading");

    settle("Ada");
    await waitFor(() => {
      expect(container.textContent).toBe("Ada");
    });
  });
});

describe("useResetAtom", () => {
  it("hands a component a () => void that puts the atom back", () => {
    const theme = atomWithReset("light");

    component Switcher() {
      const [value, set] = useAtom(theme);
      const reset = useResetAtom(theme);
      return (
        <div>
          <output>{value}</output>
          <button type="button" onClick={() => set("dark")}>
            dark
          </button>
          <button type="button" onClick={reset}>
            reset
          </button>
        </div>
      );
    }

    const { container } = render(<Switcher />);
    expect(container.textContent).toBe("lightdarkreset");

    act(() => {
      container.querySelectorAll("button")[0].click();
    });
    expect(container.querySelector("output")?.textContent).toBe("dark");

    // The whole of the difference from `useSetAtom`: this goes straight onto
    // an `onClick`, with no wrapper to supply the symbol.
    act(() => {
      container.querySelectorAll("button")[1].click();
    });
    expect(container.querySelector("output")?.textContent).toBe("light");
  });

  it("resets the store its subtree is scoped to", () => {
    const theme = atomWithReset("light");
    const scoped = createStore();
    write(theme, "dark", scoped);
    write(theme, "solar");

    component Reset() {
      const reset = useResetAtom(theme);
      return (
        <button type="button" onClick={reset}>
          reset
        </button>
      );
    }

    const { container } = render(
      <Provider store={scoped}>
        <Reset />
      </Provider>,
    );
    act(() => {
      container.querySelector("button")?.click();
    });

    expect(read(theme, scoped)).toBe("light");
    expect(read(theme)).toBe("solar");
  });
});

describe("useAtomCallback", () => {
  it("reads the current value without subscribing the component", () => {
    const draft = atom("first");
    const seen: Array<string> = [];
    let renders = 0;

    component Submit() {
      renders += 1;
      const submit = useAtomCallback((get) => {
        seen.push(get(draft));
      });
      return (
        <button type="button" onClick={submit}>
          submit
        </button>
      );
    }

    const { container } = render(<Submit />);
    expect(renders).toBe(1);

    act(() => {
      write(draft, "second");
    });
    // The point of the hook: the value it reads changed and the component
    // that reads it did not re-render, because it never subscribed.
    expect(renders).toBe(1);

    act(() => {
      container.querySelector("button")?.click();
    });
    expect(seen).toEqual(["second"]);
  });

  it("writes, and takes the arguments the caller passes", () => {
    const count = atom(0);

    component Add() {
      const add = useAtomCallback((get, set, by: number) => {
        set(count, get(count) + by);
        return get(count);
      });
      return (
        <button type="button" onClick={() => add(5)}>
          add
        </button>
      );
    }

    const { container } = render(<Add />);
    act(() => {
      container.querySelector("button")?.click();
    });
    expect(read(count)).toBe(5);
  });

  it("reads the store its subtree is scoped to", () => {
    const count = atom(0);
    const scoped = createStore();
    write(count, 7, scoped);
    const seen: Array<number> = [];

    component Peek() {
      const peek = useAtomCallback((get) => {
        seen.push(get(count));
      });
      return (
        <button type="button" onClick={peek}>
          peek
        </button>
      );
    }

    const { container } = render(
      <Provider store={scoped}>
        <Peek />
      </Provider>,
    );
    act(() => {
      container.querySelector("button")?.click();
    });
    expect(seen).toEqual([7]);
  });
});

// The part of `uf check --json` the block below reads. A message arrives as
// spans rather than a string so that a renderer can mark the code inside it,
// which is why the comparison joins it back together first.
type CheckDiagnostic = {
  primary: { path: string, start: { line: number, column: number } },
  message: Array<{ kind: string, text: string }>,
};
type CheckReport = {
  typeCheck: { status: string, filesChecked: number, diagnostics: Array<CheckDiagnostic> },
};

/**
 * This checkout, found by a file only it has.
 *
 * Searched for upwards rather than assumed, because `uf test` runs a suite
 * from the project root and `uf test#library` runs it from the workspace —
 * the same reasoning `ui.test.js` writes out for the same block.
 */
const repository: string = (() => {
  const wanted = path.join("packages", "state", "internal", "composed.js");
  const from = process.env.UF_PROJECT_ROOT ?? process.cwd();
  let directory = from;
  for (let up = 0; up < 8; up += 1) {
    if (fs.existsSync(path.join(directory, wanted))) return directory;
    directory = path.dirname(directory);
  }
  throw new Error(`could not find ${wanted} above ${from}`);
})();

// The binary running this suite: `uf test` puts its own path in `UF_BINARY`,
// so this checks *this* build rather than whatever `uf` is on PATH.
const UF: string = (() => {
  const binary = process.env.UF_BINARY;
  if (binary == null || binary === "") {
    throw new Error("UF_BINARY is not set: this test runs `uf check`, and `uf test` names it");
  }
  return binary;
})();

describe("the utilities' types, held to what the checker actually says", () => {
  // The promise `jotai/utils` makes and the one no amount of rendering can
  // check: a derived atom's value infers from what it derives, a persisted
  // atom's value is the `Loadable` and not the thing inside it, and a misuse
  // of either is reported at the call rather than as an `any` three files
  // away.
  //
  // `tests/type-tests/state-utils.js` is that misuse, written down. It is
  // *supposed* to fail `uf check`, it marks each line that must fail with a
  // `// expect:` comment, and this reads both and compares them — so a change
  // that makes one of them stop being an error fails here, and so does one
  // that makes something else in that file start being one.
  //
  // Both paths go to the checker in one command, and that is load-bearing:
  // `uf check` builds its module map from the files it is asked about, so a
  // relative import that leaves that set resolves to an any-typed value —
  // after which every type in the fixture is `any` and every line of it
  // passes. The fixture's own header says why it is not inside the package.
  const fixture = path.join("tests", "type-tests", "state-utils.js");

  it("reports every misuse, and only the misuses", () => {
    const source = fs.readFileSync(path.join(repository, fixture), "utf8").split("\n");
    const wanted = new Map<number, string>();
    source.forEach((line, index) => {
      const marker = line.match(/^\s*\/\/ expect: (.+)$/);
      if (marker != null) {
        // Lines are one-based, and the line that must fail is the next one.
        wanted.set(index + 2, marker[1]);
      }
    });
    // Without this the test would pass on a fixture somebody had emptied.
    expect(wanted.size).toBeGreaterThan(8);

    const run = spawnSync(UF, ["check", "tests/type-tests", "packages/state", "--json"], {
      cwd: repository,
      encoding: "utf8",
      maxBuffer: 32 * 1024 * 1024,
    });
    // A non-zero status is expected — the fixture is a file of deliberate
    // errors. The answer is on stdout either way, and when it is not, this
    // says which command in which directory printed nothing rather than
    // leaving a reader with `Unexpected end of JSON input`.
    if (run.stdout === "") {
      throw new Error(
        `\`uf check tests/type-tests packages/state --json\` in ${repository} printed ` +
          `nothing: status ${String(run.status)}, stderr ${JSON.stringify(run.stderr)}`,
      );
    }
    const report: CheckReport = JSON.parse(run.stdout);
    expect(report.typeCheck.status).toBe("checked");
    expect(report.typeCheck.filesChecked).toBeGreaterThan(0);

    const reported = new Map<number, string>();
    for (const diagnostic of report.typeCheck.diagnostics) {
      if (diagnostic.primary.path.endsWith(fixture)) {
        reported.set(
          diagnostic.primary.start.line,
          diagnostic.message.map((span) => span.text).join(""),
        );
      }
    }

    const missing = [];
    for (const [line, expected] of wanted) {
      const said = reported.get(line);
      if (said == null || !said.includes(expected)) {
        missing.push(`${fixture}:${String(line)} should say "${expected}", said ${String(said)}`);
      }
    }
    // Every marked line is an error, with the message the fixture predicted.
    expect(missing).toEqual([]);

    // And nothing else in the file is: the correct uses at the bottom of the
    // fixture check, which is what says it is not simply broken.
    const unexpected = [...reported.keys()]
      .filter((line) => !wanted.has(line))
      .map((line) => `${fixture}:${String(line)} ${String(reported.get(line))}`);
    expect(unexpected).toEqual([]);
  });
});
