// @flow
//
// `@uniflowed/state`: the atom surface, and the React binding rendered for
// real on `react-dom/server`.

import { describe, expect, fn, it } from "@uniflowed/test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import {
  atom,
  atomFamily,
  atomWithStorage,
  batch,
  read,
  selector,
  subscribe,
  useAtom,
  useAtomValue,
  useCell,
  useSetAtom,
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
  it("is a cell, readable and writable with no provider", () => {
    const count = atom(1);
    expect(read(count)).toBe(1);
    write(count, 2);
    expect(read(count)).toBe(2);
  });

  it("shares one value between every reader of the module", () => {
    const count = atom(0);
    const listener = fn();
    subscribe(count, listener);
    write(count, 1);
    expect(listener).toHaveBeenCalledTimes(1);
  });
});

describe("selector", () => {
  it("derives from atoms and re-derives when they change", () => {
    const first = atom(2);
    const second = atom(3);
    const product = selector(() => read(first) * read(second));
    expect(read(product)).toBe(6);
    write(first, 4);
    expect(read(product)).toBe(12);
  });

  it("does not wake readers when the derived value is unchanged", () => {
    const rows = atom<$ReadOnlyArray<number>>([1, 2, 3]);
    const count = selector(() => read(rows).length);
    const listener = fn();
    subscribe(count, listener);
    write(rows, [4, 5, 6]);
    expect(listener).not.toHaveBeenCalled();
    write(rows, [1]);
    expect(listener).toHaveBeenCalledTimes(1);
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
});

describe("the React binding", () => {
  it("renders the current value of an atom", () => {
    const name = atom("uf");
    const Greeting = () => createElement("p", null, `hello ${useAtomValue(name)}`);
    expect(renderToStaticMarkup(createElement(Greeting))).toBe("<p>hello uf</p>");
  });

  it("renders a value written before the render", () => {
    const count = atom(0);
    write(count, 41);
    const Count = () => createElement("span", null, String(useAtomValue(count) + 1));
    expect(renderToStaticMarkup(createElement(Count))).toBe("<span>42</span>");
  });

  it("renders a derived cell through useCell", () => {
    const count = atom(3);
    const doubled = selector(() => read(count) * 2);
    const Doubled = () => createElement("b", null, String(useCell(doubled)));
    expect(renderToStaticMarkup(createElement(Doubled))).toBe("<b>6</b>");
  });

  it("gives useAtom the shape useState returns", () => {
    const count = atom(1);
    let setter = null;
    const Count = () => {
      const [value, set] = useAtom(count);
      setter = set;
      return createElement("i", null, String(value));
    };
    expect(renderToStaticMarkup(createElement(Count))).toBe("<i>1</i>");
    expect(typeof setter).toBe("function");
  });

  it("hands out one stable setter per atom, so a memoised child never rerenders", () => {
    const count = atom(0);
    const seen = [];
    const Writer = () => {
      seen.push(useSetAtom(count));
      return null;
    };
    renderToStaticMarkup(createElement(Writer));
    renderToStaticMarkup(createElement(Writer));
    expect(seen).toHaveLength(2);
    expect(seen[0]).toBe(seen[1]);
  });

  it("accepts a reducer through the setter", () => {
    const count = atom(10);
    let setter = null;
    const Count = () => {
      setter = useSetAtom(count);
      return null;
    };
    renderToStaticMarkup(createElement(Count));
    setter((current: number) => current + 5);
    expect(read(count)).toBe(15);
  });
});
