// @flow
//
// The bytes of `dist/` on their way into a compiled binary.
//
// `uf build --compile` embeds everything `uf build` wrote: `uf_bundle::embed`
// writes one generated module holding base64 of every file in the output
// directory, and the standalone link bundles it. The module is data — its size
// is the size of the *site*, and it grows with every page somebody adds.
//
// It stopped being bundleable when the docs site passed eight megabytes:
//
//     TransformError: source is 8442536 bytes, over the 8388608 byte ceiling
//       [plugin uf:flow] docs/.uf/build/compile/assets.js
//
// That ceiling is `uf_transform::MAX_SOURCE_BYTES`, and it belongs where it
// is: it bounds the parser against a *source* file nobody meant to compile.
// What went wrong is that uf handed it something that is not source, so a
// build that had already succeeded failed at the link step, naming a file the
// project never wrote and a limit it did not break. Raising the ceiling would
// have weakened it everywhere to make room for the one file it should never
// have been asked about.
//
// The fix is which name the payload enters the graph under, and the assertions
// below are that name and its one consequence. They are written against the
// *rule* rather than against a size, on purpose: a test that packed nine
// megabytes of base64 would assert that today's ceiling is 8 MiB, and this has
// to keep holding for a project whose `dist/` is ten times that.

import path from "node:path";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { describe, expect, it } from "@uniflowed/test";

// By path rather than through the package, the way `flow-modules.test.js`
// reaches it: this is the exact module `packages/vite/index.js` gates on.
import { isFlowModule } from "../../packages/host/transform.js";
import {
  COMPILE_ASSETS_ID,
  COMPILE_ASSETS_RESOLVED_ID,
  compileAssetsPlugin,
} from "./internal/compile-assets.js";

/** A generated payload on disk, the shape `uf_bundle::embed` writes. */
function payload(contents: string): string {
  const dir = mkdtempSync(path.join(tmpdir(), "uf-compile-assets-"));
  const file = path.join(dir, "assets.js");
  writeFileSync(file, contents);
  return file;
}

describe("the payload uf's transform must never be asked about", () => {
  it("enters the graph under an id `isFlowModule` declines", () => {
    // The whole fix in one line. `isFlowModule` is what `packages/vite`'s
    // `transform` gates on, and its first rule is that a NUL-prefixed id is a
    // module the build tool synthesised rather than source anybody wrote — so
    // the payload goes from disk to the bundler without being parsed, whatever
    // it weighs.
    expect(COMPILE_ASSETS_RESOLVED_ID.startsWith("\0")).toBe(true);
    expect(isFlowModule(COMPILE_ASSETS_RESOLVED_ID)).toBe(false);
    // And the file it comes from would *not* have been declined, which is why
    // the id has to differ from the path. This is the assertion that fails if
    // somebody puts the path back into the entry's import.
    expect(isFlowModule("/app/.uf/build/compile/assets.js")).toBe(true);
  });

  it("is served from the file on disk, unchanged", () => {
    // Unchanged matters twice: the binary carries these exact bytes, and the
    // file stays openable by a person debugging a compiled binary that serves
    // the wrong thing. A plugin that re-encoded here would make those two
    // different artefacts.
    const source = 'export const assets = JSON.parse("{}");\n';
    const plugin = compileAssetsPlugin(payload(source));

    expect(plugin.resolveId(COMPILE_ASSETS_ID)).toBe(COMPILE_ASSETS_RESOLVED_ID);
    expect(plugin.load(COMPILE_ASSETS_RESOLVED_ID)).toBe(source);
  });

  it("claims nothing else", () => {
    // A plugin that answered for ids it does not own would shadow the router's
    // virtual modules, which are resolved by the same chain.
    const plugin = compileAssetsPlugin(payload("export const assets = {};\n"));

    expect(plugin.resolveId("virtual:uf/routes")).toBe(null);
    expect(plugin.resolveId("./assets.js")).toBe(null);
    expect(plugin.load("\0virtual:uf/server")).toBe(null);
  });

  it("is not read until the bundler asks for it", () => {
    // Eight megabytes of string, and a build that fails before the link step
    // never needs them. A plugin that read the file when it was constructed
    // would also throw for a build that has not written one yet.
    const plugin = compileAssetsPlugin(path.join(tmpdir(), "uf-no-such-assets.js"));

    expect(plugin.resolveId(COMPILE_ASSETS_ID)).toBe(COMPILE_ASSETS_RESOLVED_ID);
    expect(() => plugin.load(COMPILE_ASSETS_RESOLVED_ID)).toThrow();
  });
});
