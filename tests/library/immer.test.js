// @flow
//
// `@uniflowed/immer`.
//
// These test the properties that make a draft-based update *correct*, not that
// `produce` returns something: that the base survives untouched, that what did
// not change is the same object it was, that what comes out cannot be written
// to, that a draft dies at the boundary, and that patches say the same thing
// in both directions.

import { describe, expect, it } from "@uniflowed/test";
import {
  applyPatches,
  current,
  freeze,
  isDraft,
  isDraftable,
  original,
  produce,
  produceWithPatches,
  setAutoFreeze,
} from "@uniflowed/immer";

type Row = { id: number, done: boolean };
type Board = { rows: Array<Row>, meta: { title: string, seen: number } };

/** A fresh board per test: `produce` freezes what a result shares with it. */
function board(): Board {
  return {
    rows: [
      { id: 1, done: false },
      { id: 2, done: false },
    ],
    meta: { title: "inbox", seen: 0 },
  };
}

describe("produce", () => {
  it("never writes to the base", () => {
    const base = board();
    const before = JSON.stringify(base);

    const next = produce(base, (draft) => {
      draft.rows[0].done = true;
      draft.rows.push({ id: 3, done: false });
      draft.meta.title = "done";
    });

    expect(JSON.stringify(base)).toBe(before);
    expect(next.rows[0].done).toBe(true);
    expect(next.rows).toHaveLength(3);
    expect(next.meta.title).toBe("done");
  });

  it("shares every subtree the recipe did not touch", () => {
    const base = board();

    const next = produce(base, (draft) => {
      draft.rows[0].done = true;
    });

    // Changed: the root, the array, and the one row that was written.
    expect(next).not.toBe(base);
    expect(next.rows).not.toBe(base.rows);
    expect(next.rows[0]).not.toBe(base.rows[0]);
    // Unchanged, and the *same object* — this is the property the whole
    // library exists for, and the reason a memoised React child that reads
    // `meta` does not re-render when a row changes.
    expect(next.rows[1]).toBe(base.rows[1]);
    expect(next.meta).toBe(base.meta);
  });

  it("returns the base itself when nothing changed", () => {
    const base = board();

    // Reading a nested value must not count as a change, or every selector
    // that peeks at state would allocate a copy of everything it looked at.
    expect(produce(base, () => {})).toBe(base);
    expect(
      produce(base, (draft) => {
        draft.rows[0].id;
        draft.meta.title;
      }),
    ).toBe(base);
    // Nor does writing back what is already there.
    expect(
      produce(base, (draft) => {
        draft.meta.title = "inbox";
      }),
    ).toBe(base);
  });

  it("copies only the path from the root to what changed", () => {
    const deep = {
      a: { b: { c: { value: 1 } }, sibling: { value: 2 } },
      other: { value: 3 },
    };

    const next = produce(deep, (draft) => {
      draft.a.b.c.value = 9;
    });

    expect(next.a.b.c.value).toBe(9);
    expect(next.a.sibling).toBe(deep.a.sibling);
    expect(next.other).toBe(deep.other);
    expect(next.a.b.c).not.toBe(deep.a.b.c);
  });

  it("keeps a value the recipe deleted out of the result", () => {
    const base: { name: string, nickname?: string } = { name: "ada", nickname: "a" };

    const next = produce(base, (draft) => {
      delete draft.nickname;
    });

    expect("nickname" in next).toBe(false);
    expect(base.nickname).toBe("a");
  });
});

describe("the published value", () => {
  it("is frozen, and writing to it throws", () => {
    const next = produce(board(), (draft) => {
      draft.meta.seen = 1;
    });

    expect(Object.isFrozen(next)).toBe(true);
    expect(Object.isFrozen(next.meta)).toBe(true);
    // Every module is strict mode, so this is a TypeError rather than a write
    // that silently does nothing — which is the entire point of freezing.
    expect(() => {
      next.meta.seen = 2;
    }).toThrow();
    expect(() => {
      next.rows.push({ id: 3, done: false });
    }).toThrow();
  });

  it("freezes the base's untouched subtrees too, because it shares them", () => {
    const base = board();

    const next = produce(base, (draft) => {
      draft.meta.seen = 1;
    });

    // `next.rows[1] === base.rows[1]`, so an unfrozen `base.rows[1]` would be
    // a mutable object reachable from an "immutable" result.
    expect(next.rows[1]).toBe(base.rows[1]);
    expect(Object.isFrozen(base.rows[1])).toBe(true);
  });

  it("is not frozen while auto-freezing is off", () => {
    setAutoFreeze(false);
    try {
      const next = produce(board(), (draft) => {
        draft.meta.seen = 1;
      });
      expect(Object.isFrozen(next)).toBe(false);
      next.meta.seen = 7;
      expect(next.meta.seen).toBe(7);
    } finally {
      setAutoFreeze(true);
    }
  });
});

