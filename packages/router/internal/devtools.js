// @flow
//
// Internal to `@uniflowed/router`: whether React DevTools can actually attach
// to the page this browser just hydrated.
//
// `@uniflowed/vite` installs the hook DevTools attaches through, as a classic
// script above every module — see `packages/vite/internal/devtools.js`, which
// has the argument. That is uf saying what it intends. This is the half that
// checks it happened, and the two are not the same claim: the preamble is
// injected by a `transformIndexHtml` hook, into a document a project's own Vite
// plugins also write to, and the thing that has to be true is an *ordering* —
// the hook exists before `react-dom` is evaluated — which no amount of reading
// the injector can establish about a particular page.
//
// It runs once, after hydration, in development only, and says nothing at all
// when there is nothing wrong. See ubugeeei-prod/uf#503.
//
// # Why it reports rather than throws
//
// Nothing here is a reason to stop a page. DevTools not attaching costs a
// developer a panel, and a framework that refused to render over it would have
// turned a missing convenience into an outage. So the two findings go to the
// terminal on the same channel as every other browser-side diagnostic — see
// `./diagnostics.js` — and the page carries on.
//
// # The two findings, and why they are the two
//
// A third condition — React's *development* build, which is what gives the
// panel props, hooks and source positions — is not checked here because a
// browser cannot tell the difference from the outside, and because uf owns it
// end to end: `mode` is `development` and `uf transform` is called with
// `development: true`, both asserted in `tests/library/devtools.test.js`
// against the plugin rather than against a page. What is left is what only a
// running page knows.
//
//   1. **There is no hook.** Something ran before `react-dom` and there was
//      nothing for it to register with, or the preamble did not reach this
//      document. Either way no renderer was announced and the panel will say
//      the page is not using React.
//   2. **There is more than one renderer.** Two copies of `react-dom` each
//      injected, and DevTools shows the tree of whichever it heard from — which
//      is the shape of the "multiple renderers concurrently rendering the same
//      context provider" report, and a component tree that is missing half the
//      page for a reason nothing on screen explains.

import { reportDiagnostic } from "./diagnostics.js";

/**
 * The global React registers itself with.
 *
 * The same string as `DEVTOOLS_HOOK` in `@uniflowed/vite`'s
 * `internal/devtools.js`, written out again rather than imported for the reason
 * that file's neighbour `internal/diagnostics.js` gives about the endpoint
 * paths: `@uniflowed/vite` is loaded by Vite before any Flow transform exists
 * and this module is Flow, so the import cannot go either way.
 * `tests/library/devtools.test.js` asserts the two spellings agree, which is
 * what makes a duplicated constant honest.
 */
export const DEVTOOLS_HOOK: string = "__REACT_DEVTOOLS_GLOBAL_HOOK__";

/** The part of a page this module reads. */
type HookWindow = {
  [key: string]: mixed,
  ...
};

/**
 * What is wrong with this page's DevTools hook, as a diagnostic, or `null`.
 *
 * Separated from the reporting so that a test can ask the question without a
 * channel to answer on, and because the wording is the part worth pinning: a
 * reader who sees this in a terminal has to be able to act on it without
 * reading this file.
 */
export function devtoolsProblem(win: HookWindow): {|
  readonly message: string,
  readonly detail: $ReadOnlyArray<string>,
|} | null {
  const hook = win[DEVTOOLS_HOOK];
  if (hook == null || typeof hook !== "object") {
    return {
      message: `React DevTools cannot attach: nothing installed \`${DEVTOOLS_HOOK}\` before react-dom ran`,
      detail: [
        "React registers itself with that global while `react-dom` is evaluated, once and never again.",
        "`uf dev` injects the hook as a classic script at the top of the head; a plugin that replaces",
        "`transformIndexHtml`'s output, or a document that does not go through it, takes it away.",
      ],
    };
  }

  // `renderers` is a Map React puts its renderer in, keyed by the id `inject`
  // handed back. Anything else there is a hook uf did not install and DevTools
  // did not either, and guessing at its shape would report a problem that is
  // really this module not recognising one.
  const renderers = (hook: $FlowFixMe).renderers;
  const count = renderers instanceof Map ? renderers.size : null;
  if (count != null && count > 1) {
    return {
      message: `React DevTools has ${count} renderers on this page and will show one of them`,
      detail: [
        "Two copies of `react-dom` are loaded, so half the component tree is in a tree the panel cannot see.",
        '`resolve.dedupe: ["react", "react-dom"]` is what usually prevents it; a linked package with its',
        "own `react-dom` in `node_modules` is what usually causes it.",
      ],
    };
  }
  return null;
}

/**
 * Report the problem, if there is one, to the terminal running `uf dev`.
 *
 * Throws nothing and returns nothing, and the guard is around the whole body
 * rather than around the reading: this is called from the line after a
 * successful hydration, so every failure available to it — a hook object whose
 * property getter throws, a host with no `fetch` to report through — is a
 * development convenience failing, and a development convenience that can take
 * a working page down is worse than no convenience at all.
 */
export function reportDevtools(win: HookWindow): void {
  try {
    const problem = devtoolsProblem(win);
    if (problem == null) {
      return;
    }
    reportDiagnostic({ severity: "warn", message: problem.message, detail: problem.detail });
  } catch {
    // Deliberately silent. There is no second channel to complain on, and the
    // thing being reported was never worth interrupting anybody for.
  }
}
