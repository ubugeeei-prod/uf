// @flow
//
// `expect.any` and the rest of the matchers that stand in for a value.
//
// The point of most of these tests is *depth*: a matcher nested inside an
// expected object has to work for the same reason a top-level one does, because
// `equals` asks every value it meets whether it is a matcher rather than
// special-casing the outermost one.

import { describe, expect, it } from "@uniflowed/test";

describe("expect.any", () => {
  it("accepts a primitive as well as its wrapper", () => {
    // A string literal is not an instance of anything, and a test that wrote
    // `expect.any(String)` meant the type rather than the wrapper.
    expect("uf").toEqual(expect.any(String));
    expect(1).toEqual(expect.any(Number));
    expect(true).toEqual(expect.any(Boolean));
    expect(1n).toEqual(expect.any(BigInt));
    expect(() => {}).toEqual(expect.any(Function));
  });

  it("accepts an instance of a class", () => {
    class Token {}

    expect(new Token()).toEqual(expect.any(Token));
    expect(new Date()).toEqual(expect.any(Date));
  });

  it("rejects the wrong type", () => {
    expect(1).not.toEqual(expect.any(String));
    expect("1").not.toEqual(expect.any(Number));
    expect(null).not.toEqual(expect.any(Object));
  });

  it("treats Object as any non-null object", () => {
    // Not "has Object.prototype in its chain": a null-prototype object would
    // fail that, and a test would find it baffling.
    expect({}).toEqual(expect.any(Object));
    expect(Object.create(null)).toEqual(expect.any(Object));
  });
});

describe("expect.anything", () => {
  it("accepts everything except null and undefined", () => {
    expect(0).toEqual(expect.anything());
    expect("").toEqual(expect.anything());
    expect(false).toEqual(expect.anything());
    expect(null).not.toEqual(expect.anything());
    expect(undefined).not.toEqual(expect.anything());
  });
});

describe("expect.objectContaining", () => {
  it("ignores the properties it was not asked about", () => {
    expect({ id: "1", name: "uf", extra: true }).toEqual(expect.objectContaining({ name: "uf" }));
  });

  it("fails when a named property is absent or different", () => {
    expect({ name: "uf" }).not.toEqual(expect.objectContaining({ missing: 1 }));
    expect({ name: "uf" }).not.toEqual(expect.objectContaining({ name: "other" }));
  });

  it("nests, and holds matchers inside itself", () => {
    expect({ user: { id: "abc", age: 3 } }).toEqual(
      expect.objectContaining({ user: expect.objectContaining({ id: expect.any(String) }) }),
    );
  });
});

describe("expect.arrayContaining", () => {
  it("holds when every wanted element is somewhere in the array", () => {
    expect([1, 2, 3]).toEqual(expect.arrayContaining([3, 1]));
    expect([1, 2]).not.toEqual(expect.arrayContaining([4]));
  });

  it("compares elements structurally, and by matcher", () => {
    expect([{ id: 1 }, { id: 2 }]).toEqual(expect.arrayContaining([{ id: 2 }]));
    expect(["a", "b"]).toEqual(expect.arrayContaining([expect.any(String)]));
  });

  it("rejects something that is not an array", () => {
    expect("abc").not.toEqual(expect.arrayContaining(["a"]));
  });
});

describe("string matchers", () => {
  it("stringContaining looks for a substring", () => {
    expect("uniflowed").toEqual(expect.stringContaining("flow"));
    expect("uniflowed").not.toEqual(expect.stringContaining("react"));
    expect(1).not.toEqual(expect.stringContaining("1"));
  });

  it("stringMatching takes a pattern or a substring", () => {
    expect("uf-2026").toEqual(expect.stringMatching(/^uf-\d+$/));
    expect("uf-2026").toEqual(expect.stringMatching("2026"));
    expect("uf").not.toEqual(expect.stringMatching(/^\d+$/));
  });
});

describe("expect.closeTo", () => {
  it("uses the same tolerance as toBeCloseTo", () => {
    // A test that moves an assertion from one to the other should not have its
    // verdict change.
    expect(0.1 + 0.2).toEqual(expect.closeTo(0.3));
    expect(0.1 + 0.2).toBeCloseTo(0.3);
    expect(1.005).toEqual(expect.closeTo(1.0, 1));
    expect(1.5).not.toEqual(expect.closeTo(1.0, 1));
  });
});

describe("expect.not", () => {
  it("negates a matcher in place", () => {
    expect({ ok: true }).toEqual(expect.not.objectContaining({ error: expect.anything() }));
    expect("uf").toEqual(expect.not.stringContaining("react"));
    expect([1]).toEqual(expect.not.arrayContaining([2]));
  });
});

describe("depth", () => {
  it("works wherever a matcher is nested", () => {
    const payload = {
      meta: { requestId: "r-1", at: new Date() },
      items: [{ id: 1, tags: ["a"] }],
    };

    expect(payload).toEqual({
      meta: { requestId: expect.stringMatching(/^r-/), at: expect.any(Date) },
      items: expect.arrayContaining([
        expect.objectContaining({ tags: expect.arrayContaining([expect.any(String)]) }),
      ]),
    });
  });

  it("works through toMatchObject too", () => {
    expect({ a: 1, b: { c: "x", d: 2 } }).toMatchObject({
      b: { c: expect.any(String) },
    });
  });

  it("works in a spy's recorded arguments", () => {
    // The most common real use: asserting on a call without pinning every
    // field of every argument.
    const send = expect;
    const calls = [["POST", { id: "generated-id", body: "hi" }]];

    expect(calls[0]).toEqual(["POST", expect.objectContaining({ body: "hi" })]);
    expect(send).toBeDefined();
  });
});