describe("freeze", () => {
  it("stops at one level unless asked to go deeper", () => {
    const shallow = { inner: { value: 1 } };
    freeze(shallow);
    expect(Object.isFrozen(shallow)).toBe(true);
    expect(Object.isFrozen(shallow.inner)).toBe(false);

    const deep = { inner: { value: 1 } };
    freeze(deep, true);
    expect(Object.isFrozen(deep.inner)).toBe(true);
  });

  it("closes a Map and a Set, which Object.freeze does not", () => {
    const next = produce({ byId: new Map([["a", 1]]), tags: new Set(["x"]) }, (draft) => {
      draft.byId.set("b", 2);
      draft.tags.add("y");
    });

    // `Object.freeze` seals a collection's properties and leaves its entries
    // alone, so "frozen" would have been a lie for exactly these two types.
    expect(() => next.byId.set("c", 3)).toThrow(/frozen/);
    expect(() => next.tags.add("z")).toThrow(/frozen/);
    expect(() => next.byId.delete("a")).toThrow(/frozen/);
  });
});

describe("the draft lifecycle", () => {
  it("is not usable once produce has returned", () => {
    let escaped: null | Board = null;

    produce(board(), (draft) => {
      escaped = draft;
    });

    const kept = escaped;
    // Compared by identity rather than handed to `expect`: reading anything at
    // all off a revoked proxy throws, and a matcher inspecting its received
    // value is a read like any other.
    expect(kept === null).toBe(false);
    // A draft kept past the boundary is revoked, not merely discouraged: this
    // is what stops one being stored as React state or captured by a callback
    // and read after the render that produced it.
    expect(() => {
      if (kept != null) {
        kept.meta.seen = 1;
      }
    }).toThrow(/revoked/);
    expect(() => {
      if (kept != null) {
        return kept.meta;
      }
      return null;
    }).toThrow(/revoked/);
  });

  it("is revoked when the recipe throws, and the failure comes through", () => {
    let escaped: null | Board = null;

    expect(() =>
      produce(board(), (draft) => {
        escaped = draft;
        draft.meta.seen = 1;
        throw new Error("recipe exploded");
      }),
    ).toThrow(/recipe exploded/);

    const kept = escaped;
    expect(() => {
      if (kept != null) {
        return kept.meta;
      }
      return null;
    }).toThrow(/revoked/);
  });

  it("answers isDraft inside the recipe and not outside it", () => {
    const base = board();
    let sawDraft = false;

    const next = produce(base, (draft) => {
      sawDraft = isDraft(draft) && isDraft(draft.meta);
      draft.meta.seen = 1;
    });

    expect(sawDraft).toBe(true);
    expect(isDraft(next)).toBe(false);
    expect(isDraft(base)).toBe(false);
  });

  it("refuses defineProperty, which cannot be recorded as an assignment", () => {
    expect(() =>
      produce(board(), (draft) => {
        Object.defineProperty(draft, "extra", { value: 1 });
      }),
    ).toThrow(/defineProperty/);
  });
});

describe("original and current", () => {
  it("hands back the base, and a snapshot that does not follow the draft", () => {
    const base = board();

    produce(base, (draft) => {
      expect(original(draft)).toBe(base);
      expect(original(draft.meta)).toBe(base.meta);

      draft.rows.push({ id: 3, done: false });
      const snapshot = current(draft);

      expect(isDraft(snapshot)).toBe(false);
      expect(snapshot.rows).toHaveLength(3);
      // Untouched subtrees are shared with the base rather than copied.
      expect(snapshot.meta).toBe(base.meta);

      draft.rows.push({ id: 4, done: false });
      expect(snapshot.rows).toHaveLength(3);
      expect(current(draft).rows).toHaveLength(4);
    });
  });

  it("rejects a value that is not a draft", () => {
    expect(() => original({ a: 1 })).toThrow(/original/);
    expect(() => current({ a: 1 })).toThrow(/current/);
  });
});

