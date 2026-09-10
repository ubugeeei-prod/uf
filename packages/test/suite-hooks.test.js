// @flow
//
// `@uniflowed/test`'s own suite hooks: when `beforeAll` and `afterAll` run.
//
// A file's tests almost always live inside a `describe`, so the file's own
// `beforeAll` has no case of its own to run before — it has a suite. Deferring
// setup until a *direct* child case ran meant that hook never ran at all, in
// the ordinary shape, silently: a file that set up a fixture at the top and
// asserted on it inside a `describe` saw whatever the fixture was before the
// hook it wrote.
//
// The order matters as much as the fact. An inner suite's setup runs after its
// ancestors' so that it can build on what they made, and teardown unwinds the
// other way.

import { afterAll, beforeAll, describe, expect, it } from "@uniflowed/test";

const order: Array<string> = [];

beforeAll(() => {
  order.push("file setup");
});

describe("an outer suite whose children are all suites", () => {
  beforeAll(() => {
    order.push("outer setup");
  });

  afterAll(() => {
    order.push("outer teardown");
  });

  describe("the inner one, which holds the only case", () => {
    beforeAll(() => {
      order.push("inner setup");
    });

    it("has seen every ancestor's setup, outermost first", () => {
      expect(order).toEqual(["file setup", "outer setup", "inner setup"]);
    });
  });
});

describe("the suite that reads what the one before it left", () => {
  it("saw the inner suite torn down before this one ran", () => {
    // The outer suite above closed when its last case finished, so its
    // teardown has run by the time a sibling suite starts.
    expect(order).toEqual(["file setup", "outer setup", "inner setup", "outer teardown"]);
  });
});

describe("a suite whose cases are all skipped", () => {
  beforeAll(() => {
    order.push("never");
  });

  it.skip("does not run", () => {});
});

describe("what the skipped suite did", () => {
  it("set nothing up, because nothing in it ran", () => {
    expect(order).not.toContain("never");
  });
});
