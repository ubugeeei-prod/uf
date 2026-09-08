// @noflow
//
// Plain JavaScript: a stand-in, and there is nothing in it to type.
//
// `node:module`, as a browser bundle can have it.
//
// `../module-mocks.js` is reachable from `@uniflowed/test`, which a module
// with an in-source test block imports. The block itself is compiled away by
// `uf build` (`import.meta.uf.test` becomes `void 0`) and the import is shaken
// out after it — but the bundler resolves the graph before it shakes, and a
// `node:module` it cannot resolve is a warning on every build of every project
// that writes an in-source test.
//
// So this file exists to be resolved and never called. `registerHooks` is the
// only thing `../module-mocks.js` reaches for, and reaching for it here would
// mean a browser bundle was about to intercept module resolution, which no
// browser can do — hence the throw rather than a silent no-op.
//
// Mapped in by `package.json`'s `browser` field, which Node ignores and every
// bundler honours, so the host's own behaviour is untouched.

/** Node's synchronous module hooks, which a page does not have. */
export function registerHooks() {
  throw new Error(
    "module interception is not available in a browser: `node:module`'s hooks run in the thread doing the importing, and a page has no such thing",
  );
}
