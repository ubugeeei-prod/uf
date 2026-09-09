// @flow
//
// `happy-dom`, as a page has it: not at all, and it does not need it.
//
// `../dom.js` imports `Window` at the top and constructs one only when the
// host has no `document`. In `uf test --browser` the host is a browser, so
// `installDom` returns the page's own window before it reaches the
// constructor — but ESM evaluates every import before the module body, so the
// import still has to resolve to something, and `happy-dom` is a Node package
// that would fail at link time in a page.
//
// This is that something. It exists to be resolved, and the constructor throws
// rather than returning a plausible object, because a `Window` handed to
// `../dom.js` in a browser would mean uf had installed a *second* document
// over a real one — every query would run against a DOM the page cannot see,
// and every assertion would be about nothing.
//
// Mapped in by `package.json`'s `browser` field, which Node ignores and every
// bundler honours. The same shape as `@uniflowed/host`'s
// `internal/browser-module.js`, for the same reason.

/** happy-dom's window, which a page has no use for and cannot build. */
export class Window {
  constructor(_options?: mixed) {
    throw new Error(
      "@uniflowed/react-testing: happy-dom is not available in `uf test --browser`, and is not wanted — the page's own document is the one tests render into. Reaching here means `installDom` did not find a `document`, which a browser always has.",
    );
  }
}
