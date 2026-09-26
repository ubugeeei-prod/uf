// @noflow
//
// Plain JavaScript: executed by the host that runs Vite, before any transform.
//
// The embedded copy of `dist/`, handed to the bundler as a virtual module.
//
// `uf build --compile` puts everything `uf build` wrote inside the binary, and
// the way it travels there is one generated module: `uf_bundle::embed` writes
// `export const assets = JSON.parse("…")` into `.uf/build/compile/assets.js`,
// where the string is base64 of every file in the output directory. For the
// docs site that is eight megabytes; for a project with images it is tens.
//
// # Why it does not enter the graph as the file it is
//
// Because it is uf's own data, and the module graph is where uf's *source*
// transform is. `crates/uf_transform` refuses a source over
// `MAX_SOURCE_BYTES` — 8 MiB, a ceiling that bounds the parser against a file
// nobody meant to compile — and `.uf/build/compile/assets.js` is a file
// nobody wrote at all: its size is the size of the *build output*, which has
// no relationship to the size of any source file and grows every time a page
// is added. A site that crossed the line failed with
//
//     TransformError: source is 8442536 bytes, over the 8388608 byte ceiling
//       [plugin uf:flow] docs/.uf/build/compile/assets.js
//
// which names a file the project does not own, for a limit it did not break,
// at the end of a build that had already succeeded.
//
// So the payload enters the graph the way a build tool's own modules do,
// under a NUL-prefixed id. `@uniflowed/host/transform`'s `isFlowModule`
// declines those by name — "a build tool synthesises modules of its own", the
// first sentence of its contract — so the bytes go from disk to the bundler
// without being parsed as Flow on the way. Raising the ceiling instead would
// have weakened the one thing it is for, on every real source file, to make
// room for a file that is not source.
//
// The file on disk is still written and still what is served, so a person
// debugging a compiled binary can open the thing that went into it. What
// changed is only which name the bundler reaches it by.

import { readFileSync } from "node:fs";

/** What the generated entry imports. */
export const COMPILE_ASSETS_ID = "virtual:uf/compile-assets";

/** The resolved id Vite hands back for it. */
export const COMPILE_ASSETS_RESOLVED_ID = `\0${COMPILE_ASSETS_ID}`;

/**
 * Serve `file` — `uf_bundle::embed`'s output — as [`COMPILE_ASSETS_ID`].
 *
 * Read at `load` rather than when the plugin is made, so a build that fails
 * before it reaches the entry never pays for eight megabytes of string, and
 * so the bytes are the ones on disk at the moment the bundler asked for them
 * rather than at the moment the plugin list was assembled.
 */
export function compileAssetsPlugin(file) {
  return {
    name: "uf:compile-assets",
    resolveId(id) {
      return id === COMPILE_ASSETS_ID ? COMPILE_ASSETS_RESOLVED_ID : null;
    },
    load(id) {
      return id === COMPILE_ASSETS_RESOLVED_ID ? readFileSync(file, "utf8") : null;
    },
  };
}
