// @flow
//
// The one place `@uniflowed/test` imports a module whose URL is only known at
// run time.
//
// A test runner exists to load files it discovers: the Node worker loads each
// test file by URL, with a query that makes a rerun a fresh evaluation; the
// browser page loads the file the runner asked for; and `uft.mock`,
// `uft.importActual` and `uft.importMock` load the real module behind a
// specifier. None of those can be a string literal, and Flow types `import()`
// of anything else as an error. Every one of them comes through
// `importModuleAt`, so the suppression that admits a computed specifier exists
// once, beside the reason for it. What it hands back is a namespace of
// `mixed` exports, never `any`.
//
// No imports of its own, because the browser page loads it too.

/** A module namespace: its exports by name, each unknown until checked. */
export type ModuleNamespace = { readonly [string]: mixed };

/**
 * The namespace of the module at `url`, an absolute URL (or a `/@fs/…` path
 * the page's server answers) that the runner itself decided to load.
 *
 * It rejects the way `import()` does, when the module cannot be loaded or throws
 * while it evaluates, and the runner reports that as the file failing to load.
 * It rejects with a `TypeError` in the impossible case that the host answers
 * with something other than an object.
 */
export function importModuleAt(url: string): Promise<ModuleNamespace> {
  // $FlowFixMe[unsupported-syntax] the one computed import() in this package; see the header.
  const loaded = import(url);
  return loaded.then((namespace: mixed) => {
    if (namespace == null || typeof namespace !== "object") {
      throw new TypeError(`@uniflowed/test: ${url} did not load as a module`);
    }
    return namespace;
  });
}
