// @flow
//
// `@uniflowed/hmr` — the way a diagnostic the browser produced reaches the
// terminal.
//
// Every other uf diagnostic — a type error, a lint finding, a failing test, a
// page that rendered its error boundary — arrives in the terminal the
// developer already has open. One produced *in the browser* had nowhere to go:
// the page is the only process that knows about it, and `uf dev`'s diagnostics
// come up the driver's event channel from the Node process. So a browser-only
// one had to be noticed, in a window that may not be in front, by somebody who
// did not know to look. See ubugeeei-prod/uf#583.
//
// This is the client half. The server half is `uf dev`, through
// `@uniflowed/vite`'s `internal/diagnostics.js`, which turns what arrives into
// the same `error`/`warning` rendering the terminal gives everything else.
//
// # One channel
//
// Deliberately one, rather than one per feature. `@uniflowed/web/vitals` posts
// the five numbers it measures to `/__uf/vitals`, and the same dev-server code
// turns those into the same kind of diagnostic — one contract, one endpoint to
// serve, one place that decides how a browser's report reads in a terminal.
//
// The first caller of *this* endpoint is the hydration-mismatch report in
// `@uniflowed/router`, which is ubugeeei-prod/uf#582 and is not merged yet.
// That is why this exists before it has a caller in the tree: the report is a
// browser diagnostic, and the whole of #583 is that a browser diagnostic
// should not arrive by a route of its own.
//
// # It is development only, and it is best effort
//
// `uf dev` serves this path and nothing else does: a built application has no
// `/__uf/` anything, so a call in production posts to a path that answers 404
// and the rejected promise is swallowed here. That is the intended behaviour
// rather than a hazard — but a caller should still gate on `import.meta.hot`,
// so that a production bundle has no path to this module rather than merely no
// answer from it.
//
// Nothing here opens a connection until it is called, importing it does
// nothing at all, and what it sends goes to the page's own origin. There is no
// destination to configure, no third party, and nothing leaves the machine.

/**
 * The path `uf dev` serves the diagnostic channel on.
 *
 * Under `/__uf/`, beside the update stream this package's client opens and the
 * vitals endpoint `@uniflowed/web/vitals` posts to, for the reason that prefix
 * exists: a directory in `app/` whose name begins with `_` is not a route, so
 * no application can put anything here and nothing here can shadow a path a
 * project wrote.
 */
export const DIAGNOSTIC_ENDPOINT: string = "/__uf/diagnostic";

/**
 * How loudly a diagnostic reads in the terminal.
 *
 * `error` for something that is wrong, `warn` for something that is worth
 * knowing, `info` for something that is only a measurement. Three rather than
 * two because the vitals report on the same channel needs the third: a page
 * whose numbers are all good has still reported, and printing that as a
 * warning would teach the reader to ignore the warnings.
 */
export type DiagnosticSeverity = "error" | "warn" | "info";

/**
 * One diagnostic, as the channel carries it.
 *
 * `message` is the headline and the only required field: one line, the thing
 * that is wrong. `detail` is everything under it — the values that differed,
 * the path through the tree, the advice — and is a list of lines rather than a
 * blob so the terminal can indent them without guessing where they break.
 *
 * `url` is the page the browser was on, which the reporter fills in when the
 * caller does not: a diagnostic that does not say which page produced it is a
 * diagnostic somebody has to reproduce before they can act on it.
 *
 * `file`, `line` and `column` are for the rare browser-side report that knows
 * a source position. When all three are present `uf dev` draws its ordinary
 * code frame; when they are not it prints the headline and the detail, which
 * is what a hydration mismatch — a fact about a DOM node rather than a line —
 * can honestly offer.
 */
export type BrowserDiagnostic = {
  readonly severity: DiagnosticSeverity,
  readonly message: string,
  readonly detail?: $ReadOnlyArray<string>,
  readonly url?: string,
  readonly file?: string,
  readonly line?: number,
  readonly column?: number,
};

/** The part of the browser this module needs. */
type ReportingWindow = {
  readonly location?: { readonly href?: string, ... },
  readonly fetch?: (input: string, init: { ... }) => Promise<mixed>,
  ...
};

/** For a call made where there is no browser to report from. */
function noop(): void {}

/**
 * Send one diagnostic to `uf dev`, if there is a `uf dev` to send it to.
 *
 * Returns nothing and throws nothing. A diagnostic is a thing a person reads,
 * not a thing an application branches on, and a reporter that could fail would
 * make every caller wrap it — from a code path that is, by construction,
 * already handling something that went wrong.
 *
 * `fetch` rather than `sendBeacon`, which is the opposite of the choice
 * `vitalsBeacon` makes and for the opposite reason: a vital is measured as the
 * page is put away and needs a transport that outlives it, while a diagnostic
 * is produced by a page that is still running and had better arrive in the
 * order it happened. `keepalive` covers the case where the page goes away
 * immediately afterwards anyway.
 *
 * @param diagnostic what to report
 * @param endpoint where to post it; [`DIAGNOSTIC_ENDPOINT`] by default
 */
export function reportDiagnostic(diagnostic: BrowserDiagnostic, endpoint?: string): void {
  const win = reportingWindow();
  if (win == null) {
    return;
  }
  const post = win.fetch;
  if (post == null) {
    return;
  }

  const body = JSON.stringify({
    ...diagnostic,
    url: diagnostic.url ?? win.location?.href ?? "",
  });
  // A rejected promise nobody is holding becomes an unhandled rejection, which
  // arrives in whatever error reporter the application installed — from a line
  // about development tooling. Losing the report is the right outcome when
  // there is nothing listening; reporting the loss as an application error is
  // not.
  post(endpoint ?? DIAGNOSTIC_ENDPOINT, {
    method: "POST",
    body,
    keepalive: true,
    headers: { "content-type": "application/json" },
  }).then(noop, noop);
}

/**
 * The window to report from, or `null` where there is no browser.
 *
 * The same reading `@uniflowed/web/vitals` makes, and for the same reason: in
 * a browser `globalThis` *is* the window, but not where a document has been
 * installed onto another host's global, which is every uf test process. Asking
 * for the document first is what makes both cases work.
 */
function reportingWindow(): ReportingWindow | null {
  if (typeof globalThis.document === "undefined") {
    return null;
  }
  return globalThis.window ?? globalThis;
}
