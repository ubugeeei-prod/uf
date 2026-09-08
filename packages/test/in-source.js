// @flow
//
// In-source tests: `if (import.meta.uf.test) { … }` in the file being tested.
//
// A three-line test for a three-line pure function does not want a file of its
// own, an import of the thing it tests, and a path that has to be kept in step
// with it. `import.meta.uf.test` is the block that holds it, and it is the
// same idea as Vitest's `import.meta.vitest` for the same reason.
//
// # What a block looks like
//
//     import { expect, it } from "@uniflowed/test";
//
//     export function add(a: number, b: number): number {
//       return a + b;
//     }
//
//     if (import.meta.uf.test) {
//       it("adds", () => {
//         expect(add(1, 2)).toBe(3);
//       });
//     }
//
// An ordinary import for the bindings, because that is what makes them typed:
// Flow has no `typeof import("…")` for a library definition to use, so a
// marker that carried the API could not be described to the checker. The
// marker's *value* is the API anyway — `const { it } = import.meta.uf.test` is
// what somebody arriving from Vitest writes and it works — but it is untyped,
// and the import is the form uf recommends.
//
// The import costs nothing in a build. The branch folds to `if (void 0)` and
// goes, `@uniflowed/test` declares `sideEffects: false` so the now-unused
// import goes with it, and the package carries a `browser` field mapping the
// six `node:` builtins its edges import at `./internal/browser/node.js` — so
// the bundler does not even print a warning about modules it is in the middle
// of removing.
//
// # What makes the block disappear
//
// `uf` compiles the marker away rather than leaving it to be falsy at runtime:
// `crates/uf_transform`'s printer substitutes `void 0` for
// `import.meta.uf.test` in every transform except the ones `uf test` asks for,
// so `uf build` sees `if (void 0)` and the bundler removes the block. There is
// no `import.meta.uf` at runtime, in any host, which is also why the block
// cannot throw in a browser that never heard of uf.
//
// `tests/library/in-source.test.js` and `tests/library/in-source-subject.js`
// are the block running. The other half is in `crates/uf_cli/tests/vite.rs`,
// which builds a fixture whose block holds a string that appears nowhere else
// and reads the emitted client bundle back looking for it — in the same file
// the module's other marker has to be present in, because "a string is
// missing" is only evidence when something establishes the module is not.
//
// # Why the marker is a call, and what it is called with
//
// Under `uf test` the printer substitutes
// `globalThis.__ufInSourceTests?.(import.meta.url)`, and this module is what
// answers it. The argument is the module asking, because a source file with an
// in-source block is an ordinary module and is imported more than once in a
// normal run: `uf test` imports `add.js` as the file it is running, and
// `add.test.js` imports it again for the function. Both evaluations reach the
// marker. Only the first is the file the run is reporting on, so only the
// first is given the API — otherwise every in-source case would register twice
// and the second copy would be filed under whichever test file imported it.

import {
  afterAll,
  afterEach,
  beforeAll,
  beforeEach,
  describe,
  it,
  test,
} from "./internal/registry.js";
import { fn, spyOn } from "./internal/spy.js";

import type { Expect } from "./internal/expect.js";
import type { Uft } from "./internal/namespace.js";
import { expect } from "./internal/expect.js";
import { uft } from "./internal/namespace.js";

/**
 * The name `uf` compiles `import.meta.uf.test` into a call on.
 *
 * Exported so the worker and the tests name it once; it is spelled out in `crates/uf_transform/src/print.rs` as well, and
 * `tests/library/in-source.test.js` is what keeps the two spellings equal.
 */
export const IN_SOURCE_GLOBAL: "__ufInSourceTests" = "__ufInSourceTests";

/**
 * What an in-source block is handed.
 *
 * The subset of `@uniflowed/test` a test body needs, and deliberately not all
 * of it: a block that wants module mocking or a snapshot is a block that has
 * outgrown living inside the file it tests, and `import { uft } from
 * "@uniflowed/test"` in a file of its own is the answer to that. `uft` is here
 * anyway because spies and the fake clock are ordinary for a unit test.
 */
export type InSourceTests = {
  readonly describe: typeof describe,
  readonly it: typeof it,
  readonly test: typeof test,
  readonly expect: Expect,
  readonly beforeAll: typeof beforeAll,
  readonly beforeEach: typeof beforeEach,
  readonly afterAll: typeof afterAll,
  readonly afterEach: typeof afterEach,
  readonly fn: typeof fn,
  readonly spyOn: typeof spyOn,
  readonly uft: Uft,
};

/**
 * The API every in-source block in a run shares.
 *
 * One frozen object rather than one per file: the bindings are the module's
 * own singletons — `it` registers into the one registry the worker resets
 * between files — so a copy per file would only be a copy of the same
 * references.
 */
const API: InSourceTests = Object.freeze({
  describe,
  it,
  test,
  expect,
  beforeAll,
  beforeEach,
  afterAll,
  afterEach,
  fn,
  spyOn,
  uft,
});

/**
 * The module URL, without whatever a host appended to it.
 *
 * The worker busts its import cache with `?uf-run=<generation>` so a watch
 * rerun sees the edited file, which means the file being run reports an
 * `import.meta.url` that its own path does not match. Comparing the part
 * before the query is comparing the module.
 *
 * A hash is stripped for the same reason and never appears in practice; it
 * costs one `indexOf` to not have to think about it again.
 */
function moduleUrl(url: string): string {
  const end = url.search(/[?#]/);
  return end === -1 ? url : url.slice(0, end);
}

/**
 * Hand in-source blocks their API for the file `url`, and nothing to any other
 * module.
 *
 * Returns the function that puts the global back as it was, which the worker
 * calls when the file is over. Restoring rather than leaving it set is the
 * same rule the worker applies to environment stubs and module mocks: a file
 * may not change what the next file on this worker sees.
 */
export function installInSourceTests(url: string): () => void {
  const wanted = moduleUrl(url);
  const globals: { [string]: mixed } = globalThis as $FlowFixMe;
  const previous = globals[IN_SOURCE_GLOBAL];
  globals[IN_SOURCE_GLOBAL] = (asking: string): InSourceTests | void =>
    moduleUrl(asking) === wanted ? API : undefined;
  return () => {
    globals[IN_SOURCE_GLOBAL] = previous;
  };
}
