// @flow
//
// Internal to `@uniflowed/router`: which build this page is, and what it does
// when the server is on another one.
//
// A tab stays open across a deploy. The page in it was built by build N — its
// action ids, its chunk names, its copy of React — and after build N+1 goes
// live the server it talks to is N+1's. Three things can then go wrong, and
// this module is the browser's half of the answer to each:
//
// 1. **An action call.** N's action ids name nothing in N+1 (an id is an HMAC
//    over a per-build secret), so the call cannot be answered — and must not
//    be answered by guessing. The call names its build in the
//    [`DEPLOYMENT_HEADER`]; a server on another build answers `409` without
//    running anything; and the reference makes a hard navigation instead of
//    throwing, so the reader lands on N+1's page and presses the button again
//    on a page whose ids are right.
// 2. **A navigation.** N+1's payload names N+1's client chunks, and loading
//    them into N's page would put a second React beside the first. A payload
//    request names its build the same way and is refused the same way, and
//    `./flight-browser.js` already turns any answer that is not a payload into
//    a document load. A payload the build *prerendered* is a file, answered by
//    a host that runs nothing, so the payload also says which build rendered
//    it (`FlightRoot.deployment`), and the router loads the document instead
//    of rendering one from another build ([`fromAnotherDeployment`]).
// 3. **A chunk.** `uf build` keeps the previous build's hashed files in the
//    output directory for one more build, so a lazy chunk of N's is still
//    there while N+1 is live and the page keeps working. Past that window — or
//    on a host that did not keep them — the `import()` fails, and a
//    navigation whose module will not load becomes a document load of the
//    same URL rather than an error page ([`isChunkLoadFailure`]).
//
// The id is `<meta name="uf:deployment">` in the document's head, which
// `./shell.js` writes. `uf dev` writes none, and a page without one sends no
// header and compares nothing: there is no other build to be skewed against.

/** The request header a page names its build in, and a refusal names the server's in. */
export const DEPLOYMENT_HEADER: string = "uf-deployment";

/** The `<meta name>` the document carries the id under. */
export const DEPLOYMENT_META: string = "uf:deployment";

/**
 * What this page has read, once.
 *
 * `undefined` is "not read yet"; `null` is "read, and there is none". Read
 * before hydration by [`rememberDeployment`], because an application that owns
 * `<html>` hands its head to React, and the id is a fact about the document
 * that arrived rather than about whatever the head holds later.
 */
let remembered: string | null | void;

/** The parts of a `Document` read here. */
type HeadLike = interface {
  readonly querySelector: (selector: string) => ?interface {
    readonly getAttribute: (name: string) => ?string,
  },
};

/**
 * Read the page's build from its document and keep it.
 *
 * Called by both client entries before `hydrateRoot`, through
 * `./prepare-document.js`.
 */
export function rememberDeployment(document: ?HeadLike): void {
  const meta = document?.querySelector(`meta[name="${DEPLOYMENT_META}"]`);
  const content = meta?.getAttribute("content");
  remembered = content == null || content === "" ? null : content;
}

/** The build this page is, or `null` for a page that does not say. */
export function currentDeployment(): string | null {
  if (remembered === undefined) {
    rememberDeployment(typeof window === "undefined" ? null : window.document);
  }
  return remembered ?? null;
}

/** Forget what was read; for a test that builds a second page in one process. */
export function forgetDeployment(): void {
  remembered = undefined;
}

/** Name this page's build on an outgoing request's headers, when it has one. */
export function withDeployment(headers: { [string]: string }): { [string]: string } {
  const deployment = currentDeployment();
  if (deployment != null) {
    headers[DEPLOYMENT_HEADER] = deployment;
  }
  return headers;
}

/**
 * Whether `response` is a server on another build refusing this page.
 *
 * The status and the header together: a `409` alone is an application's own
 * answer to something, and only a front door refusing a build names the build
 * it *is* on.
 */
export function refusedAsAnotherDeployment(response: Response): boolean {
  if (response.status !== 409) return false;
  const serving = response.headers.get(DEPLOYMENT_HEADER);
  return serving != null && serving !== currentDeployment();
}

/**
 * Whether a payload was rendered by another build than this page's.
 *
 * Only when both say: a payload from a server with no id (`uf dev`) and a page
 * with none are the same build as far as anyone can tell.
 */
export function fromAnotherDeployment(rendered: ?string): boolean {
  const current = currentDeployment();
  return current != null && rendered != null && rendered !== "" && rendered !== current;
}

/**
 * Whether `error` is a module that could not be fetched, rather than one that
 * threw while it ran.
 *
 * An `import()` of a file the server no longer has rejects with a `TypeError`
 * whose message is the browser's own and differs per engine; these are the
 * three that ship, plus Vite's own message for a stylesheet its preload could
 * not load. A module that threw while evaluating rejects with whatever it
 * threw, and is an error for the route's boundary to show — reloading the page
 * would only throw it again.
 */
export function isChunkLoadFailure(error: mixed): boolean {
  if (!(error instanceof Error)) return false;
  const message = error.message;
  return (
    message.includes("Failed to fetch dynamically imported module") ||
    message.includes("error loading dynamically imported module") ||
    message.includes("Importing a module script failed") ||
    message.includes("Unable to preload CSS")
  );
}

/**
 * Leave this page for `href` by loading its document.
 *
 * `assign` rather than the history API, because the point is to stop running
 * this build's code: the document that arrives is the current build's, with
 * the current build's scripts.
 */
export function loadDocument(href: string): void {
  window.location.assign(href);
}
