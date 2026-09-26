// @flow
//
// `node:module`, as `uf test --browser` has it.
//
// `../render.js` loads `react-dom/client` through `createRequire` because
// `render` is synchronous — a test asserts on the line after it — and an
// `import` is not. That is right on Node and impossible in a page: a browser
// resolves modules asynchronously and has no way to be asked to do it in the
// middle of a call.
//
// So the asynchronous import happens *here*, at module scope, before any test
// file has run — the one moment in a browser run when being asynchronous costs
// nothing, because nothing is waiting yet. `createRequire` then answers from
// what this module already holds, synchronously, and `render.js` does not
// change.
//
// Reached through `package.json`'s `browser` field, which Node ignores and
// every bundler honours, so nothing about a Node worker changes. The same
// mechanism, for the same reason, as `@uniflowed/test`'s `internal/browser/`
// and `@uniflowed/host`'s `internal/browser-module.js`.
//
// # Why `react-dom/client` and nothing else
//
// It is the one specifier this package requires, and a `require` that answered
// for a list of guesses would be a list to keep in step with a file that never
// asked for it. Anything else throws a sentence naming what is not available,
// which is the rule the rest of uf's browser shims follow: a boundary should
// be a sentence, not a surprise in a stack trace.

import * as client from "react-dom/client";

/**
 * Modules this page brought with it, by the specifier they answer to.
 *
 * `default` first, then the namespace. React ships CommonJS, and what a
 * CommonJS module *is* — the object `module.exports` ended up as — is its
 * default export; the named exports beside it are a convenience read off that
 * object. `createRequire` is being asked for what `require` would have
 * returned, which is the first of those two.
 */
const provided: Map<string, mixed> = new Map([["react-dom/client", client.default ?? client]]);

/**
 * A `require` that answers for what the page already imported.
 *
 * Refusing loudly rather than returning `undefined`: a caller that gets
 * `undefined` back fails later, somewhere else, with a message about a
 * property of `undefined`.
 */
export function createRequire(_from: mixed): (specifier: string) => mixed {
  return (specifier: string) => {
    const module = provided.get(specifier);
    if (module !== undefined) {
      return module;
    }
    throw new Error(
      `\`require("${specifier}")\` is not available in \`uf test --browser\`: a browser resolves modules asynchronously and cannot be asked to do it in the middle of a call. Import it from the test file instead, or run the file on Node.`,
    );
  };
}
