// @flow
//
// Snapshots.
//
// A snapshot is an assertion whose expected value was written by the last run
// rather than by a person, which is the whole idea and the whole danger. The
// tests that matter here are the ones about *when* a snapshot is written: a
// missing one is created, a different one fails, and only a run that was asked
// to update rewrites anything.

import { describe, expect, it } from "@uniflowed/test";

describe("toMatchSnapshot", () => {
  it("matches a primitive", () => {
    expect("uniflowed").toMatchSnapshot();
  });

  it("matches an object", () => {
    expect({ name: "uf", version: 1, tags: ["flow", "react"] }).toMatchSnapshot();
  });

  it("keys two snapshots in one test apart", () => {
    // Both live under this test's name, numbered in the order they were taken.
    expect("first").toMatchSnapshot();
    expect("second").toMatchSnapshot();
  });

  it("takes a hint, which lands in the key", () => {
    expect({ ok: true }).toMatchSnapshot("the happy path");
  });

  it("handles a value with backticks and a substitution in it", () => {
    // A backtick would end the template the snapshot file stores it in, and
    // `${` would start a substitution that runs code when the file is read.
    expect("a `backtick` and a ${substitution}").toMatchSnapshot();
  });
});

describe("toMatchInlineSnapshot", () => {
  it("matches a snapshot written in the test itself", () => {
    expect("uniflowed").toMatchInlineSnapshot(`"uniflowed"`);
  });

  it("ignores the indentation the call gave it", () => {
    // The stored form sits inside the call, so indentation is not the
    // assertion.
    expect({ a: 1 }).toMatchInlineSnapshot(`
      { a: 1 }
    `);
  });

  it("fails with what to paste in when there is none", () => {
    // uf does not rewrite a test file — a tool that edits the file you are
    // editing is a tool that loses work — so it reports and leaves the decision
    // to a person.
    let message = "";
    try {
      expect("anything").toMatchInlineSnapshot();
    } catch (error) {
      message = String(error);
    }

    expect(message).toContain("Paste this into the call");
    expect(message).toContain("anything");
  });

  it("fails with both sides when it does not match", () => {
    let message = "";
    try {
      expect("received").toMatchInlineSnapshot(`"stored"`);
    } catch (error) {
      message = String(error);
    }

    expect(message).toContain("stored");
    expect(message).toContain("received");
  });
});