describe("arrays", () => {
  it("records push, splice, pop and index writes", () => {
    const base = { list: [1, 2, 3] };

    const next = produce(base, (draft) => {
      draft.list.push(4);
      draft.list[0] = 9;
      draft.list.splice(2, 1);
    });

    expect(next.list).toEqual([9, 2, 4]);
    expect(base.list).toEqual([1, 2, 3]);
  });

  it("keeps untouched items identical", () => {
    const rows = [{ n: 1 }, { n: 2 }, { n: 3 }];

    const next = produce({ rows }, (draft) => {
      draft.rows[1].n = 20;
    });

    expect(next.rows[0]).toBe(rows[0]);
    expect(next.rows[2]).toBe(rows[2]);
    expect(next.rows[1]).not.toBe(rows[1]);
  });

  it("is still an array to Array.isArray inside the recipe", () => {
    let sawArray = false;

    produce({ list: [1] }, (draft) => {
      sawArray = Array.isArray(draft.list);
      draft.list.push(2);
    });

    expect(sawArray).toBe(true);
  });
});

describe("Map drafts", () => {
  it("drafts the values it hands out, and shares the ones it does not", () => {
    const kept = { n: 2 };
    const base = {
      byId: new Map([
        ["a", { n: 1 }],
        ["b", kept],
      ]),
    };

    const next = produce(base, (draft) => {
      const row = draft.byId.get("a");
      if (row != null) {
        row.n = 10;
      }
    });

    expect(next.byId.get("a")).toEqual({ n: 10 });
    expect(base.byId.get("a")).toEqual({ n: 1 });
    expect(next.byId.get("b")).toBe(kept);
    expect(next.byId).not.toBe(base.byId);
  });

  it("supports set, delete, clear, size and iteration", () => {
    const base = {
      byId: new Map([
        ["a", 1],
        ["b", 2],
      ]),
    };

    const next = produce(base, (draft) => {
      expect(draft.byId.size).toBe(2);
      expect(draft.byId.has("a")).toBe(true);
      draft.byId.set("c", 3);
      draft.byId.delete("a");
      expect([...draft.byId.keys()]).toEqual(["b", "c"]);
      expect([...draft.byId.values()]).toEqual([2, 3]);
    });

    expect([...next.byId.entries()]).toEqual([
      ["b", 2],
      ["c", 3],
    ]);
    expect(next.byId instanceof Map).toBe(true);
    expect(base.byId.size).toBe(2);

    const cleared = produce(next, (draft) => {
      draft.byId.clear();
    });
    expect(cleared.byId.size).toBe(0);
  });

  it("freezes the values inside a Map, not only the Map", () => {
    const base = {
      byId: new Map([
        ["a", { n: 1 }],
        ["b", { n: 2 }],
      ]),
    };

    const next = produce(base, (draft) => {
      const row = draft.byId.get("a");
      if (row != null) {
        row.n = 9;
      }
    });

    // A collection whose entries are still writable is not frozen in any sense
    // a caller cares about, and `Object.freeze` alone does not reach them.
    const changed = next.byId.get("a");
    const untouched = next.byId.get("b");
    expect(changed != null && Object.isFrozen(changed)).toBe(true);
    expect(untouched != null && Object.isFrozen(untouched)).toBe(true);
  });

  it("returns the base when a Map is only read", () => {
    const base = { byId: new Map([["a", { n: 1 }]]) };

    expect(
      produce(base, (draft) => {
        draft.byId.get("a");
        draft.byId.has("a");
      }),
    ).toBe(base);
  });
});

describe("Set drafts", () => {
  it("adds, deletes and answers has", () => {
    const base = { tags: new Set(["a", "b"]) };

    const next = produce(base, (draft) => {
      expect(draft.tags.has("a")).toBe(true);
      draft.tags.delete("a");
      draft.tags.add("c");
    });

    expect([...next.tags]).toEqual(["b", "c"]);
    expect([...base.tags]).toEqual(["a", "b"]);
    expect(next.tags instanceof Set).toBe(true);
  });

  it("drafts its members, in the order they were in", () => {
    const base = { tags: new Set([{ name: "a" }, { name: "b" }]) };

    const next = produce(base, (draft) => {
      for (const tag of draft.tags) {
        if (tag.name === "a") {
          tag.name = "z";
        }
      }
    });

    expect([...next.tags].map((tag) => tag.name)).toEqual(["z", "b"]);
    expect([...base.tags].map((tag) => tag.name)).toEqual(["a", "b"]);
  });

  it("returns the base when a Set is only read", () => {
    const base = { tags: new Set(["a"]) };

    expect(
      produce(base, (draft) => {
        expect(draft.tags.has("a")).toBe(true);
        expect(draft.tags.size).toBe(1);
      }),
    ).toBe(base);
  });
});

