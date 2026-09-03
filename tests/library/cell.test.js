// @flow
//
// `@uniflowed/cell` under the runner that ships with the toolchain.

import { describe, expect, fn, it } from "@uniflowed/test";
import {
  batch,
  cell,
  computed,
  read,
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
    const loaded = resource(() => (load(): $FlowFixMe));
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
