// @flow
//
// An ordinary module with an in-source test in it.
//
// Two files use this one and they ask different questions of it.
//
// `uf test` runs *this file*, because discovery finds the `it` inside the
// block exactly as it finds one in a `.test.js` file — that is what "the same
// discovery" in ubugeeei-prod/uf#518 means, and a passing case named
// `duplicate > removes a repeat` in a run of the library suite is the proof.
//
// The bindings come from an ordinary top-level import rather than out of the
// marker, which is the form uf recommends: they are fully typed that way, and
// both the import and the block leave a production build — the marker folds to
// `void 0`, the bundler drops the branch, and `@uniflowed/test` declares
// `sideEffects: false` so nothing keeps the import alive.
//
// `./in-source.test.js` imports it, and what it checks is the half that has
// nothing to do with the assertions: a module imported by a test file must not
// register the block a second time. `MARKER_AT_IMPORT` below records what the
// marker said at the moment this module was evaluated, which is the only place
// that fact can be observed from.

import { describe, expect, it } from "@uniflowed/test";

/**
 * `values` with consecutive repeats removed.
 *
 * Three lines, one behaviour, no dependencies — the shape ubugeeei-prod/uf#518
 * is about, where a test file of its own is more ceremony than the function.
 */
export function withoutRepeats<T>(values: $ReadOnlyArray<T>): Array<T> {
  return values.filter((value, at) => at === 0 || value !== values[at - 1]);
}

/**
 * Whether this module was given the in-source API when it was evaluated.
 *
 * `uf test` compiles `import.meta.uf.test` into a call carrying this module's
 * own URL, so the answer is "only when this module is the file being run".
 * Every other host compiles it to `void 0`, so this is `false` in a build and
 * the constant folds away with everything else that reads it.
 */
export const MARKER_AT_IMPORT: boolean = import.meta.uf.test != null;

if (import.meta.uf.test) {
  describe("duplicate", () => {
    it("removes a repeat", () => {
      expect(withoutRepeats([1, 1, 2, 2, 2, 3])).toEqual([1, 2, 3]);
    });

    it("keeps a value that comes back later", () => {
      expect(withoutRepeats(["a", "b", "a"])).toEqual(["a", "b", "a"]);
    });
  });
}
