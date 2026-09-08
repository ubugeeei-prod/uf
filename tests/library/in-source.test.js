// @flow
//
// In-source tests: what the marker is, and who is allowed to see it.
//
// The assertions in `./in-source-subject.js` prove that an in-source block
// runs. This file proves the three properties around it that a failure would
// otherwise be silent about:
//
//   * the marker in the file `uf test` is running is there, so a block here is
//     as good as a block in an ordinary source file;
//   * the marker in a module that file merely imports is `undefined`, so an
//     in-source case is registered once however many test files reach for the
//     function beside it;
//   * the global the marker compiles into is the name `@uniflowed/test`
//     publishes, so the compiler and the runtime cannot drift apart in
//     silence.
//
// What is *not* here is the elimination, because it cannot be: this file runs
// under `uf test`, where the marker is deliberately present. That half is
// `crates/uf_cli/tests/in_source.rs`, which builds a fixture and reads the
// bundle back.

import { describe, expect, it } from "@uniflowed/test";
import { IN_SOURCE_GLOBAL } from "@uniflowed/test/in-source";

import { MARKER_AT_IMPORT, withoutRepeats } from "./in-source-subject.js";

describe("in-source tests", () => {
  it("marks the file the run is reporting on", () => {
    // The guard an in-source block is written against, in the file `uf test`
    // is running. `uf` compiled it into a call carrying this module's URL, and
    // the worker answered.
    expect(import.meta.uf.test != null).toBe(true);
  });

  it("hands nothing to a module the file imported", () => {
    // `./in-source-subject.js` was evaluated by this file's import, so its
    // marker was read while this file was the one being run. Its block
    // therefore registered nothing here — if it had, this run would report
    // `duplicate > removes a repeat` twice, once under a file that does not
    // declare it.
    expect(MARKER_AT_IMPORT).toBe(false);
    // And the function it exports is still the ordinary export it always was.
    expect(withoutRepeats([1, 1, 2])).toEqual([1, 2]);
  });

  it("answers only for the module that asks", () => {
    const ask: mixed = (globalThis as $FlowFixMe)[IN_SOURCE_GLOBAL];
    expect(typeof ask).toBe("function");
    const asking = ask as $FlowFixMe;
    const here = String(import.meta.url);
    expect(asking(here)).toBeTruthy();
    // The query the worker appends to bust its import cache is not part of the
    // module's identity, and a hash never is.
    expect(asking(`${here}?uf-run=99`)).toBeTruthy();
    expect(asking("file:///somewhere/else.js")).toBeUndefined();
  });

  it("carries the names a block reads out of the marker", () => {
    // A block may take its bindings from the marker rather than from an
    // import, which is the shape somebody arriving from Vitest will write. It
    // is untyped — Flow has no `typeof import("…")` for a library definition
    // to use — so the names are checked here instead.
    const asking = (globalThis as $FlowFixMe)[IN_SOURCE_GLOBAL];
    const api = asking(String(import.meta.url));
    for (const name of [
      "describe",
      "it",
      "test",
      "expect",
      "beforeAll",
      "beforeEach",
      "afterAll",
      "afterEach",
      "fn",
      "spyOn",
      "uft",
    ]) {
      expect(api[name]).toBeDefined();
    }
  });
});
