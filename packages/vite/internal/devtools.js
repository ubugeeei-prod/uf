// @noflow
//
// Plain JavaScript: executed by the host that runs Vite, before any transform.
//
// React DevTools, in `uf dev`, on purpose.
//
// DevTools does not attach to React. React attaches to *DevTools*: while
// `react-dom` is being evaluated it looks for `__REACT_DEVTOOLS_GLOBAL_HOOK__`
// on the global object and registers itself with whatever it finds, once. A
// hook that arrives after that line has run is a hook no renderer ever sees, so
// everything below is about one ordering — the hook exists first — and about
// saying so in a file whose name a person can grep for.
//
// # Why this file exists at all, when it worked before it
//
// It did work, and by accident. The Fast Refresh preamble calls
// `injectIntoGlobalHook` (`./refresh-runtime.js`, Meta's runtime as vendored by
// `@vitejs/plugin-react`), and that function installs a hook when it finds
// none, because Fast Refresh needs one to decorate. So `uf dev` had a DevTools
// hook as a side effect of a function whose subject is hot reloading, in a file
// uf does not own, with nothing anywhere naming DevTools and no test that would
// notice its absence. The next upgrade of that vendored runtime, or a change to
// how the preamble is injected, could have taken it away in a diff nobody would
// read as being about DevTools. See ubugeeei-prod/uf#503.
//
// So the hook is installed here, first, deliberately, and `devtools.test.js`
// beside this package runs this script's own text against a fake window.
//
// # Three things DevTools needs, and what carries each
//
// Naming them together, because each is provided somewhere else and each is one
// edit away from being lost:
//
//   1. **The hook, before the renderer.** This module, injected by
//      `packages/vite/index.js`'s `transformIndexHtml` as a *classic* script at
//      the top of the head — see [`devtoolsPreamble`] for why classic.
//   2. **One copy of the renderer.** `resolve.dedupe: ["react", "react-dom"]`
//      in that same file. Two copies of `react-dom` register two renderers, and
//      DevTools shows the tree of whichever one it heard from — which is the
//      shape of the "multiple renderers concurrently rendering" report.
//   3. **The development build of it.** `mode` is `development`, so Vite
//      resolves React's development export condition, and `uf transform` is
//      called with `development: true` — which is what emits `jsxDEV` and the
//      `_jsxFileName` beside every element. Against a production build DevTools
//      says so and shows a tree with no props, no hooks and no source.
//
// # And out of a production build
//
// A production build injects none of this: `transformIndexHtml` returns an
// empty list unless the plugin is serving. That is the half a person can check
// on the artefact rather than by reading, and
// `crates/uf_cli/tests/vite.rs`'s `a_build_ships_no_devtools_hook` does.
//
// What it checks for is the *assignment* below rather than the name, and the
// distinction is worth stating here because the obvious test is wrong: React's
// own production build mentions `__REACT_DEVTOOLS_GLOBAL_HOOK__` twice, because
// reading that global is how a deployed React application is attachable at all.
// React reads it; only an installer writes it. So `window.<hook> =` is what
// must be absent, and it is absent because this function is never called
// outside a dev server.

/**
 * The global React registers itself with.
 *
 * Written once, here, so that every other mention of it in uf — the injected
 * script below, the assertion that a build has none — is this constant rather
 * than a fourth spelling of a name whose whole value is that it matches
 * React's exactly.
 */
export const DEVTOOLS_HOOK = "__REACT_DEVTOOLS_GLOBAL_HOOK__";

/**
 * The script every document loads before anything else in development.
 *
 * # A classic script, not a module
 *
 * Everything else uf injects is `type="module"`, and a module script is
 * deferred: it runs after the document has been parsed, in document order with
 * the other modules. That would still be early enough today, because the client
 * entry is also a module and comes later — but "early enough as long as nobody
 * adds a script above it" is exactly the accident this file exists to end. A
 * classic inline script runs while the parser is on it, so no module, no
 * import, and no `<script src>` a project's own Vite plugin injects can get
 * between this and the renderer.
 *
 * # It never replaces a hook that is already there
 *
 * The DevTools extension installs its hook at `document_start`, which is before
 * any script in the document, so on a machine that has DevTools the branch
 * below is not taken and the extension's hook is what React registers with.
 * Overwriting it would be the one way this file could break the thing it exists
 * to support: the extension holds the connection to the panel, and a stub in
 * its place is a page DevTools can see and never hear from.
 *
 * What is installed when there is nothing to leave alone is the minimum a
 * renderer will register with — `renderers`, `supportsFiber`, `inject` and the
 * three commit callbacks. It reports nothing to anybody: there is no panel, and
 * the point of installing it is that `react-dom` takes the branch where a hook
 * exists, so Fast Refresh has one to decorate and DevTools opened *later* in
 * the same page finds a renderer already registered rather than a page that has
 * to be reloaded. It is the same shape `injectIntoGlobalHook` installs, because
 * it is the same contract; the difference is that this is uf saying so.
 */
export function devtoolsPreamble() {
  return `(function () {
  if (window.${DEVTOOLS_HOOK} != null) return;
  var nextID = 0;
  window.${DEVTOOLS_HOOK} = {
    renderers: new Map(),
    supportsFiber: true,
    inject: function () { return nextID++; },
    onScheduleFiberRoot: function () {},
    onCommitFiberRoot: function () {},
    onCommitFiberUnmount: function () {},
  };
})();`;
}
