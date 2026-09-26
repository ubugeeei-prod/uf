// @flow
//
// The object `expect` hands back, as a caller can reach it.
//
// Every matcher is a getter on a prototype shared by every expectation (see
// `internal/expect.js`'s `bind`), so that an assertion does not build forty-one
// closures to call one. What a caller could do with the object when each
// matcher was its own closure, it can still do.

import { describe, expect, it } from "@uniflowed/test";

import { AssertionError } from "./internal/expect.js";

/** What `run` threw, or `null`. */
function thrown(run: () => mixed): mixed {
  try {
    run();
  } catch (error) {
    return error;
  }
  return null;
}

describe("a matcher taken off its expectation", () => {
  it("still asserts about the value it was taken from", () => {
    const { toBe } = expect(1);
    toBe(1);
    const failure = thrown(() => toBe(2));
    expect(failure).toBeInstanceOf(AssertionError);
    expect((failure as $FlowFixMe).message).toBe("expected 1 to be 2");
  });

  it("keeps its polarity under .not", () => {
    const { toBe } = expect(1).not;
    toBe(2);
    expect((thrown(() => toBe(1)) as $FlowFixMe).message).toBe("expected 1 not to be 1");
  });

  it("can be handed to a callback", () => {
    [2, 3].forEach(expect(1).not.toBe);
  });
});

describe("two expectations", () => {
  it("do not share a value", () => {
    const one = expect("one");
    const two = expect("two");
    one.toBe("one");
    two.toBe("two");
    expect(thrown(() => one.toBe("two"))).toBeInstanceOf(AssertionError);
  });

  it("negate twice back to the original", () => {
    expect(1).not.not.toBe(1);
    expect(thrown(() => expect(1).not.not.toBe(2))).toBeInstanceOf(AssertionError);
  });
});

describe("the promise surface", () => {
  it("is on the expectation", async () => {
    await expect(Promise.resolve(3)).resolves.toBe(3);
    await expect(Promise.reject(new Error("no"))).rejects.toThrow("no");
  });

  it("is not on its negation", () => {
    expect((expect(Promise.resolve(1)).not as $FlowFixMe).resolves).toBe(undefined);
    expect((expect(Promise.resolve(1)).not as $FlowFixMe).rejects).toBe(undefined);
  });
});