describe("what the recipe returns", () => {
  it("replaces the state when it returns a value", () => {
    const next = produce({ a: 1 }, () => ({ a: 5 }));

    expect(next).toEqual({ a: 5 });
    expect(Object.isFrozen(next)).toBe(true);
  });

  it("uses the draft when it returns undefined", () => {
    const next = produce({ a: 1 }, (draft) => {
      draft.a = 2;
    });

    expect(next).toEqual({ a: 2 });
  });

  it("refuses to both return a value and write to the draft", () => {
    // Neither answer is defensible, so there is no answer: silently dropping
    // the assignments or silently dropping the return would both read as
    // working code.
    expect(() =>
      produce({ a: 1 }, (draft) => {
        draft.a = 2;
        return { a: 3 };
      }),
    ).toThrow(/returned a new value/);
  });

  it("accepts the draft itself as the return value", () => {
    const next = produce({ a: 1 }, (draft) => {
      draft.a = 2;
      return draft;
    });

    expect(next).toEqual({ a: 2 });
  });

  it("works on a value that cannot be drafted at all", () => {
    expect(produce(1, () => 2)).toBe(2);
    expect(produce("a", () => undefined)).toBe("a");
  });
});

describe("the curried form", () => {
  it("makes a reducer, and passes the extra arguments through", () => {
    type Counter = { count: number, log: Array<string> };

    const step: (Counter, number) => Counter = produce((draft: Counter, by: number) => {
      draft.count += by;
      draft.log.push(`+${String(by)}`);
    });

    const start: Counter = { count: 0, log: [] };
    const once = step(start, 2);
    const twice = step(once, 3);

    expect(twice.count).toBe(5);
    expect(twice.log).toEqual(["+2", "+3"]);
    expect(start.count).toBe(0);
  });
});

describe("nested and repeated produce", () => {
  it("finishes an inner produce inside the outer result", () => {
    const rows = [{ id: 1 }];
    const base = { rows, page: 1 };

    const next = produce(base, (draft) => {
      draft.rows = produce(rows, (inner) => {
        inner.push({ id: 2 });
      });
    });

    expect(next.rows).toHaveLength(2);
    // The inner call froze nothing — it was inside a scope the outer call
    // still owned — and the outer call froze the lot on its way out.
    expect(Object.isFrozen(next.rows)).toBe(true);
    expect(Object.isFrozen(next.rows[1])).toBe(true);
    expect(base.rows).toHaveLength(1);
  });

  it("keeps sharing across a chain of produces", () => {
    const base = { a: { n: 1 }, b: { n: 2 }, c: { n: 3 } };

    const one = produce(base, (draft) => {
      draft.a.n = 10;
    });
    const two = produce(one, (draft) => {
      draft.b.n = 20;
    });
    const three = produce(two, (draft) => {
      draft.c.n = 30;
    });

    expect(three.a).toBe(one.a);
    expect(three.b).toBe(two.b);
    expect(three.c).not.toBe(base.c);
    expect(base).toEqual({ a: { n: 1 }, b: { n: 2 }, c: { n: 3 } });
  });
});

