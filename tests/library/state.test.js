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

import { describe, expect, fn, it } from "@uniflowed/test";
import * as React from "@uniflowed/react";
import { renderToStaticMarkup } from "react-dom/server";
import { act, render, waitFor } from "@uniflowed/react-testing";
import { cell, computed, read as readCell, write as writeCell } from "@uniflowed/cell";
import {
  Provider,
  RESET,
  action,
  asyncAtom,
  atom,
  atomFamily,
  atomWithDefault,
  atomWithStorage,
  batch,
  createStore,
  getDefaultStore,
  read,
  selector,
  subscribe,
  unwrap,
  useAtom,
  useAtomValue,
  useCell,
  useSetAtom,
  useStore,
  writableSelector,
  write,
} from "@uniflowed/state";

/** A `StorageAdapter` backed by a map, so the tests need no browser. */
function memoryStorage(seed?: { [string]: string }): {
  getItem: (key: string) => null | string,
  setItem: (key: string, value: string) => void,
  entries: Map<string, string>,
} {
  const entries: Map<string, string> = new Map(Object.entries(seed ?? {}));
  return {
    entries,
    getItem: (key) => entries.get(key) ?? null,
    setItem: (key, value) => {
      entries.set(key, value);
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
  /** A load whose promises are settled by the test, one per key. */
  function controlled() {
    const settle = new Map();
    const fail = new Map();
    return {
      settle,
      fail,
      load: (key) =>
        new Promise((resolve, reject) => {
          settle.set(key, resolve);
          fail.set(key, reject);
        }),
    };
  }

  it("is loading until the promise settles, then holds the data", async () => {
    const { settle, load } = controlled();
    const user = asyncAtom(() => load("only"));
    subscribe(user, () => {});
    expect(read(user)).toEqual({ state: "loading" });

    settle.get("only")("Ada");
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

    settle.get("a")("value a");
    await Promise.resolve();
    await Promise.resolve();
    expect(read(user)).toEqual({ state: "hasData", data: "value a" });

    write(id, "b");
    expect(read(user)).toEqual({ state: "loading" });
    settle.get("b")("value b");
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
    settle.get("fast")("fast value");
    await Promise.resolve();
    await Promise.resolve();
    expect(read(user)).toEqual({ state: "hasData", data: "fast value" });

    // The load it superseded settles last, which is the race this loses safely.
    settle.get("slow")("slow value");
    await Promise.resolve();
    await Promise.resolve();
    expect(read(user)).toEqual({ state: "hasData", data: "fast value" });
  });

  it("unwraps to a fallback until the data arrives", async () => {
    const { settle, load } = controlled();
    const user = asyncAtom(() => load("only"));
    const name = unwrap(user, "anonymous");
    subscribe(name, () => {});
    expect(read(name)).toBe("anonymous");

    settle.get("only")("Ada");
    await Promise.resolve();
    await Promise.resolve();
    expect(read(name)).toBe("Ada");
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

describe("atomWithStorage", () => {
  it("is a plain atom when no storage is given", () => {
    const theme = atomWithStorage("theme", "light");
    expect(read(theme)).toBe("light");
  });

  it("starts from what was stored", () => {
    const storage = memoryStorage({ theme: '"dark"' });
    expect(read(atomWithStorage("theme", "light", storage))).toBe("dark");
  });

  it("writes back on every change", () => {
    const storage = memoryStorage();
    const theme = atomWithStorage("theme", "light", storage);
    write(theme, "dark");
    expect(storage.entries.get("theme")).toBe('"dark"');
  });

  it("falls back to the initial value when the stored data is malformed", () => {
    const storage = memoryStorage({ theme: "{not json" });
    expect(read(atomWithStorage("theme", "light", storage))).toBe("light");
  });

  it("persists once for a batch of writes", () => {
    const storage = memoryStorage();
    const count = atomWithStorage("count", 0, storage);
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
    const seen = atomWithStorage("seen", 0, storage);
    write(seen, (current) => current + 1);
    expect(storage.entries.get("seen")).toBe("1");
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
    const count = cell(3);
    const doubled = computed(() => readCell(count) * 2);
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
