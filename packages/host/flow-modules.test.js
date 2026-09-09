// @flow
//
// Which modules uf is responsible for, asked the two ways a host can ask.
//
// `packages/host/transform.js` answers that question twice. `isFlowModule` is
// the function, and it is what Node's loader hooks call once a module has
// already reached them. `FLOW_MODULE_PATTERN` is the same rule as a pattern,
// and it exists because Bun's plugin API selects modules *before* it calls
// anything and has no way for a hook to say "not mine": every shape a
// declining `onLoad` could return is
//
//     TypeError: onLoad() expects an object returned
//
// so `packages/host/bun-preload.js` took a Bun process down on the first
// ordinary `.js` dependency it met — which is to say on every Bun project.
// See ubugeeei-prod/uf#418.
//
// Two spellings of one rule is a drift risk, and this file is what makes it
// not one. The contract is equality, not approximation: for every path with no
// query string and no leading NUL the two must give the same answer, because
// the pattern is what decides whether the function is ever consulted. A
// pattern that under-matched would leave a `@uniflowed` package's Flow source
// to Bun's own parser; one that over-matched would put a CommonJS dependency
// through `onLoad`, and anything that leaves `onLoad` is an ES module to Bun
// whatever its contents say — so `import dep from "dep"` would stop finding a
// default export. Neither failure names this file when it happens, which is
// why the table below is as long as it is.

import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "@uniflowed/test";

/** The module under test, reached as a path so no resolution is involved. */
const TRANSFORM: string = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "./transform.js",
);

const host = await import(TRANSFORM);

/**
 * Every path either expression could get wrong on its own, and the answer.
 *
 * The interesting half is the four `node_modules` rows. `isFlowModule` asks
 * about the *last* `/node_modules/` on the path, so a `@uniflowed` package
 * that a package manager placed inside another package's `node_modules` is
 * still uf's to transform, and a plain dependency vendored inside a
 * `@uniflowed` package is still not. A pattern that merely rejected every path
 * containing `node_modules` would agree with the function on the first three
 * rows and quietly stop transforming the fourth.
 */
const table: $ReadOnlyArray<[string, boolean]> = [
  ["/p/app.js", true],
  ["/p/app.jsx", true],
  ["/p/app.mjs", true],
  ["/p/app.cjs", true],
  ["/p/app.ts", false],
  ["/p/app.json", false],
  ["/p/app.css", false],
  ["/p/app", false],
  ["/p/.js", true],
  // A directory whose name merely contains the word.
  ["/p/node_modules_of_mine/x.js", true],
  ["/p/my_node_modules/x.js", true],
  // And a scope whose name merely starts with it.
  ["/p/node_modules/@uniflowedish/x.js", false],
  ["/p/node_modules/dep/index.js", false],
  ["/p/node_modules/@uniflowed/core/index.js", true],
  ["/p/node_modules/@uniflowed/core/node_modules/dep/index.js", false],
  ["/p/node_modules/dep/node_modules/@uniflowed/core/index.js", true],
  ["/node_modules/@uniflowed/core/a/b/c.cjs", true],
];

describe("which modules uf is responsible for", () => {
  it("says the same thing as a function and as a pattern", () => {
    for (const [id, expected] of table) {
      expect({ id, flow: host.isFlowModule(id) }).toEqual({ id, flow: expected });
      expect({ id, flow: host.FLOW_MODULE_PATTERN.test(id) }).toEqual({ id, flow: expected });
    }
  });

  it("covers every extension the loader claims, from the one list", () => {
    // The pattern is built from `FLOW_EXTENSIONS`, so a fifth extension added
    // to that list reaches Bun without anybody remembering to widen a regex —
    // which is what the Bun filter, hand-written as `/\.(js|jsx|mjs)$/`, had
    // failed to do for `.cjs` since the day that extension was added.
    for (const extension of host.FLOW_EXTENSIONS) {
      expect({ extension, flow: host.FLOW_MODULE_PATTERN.test(`/p/a${extension}`) }).toEqual({
        extension,
        flow: true,
      });
    }
  });

  it("leaves a bundler's own ids to the function", () => {
    // The two cases outside the pattern's contract, and the reason it has one:
    // a NUL-prefixed id is a synthetic module and `?raw` is a bundler's
    // parameter. Neither is a path a host asks a filesystem about, so the
    // pattern is never asked — but `isFlowModule` still is, by the Vite
    // plugin, and it still has to be right about them.
    expect(host.isFlowModule("\0uf:virtual.js")).toBe(false);
    expect(host.isFlowModule("/p/app.js?raw")).toBe(true);
    expect(host.isFlowModule("/p/style.css?inline.js")).toBe(false);
  });
});