describe("patches", () => {
  it("round-trip in both directions for objects and arrays", () => {
    const base = board();

    const [next, patches, inverse] = produceWithPatches(base, (draft) => {
      draft.meta.title = "done";
      draft.rows[0].done = true;
      draft.rows.push({ id: 3, done: false });
    });

    expect(patches.length).toBeGreaterThan(0);
    expect(applyPatches(board(), patches)).toEqual(next);
    expect(applyPatches(next, inverse)).toEqual(board());
  });

  it("round-trip a removal from the middle of an array", () => {
    const base = { list: [1, 2, 3, 4] };

    const [next, patches, inverse] = produceWithPatches(base, (draft) => {
      draft.list.splice(1, 2);
    });

    expect(next.list).toEqual([1, 4]);
    expect(applyPatches({ list: [1, 2, 3, 4] }, patches)).toEqual(next);
    expect(applyPatches(next, inverse)).toEqual({ list: [1, 2, 3, 4] });
  });

  it("round-trip for a Map", () => {
    const before = () => ({ byId: new Map([["a", 1]]) });

    const [next, patches, inverse] = produceWithPatches(before(), (draft) => {
      draft.byId.set("b", 2);
      draft.byId.delete("a");
    });

    expect(applyPatches(before(), patches)).toEqual(next);
    expect(applyPatches(next, inverse)).toEqual(before());
  });

  it("round-trip for a Set", () => {
    const before = () => ({ tags: new Set(["a", "b"]) });

    const [next, patches, inverse] = produceWithPatches(before(), (draft) => {
      draft.tags.delete("a");
      draft.tags.add("c");
    });

    expect(applyPatches(before(), patches)).toEqual(next);
    expect(applyPatches(next, inverse)).toEqual(before());
  });

  it("round-trip when one drafted value ends up at two keys", () => {
    const before = () => ({ rows: [{ id: 1 }, { id: 2 }] });

    const [next, patches, inverse] = produceWithPatches(before(), (draft) => {
      const row = draft.rows[0];
      row.id = 11;
      draft.rows.push(row);
    });

    expect(next.rows).toEqual([{ id: 11 }, { id: 2 }, { id: 11 }]);
    expect(applyPatches(before(), patches)).toEqual(next);
    expect(applyPatches(next, inverse)).toEqual(before());
  });

  it("describes a whole-state replacement with an empty path", () => {
    const [next, patches, inverse] = produceWithPatches({ a: 1 }, () => ({ a: 5 }));

    expect(patches).toEqual([{ op: "replace", path: [], value: { a: 5 } }]);
    expect(applyPatches({ a: 1 }, patches)).toEqual(next);
    expect(applyPatches(next, inverse)).toEqual({ a: 1 });
  });

  it("records nothing for a recipe that changed nothing", () => {
    const [next, patches, inverse] = produceWithPatches(board(), (draft) => {
      draft.meta.title = "inbox";
    });

    expect(patches).toEqual([]);
    expect(inverse).toEqual([]);
    expect(next).toEqual(board());
  });

  it("shares the subtrees a patch did not reach", () => {
    const base = freeze(board(), true);
    const [, patches] = produceWithPatches(base, (draft) => {
      draft.meta.seen = 1;
    });

    const applied = applyPatches(base, patches);

    expect(applied.rows).toBe(base.rows);
    expect(applied.meta).not.toBe(base.meta);
  });

  it("replays into a draft, writing to it in place", () => {
    const base = { count: 0, note: "n" };
    const [, patches] = produceWithPatches(base, (draft) => {
      draft.count = 5;
    });

    const next = produce({ count: 1, note: "n" }, (draft) => {
      applyPatches(draft, patches);
    });

    expect(next.count).toBe(5);
  });

  it("refuses a patch that addresses the prototype chain", () => {
    // Patches are data, and data arrives from somewhere. Walking through
    // `__proto__` would let a patch stream change what every object in the
    // process inherits.
    expect(() =>
      applyPatches({ nested: {} }, [
        { op: "add", path: ["nested", "__proto__", "polluted"], value: true },
      ]),
    ).toThrow(/__proto__/);
  });
});

describe("a large structure", () => {
  it("shares all of it but the path that changed", () => {
    const rows: Array<Row> = [];
    for (let index = 0; index < 10000; index += 1) {
      rows.push({ id: index, done: false });
    }
    const base = { rows, meta: { title: "big", seen: 0 } };

    const next = produce(base, (draft) => {
      draft.rows[5000].done = true;
    });

    let shared = 0;
    for (let index = 0; index < rows.length; index += 1) {
      if (next.rows[index] === rows[index]) {
        shared += 1;
      }
    }

    // 9,999 of 10,000 rows are the identical object, and the tenth is the one
    // the recipe wrote to. This is the number that says the implementation
    // copies the path and not the tree.
    expect(shared).toBe(9999);
    expect(next.meta).toBe(base.meta);
    expect(next.rows[5000].done).toBe(true);
    expect(base.rows[5000].done).toBe(false);
  });
});

describe("isDraftable", () => {
  it("covers what produce can draft, and nothing else", () => {
    expect(isDraftable({})).toBe(true);
    expect(isDraftable([])).toBe(true);
    expect(isDraftable(new Map())).toBe(true);
    expect(isDraftable(new Set())).toBe(true);
    expect(isDraftable(Object.create(null))).toBe(true);

    expect(isDraftable(null)).toBe(false);
    expect(isDraftable(1)).toBe(false);
    expect(isDraftable("a")).toBe(false);
    // A class instance is opaque on purpose: copying one means copying
    // invariants this library cannot see.
    expect(isDraftable(new Date())).toBe(false);
  });
});
