// @flow
//
// Internal to `@uniflowed/router`: which DOM subtree each boundary owns.
//
// An application has three boundaries a reader cannot see — Suspense, error,
// and client/server — and all three are decisions the build already made.
// ubugeeei-prod/uf#636 answered the third: `uf dev` says why a module is in the
// client bundle, out loud, at the moment the answer changes. This is the other
// two, and the gap it fills was named in that issue's triage: *the route table
// already has the data*. `$loading.js` nests and carries the number of
// layouts outside it, `$error.js` binds to the nearest ancestor, and
// `RouteView` threads both into one stack. What did not exist is a way to point
// at the **DOM subtree** each of them owns.
//
// That cannot be read off the table, and it cannot be read off the page either.
// A `<Suspense>` renders no element of its own; nor does a class boundary. What
// is on the page is a run of nodes, in a parent that also holds whatever the
// layout above put beside them — a `<nav>` before, a `<footer>` after — and
// nothing distinguishes the run from its neighbours. So the render has to say
// so, which is what this module is.
//
// # The mechanism: a pair of inert marks, and why a pair
//
// Each boundary `RouteView` renders wraps its children between two
// `<span hidden data-uf-boundary>` elements. The `hidden` attribute keeps the
// marks out of layout and accessibility trees; the pair are siblings of the nodes between them,
// because React fragments create no element, so "what this boundary owns" is
// `open.nextElementSibling` up to `close`.
//
// One mark would have been cheaper and would have been wrong. A boundary's run
// ends where the enclosing layout's own trailing nodes begin, and from the
// opening mark alone those are indistinguishable — the walk would hand a
// boundary the footer underneath it. Two marks are the smallest thing that
// closes.
//
// A wrapper element was the other candidate: one node instead of two, and
// `wrapper.children` with no walk at all. It loses on the thing that matters
// here — a wrapper has to exist from the first render, because introducing one
// later moves the subtree into a new parent and React answers that by
// unmounting and rebuilding everything under it. Marks are siblings, so they
// can arrive after the page has settled, which is what the section below is
// about.
//
// # They arrive after hydration, which is what makes them free of it
//
// Every edge renders `null` until it has mounted. So the tree React hydrates
// against the server's markup contains no mark, the server's markup contains no
// mark, and the two agree whatever either side believed about being in
// development — the gate is allowed to answer differently in the two processes
// because by the time it has any effect, hydration is over. `reportDevtools`
// asks its question on the line after hydration for the same reason: a
// development affordance that can turn a working page into a mismatch is worse
// than no affordance.
//
// `marksAreLive` is what keeps that from costing a second commit forever. It is
// latched by the first edge to mount, and every edge mounted afterwards — a
// navigation, or a `$loading.js` that HMR has just added — starts live and
// is in the DOM in the same commit that created it. That matters for the report
// below, which reads the DOM in the commit where the boundaries changed.
//
// # Where the report goes
//
// The terminal, on `./diagnostics.js`, which is the channel #583 established
// and #636 argued for again: a diagnostic that exists only in a browser window
// has to be noticed by somebody who does not know to look. And it is quiet
// unless the answer *changed* — the first sighting of a route says nothing, the
// same boundaries on the same route say nothing, and adding an `$error.js`
// says where it landed and what it took over. A boundary map printed on every
// reload is the banner nobody reads.
//
// The marks themselves are the other half of "see it", and the cheaper half:
// they are in the document, so the element inspector already shows where each
// boundary opens and closes with no panel to open, and
// `document.querySelectorAll("[data-uf-boundary]")` is the whole API. For the
// page you are looking at right now there is `__ufBoundaries()`, which prints
// the same report on demand.
//
// # None of it is in a build
//
// Nothing here is reachable from a production bundle: `runtime.js` guards every
// reference with `BOUNDARY_MARKS`, which is `import.meta.hot != null` — the
// gate `client.js` already uses, replaced by `undefined` in a build — and this
// package is `sideEffects: false`, so with the references folded away the
// module is dropped rather than merely unused.

import * as React from "react";
import { useEffect, useState } from "react";

import { reportDiagnostic } from "./diagnostics.js";

/** The attribute a mark carries its boundary's id in. */
export const BOUNDARY_ATTRIBUTE: string = "data-uf-boundary";

/** The attribute telling the two marks of one boundary apart. */
export const EDGE_ATTRIBUTE: string = "data-uf-boundary-edge";

/** The attribute naming the file a boundary was declared in, when one is known. */
export const SOURCE_ATTRIBUTE: string = "data-uf-boundary-source";

/** The name `uf dev` installs the on-demand report under. */
export const BOUNDARY_GLOBAL: string = "__ufBoundaries";

/**
 * What `file` says for the error boundary the build synthesises.
 *
 * `routesModuleSource` writes this string where a declared boundary has a path,
 * because the record has no module to name — the framework's own error page
 * renders in its place. Written out again rather than imported for the reason
 * `./devtools.js` gives about `DEVTOOLS_HOOK`: `@uniflowed/vite` is plain
 * JavaScript loaded by Vite before any Flow transform exists, so the import
 * cannot go either way. `boundaries.test.js` holds the two spellings together.
 */
export const SYNTHESISED_SOURCE: string = "@uniflowed/router";

/** Which kind of boundary a mark belongs to. */
export type BoundaryKind = "suspense" | "error";

/**
 * One boundary of a resolved route, as the marks and the report name it.
 *
 * `id` pairs the two marks and is stable for a boundary across renders, so a
 * navigation that keeps a boundary keeps its marks mounted. `above` is how many
 * of the route's layouts are outside it — the same number, spelled the same
 * way, that `ResolvedRoute["errorBoundary"].above` and every `loading` entry
 * carry, because there is no second vocabulary for where a thing sits in the
 * stack.
 *
 * `source` is the file it was declared in, and is `null` for every `<Suspense>`
 * boundary. That asymmetry is the route table's rather than this module's: an
 * error boundary is matched by path, so the table carries its `file`, while a
 * `$loading.js` is carried by depth alone. Adding a path to the loading
 * records would put one in every visitor's bundle to serve a report only
 * `uf dev` reads.
 */
export type RouteBoundary = {|
  readonly id: string,
  readonly kind: BoundaryKind,
  readonly above: number,
  readonly source: ?string,
|};

/** The id of the `<Suspense>` boundary at `index` of a route's `loading`. */
export function suspenseId(index: number): string {
  return `suspense:${index}`;
}

/** The id of the boundary a route's own `$error.js` renders. */
export const ROUTE_ERROR_ID: string = "error:route";

/**
 * The id of the boundary that stands outside every layout.
 *
 * It has no module and renders the framework's page; it is what is between a
 * throw in a root layout, or in the error component itself, and an unmounted
 * document. Marked like any other, because "which subtree does the last resort
 * own" is exactly as unanswerable from the page as the rest.
 */
export const ROOT_ERROR_ID: string = "error:root";

/** The part of a resolved route this module reads. */
type BoundedRoute = {
  readonly errorBoundary: { readonly above: number, ... },
  readonly loading: $ReadOnlyArray<{ readonly above: number, ... }>,
  readonly error: mixed,
  ...
};

/**
 * Every boundary a resolved route renders, in the order they nest.
 *
 * A `Map` rather than a list because it is read both ways: `RouteView` asks for
 * one by id as it builds the stack, and the report walks the values. One
 * function answering both is the point — an id `RouteView` marks and the report
 * cannot find is a boundary that silently disappears from the map, and
 * ubugeeei-prod/uf#636 made the same argument about an explanation that can
 * disagree with the thing it explains.
 *
 * The route's own error boundary is absent when the route *is* its error page,
 * which is exactly when `RouteView` does not render one: wrapping that page in
 * the boundary whose component it is would answer a throw inside it with
 * itself.
 *
 * @param resolved the route being rendered
 * @param errorSource the `file` of the nearest `$error.js`, when the table
 *   has one; `null` leaves the boundary named by its depth alone
 */
export function routeBoundaries(
  resolved: BoundedRoute,
  errorSource: ?string,
): Map<string, RouteBoundary> {
  const found: Map<string, RouteBoundary> = new Map();
  found.set(ROOT_ERROR_ID, {
    id: ROOT_ERROR_ID,
    kind: "error",
    above: 0,
    source: SYNTHESISED_SOURCE,
  });
  if (resolved.error == null) {
    found.set(ROUTE_ERROR_ID, {
      id: ROUTE_ERROR_ID,
      kind: "error",
      above: resolved.errorBoundary.above,
      source: errorSource,
    });
  }
  resolved.loading.forEach((boundary, index) => {
    const id = suspenseId(index);
    found.set(id, { id, kind: "suspense", above: boundary.above, source: null });
  });
  return found;
}

/**
 * Whether an edge that mounts now should be in the DOM immediately.
 *
 * Latched by the first edge to mount and never cleared. Read through
 * `useState`'s initialiser rather than during the render body, which is the
 * difference between "this component's first state" and "a module variable a
 * memoising compiler is entitled to hold on to".
 */
let marksAreLive = false;

/**
 * One end of one boundary.
 *
 * A hidden element and not a comment node, because React renders elements.
 * This used to be a `<template>`, but React 19.3 reports template insertion
 * during document-root hydration as a browser error. A `span hidden` carries
 * the same marker data without entering layout or the accessibility tree.
 */
component BoundaryEdge(boundary: RouteBoundary, edge: "open" | "close") {
  const [live, setLive] = useState<boolean>(() => marksAreLive);
  useEffect(() => {
    marksAreLive = true;
    setLive(true);
  }, []);
  if (!live) {
    return null;
  }
  if (edge === "close") {
    return <span hidden data-uf-boundary={boundary.id} data-uf-boundary-edge="close" />;
  }
  return (
    <span
      hidden
      data-uf-boundary={boundary.id}
      data-uf-boundary-edge="open"
      data-uf-boundary-source={boundary.source ?? undefined}
    />
  );
}

/**
 * `children`, between the two marks of `boundary`.
 *
 * `children` unchanged when there is no boundary to mark, so a caller never has
 * to ask twice. The marks are the first and last children of a fragment rather
 * than a wrapper's, so the nodes between them are siblings of them, and every
 * position in the fragment is fixed — an edge going from `null` to a hidden
 * mark after mount is an insertion beside `children` and not around it,
 * which is why it costs no remount.
 */
export function insideBoundary(boundary: ?RouteBoundary, children: React.Node): React.Node {
  if (boundary == null) {
    return children;
  }
  return (
    <>
      <BoundaryEdge boundary={boundary} edge="open" />
      {children}
      <BoundaryEdge boundary={boundary} edge="close" />
    </>
  );
}

/**
 * How far a walk between two marks will go before giving up.
 *
 * A closing mark is a sibling of its opening one and the run between them is a
 * route's rendered output, so this is never reached by a page that is behaving.
 * It is here because `docs/security.md` asks that a report have no unbounded
 * anything in it, and because a DOM somebody else's script has been editing is
 * exactly where an unbounded walk would be found.
 */
const WALK_LIMIT = 512;

/** How many owned elements one line of the report names before it counts them. */
const NAMED_LIMIT = 3;

/** One boundary, as the page has it. */
export type BoundaryFinding = {|
  readonly boundary: RouteBoundary,
  /** The top-level elements between its marks, as short selectors. */
  readonly owns: $ReadOnlyArray<string>,
  /** How many more there were than [`NAMED_LIMIT`]. */
  readonly more: number,
  /** False when the boundary rendered no marks — it is showing its fallback. */
  readonly rendered: boolean,
|};

/**
 * What each boundary owns on the page right now.
 *
 * Takes the document rather than reaching for a global, so a test can build one
 * and ask — the same shape `hydrationReport` has, and for the same reason: the
 * analysis worth checking is the one that runs in a browser, so the test has to
 * be able to call exactly it.
 *
 * A boundary with no marks in the document is not missing, it is *suspended*:
 * React removes a boundary's content while its fallback is up, and its marks
 * are part of that content. Saying so is more useful than leaving it out.
 */
export function boundaryFindings(
  boundaries: Map<string, RouteBoundary>,
  document: Document,
): $ReadOnlyArray<BoundaryFinding> {
  const findings: Array<BoundaryFinding> = [];
  for (const boundary of boundaries.values()) {
    const open = document.querySelector(
      `[${BOUNDARY_ATTRIBUTE}="${boundary.id}"][${EDGE_ATTRIBUTE}="open"]`,
    );
    if (open == null) {
      findings.push({ boundary, owns: [], more: 0, rendered: false });
      continue;
    }
    const owned: Array<string> = [];
    let steps = 0;
    let node = open.nextElementSibling;
    while (node != null && steps < WALK_LIMIT && !closes(node, boundary.id)) {
      if (node.getAttribute(BOUNDARY_ATTRIBUTE) == null) {
        owned.push(describeElement(node));
      }
      node = node.nextElementSibling;
      steps += 1;
    }
    findings.push({
      boundary,
      owns: owned.slice(0, NAMED_LIMIT),
      more: Math.max(0, owned.length - NAMED_LIMIT),
      rendered: true,
    });
  }
  return findings;
}

/** Whether `element` is the closing mark of `id`. */
function closes(element: Element, id: string): boolean {
  return (
    element.getAttribute(BOUNDARY_ATTRIBUTE) === id &&
    element.getAttribute(EDGE_ATTRIBUTE) === "close"
  );
}

/**
 * One element, short enough to sit in a line of a report.
 *
 * A CSS selector rather than a tag name, because a page has eleven `<div>`s and
 * the one being named has to be findable: the id if it has one, and otherwise
 * the first class, which is what a person would type into the inspector's
 * search box. Nothing more — the report says which subtree, and the page says
 * what is in it.
 */
export function describeElement(element: Element): string {
  const tag = element.tagName.toLowerCase();
  const id = element.getAttribute("id");
  if (id != null && id !== "") {
    return `${tag}#${id}`;
  }
  const className = element.getAttribute("class");
  const first = className == null ? "" : className.trim().split(/\s+/)[0];
  return first === "" ? tag : `${tag}.${first}`;
}

/**
 * The report, as the terminal will print it.
 *
 * Separated from the sending so that a test can pin the wording, which is the
 * part worth pinning: somebody reading this in a terminal has to be able to act
 * on it without opening this file.
 */
export function formatBoundaries(
  path: string,
  findings: $ReadOnlyArray<BoundaryFinding>,
): {| readonly message: string, readonly detail: $ReadOnlyArray<string> |} {
  const count = findings.length;
  return {
    message: `${count} ${count === 1 ? "boundary renders" : "boundaries render"} ${path}`,
    detail: findings.map(describeFinding),
  };
}

/** One boundary as one line: what it is, where it sits, and what it owns. */
function describeFinding(finding: BoundaryFinding): string {
  const { boundary } = finding;
  const kind = boundary.kind === "error" ? "error" : "suspense";
  const source =
    boundary.source == null
      ? ""
      : boundary.source === SYNTHESISED_SOURCE
        ? " (uf's own error page)"
        : ` (${boundary.source})`;
  const where =
    boundary.above === 0
      ? "outside every layout"
      : `inside ${boundary.above} ${boundary.above === 1 ? "layout" : "layouts"}`;
  if (!finding.rendered) {
    return `${kind}${source}, ${where} — showing its fallback`;
  }
  if (finding.owns.length === 0) {
    return `${kind}${source}, ${where} — owns no element of its own`;
  }
  const named = finding.owns.join(", ");
  const more = finding.more === 0 ? "" : ` and ${finding.more} more`;
  return `${kind}${source}, ${where} — owns ${named}${more}`;
}

/**
 * How many routes the "has this changed" memory keeps.
 *
 * A route table is finite and this is larger than any project's hot set, so the
 * ceiling is `docs/security.md`'s rule rather than a policy about routes: a map
 * a page can grow by navigating is a map with a bound.
 */
const MEMORY_LIMIT = 64;

/** The last boundary set seen for each route path. */
const seen: Map<string, string> = new Map();

/** What the on-demand report reads; the reporter keeps it current. */
let current: {| readonly path: string, readonly boundaries: Map<string, RouteBoundary> |} | null =
  null;

/**
 * The boundary set as one comparable string.
 *
 * Kind, depth and source — everything a reader would notice — and not what the
 * page currently owns: a boundary whose subtree changed because the route's
 * data changed has not changed, and reporting it would make this fire on every
 * keystroke behind a search box.
 */
function signature(boundaries: Map<string, RouteBoundary>): string {
  return [...boundaries.values()].map((it) => `${it.id}@${it.above}:${it.source ?? ""}`).join("|");
}

/**
 * Send the report for whatever is on the page now.
 *
 * Exported for [`BOUNDARY_GLOBAL`] and used by the reporter, so the on-demand
 * answer and the automatic one are the same sentence about the same page.
 */
export function reportBoundaries(
  path: string,
  boundaries: Map<string, RouteBoundary>,
  document: Document,
): $ReadOnlyArray<BoundaryFinding> {
  const findings = boundaryFindings(boundaries, document);
  const { message, detail } = formatBoundaries(path, findings);
  reportDiagnostic({ severity: "info", message, detail });
  return findings;
}

/**
 * Watches the boundary set and says when it changed.
 *
 * Renders nothing, and its effect has no dependency list on purpose: the
 * question is asked after every commit, and the answer is a string comparison
 * over a handful of entries before anything touches the DOM. The document is
 * read only in the commit that is about to be reported, which is also the
 * commit the marks are in — every edge mounted after the first one starts live,
 * so a boundary that has just appeared is in the page by the time this runs.
 *
 * Quiet on the first sighting of a route, for the reason ubugeeei-prod/uf#636
 * is quiet on the first scan: a listing of everything, at the moment somebody
 * loaded a page, is not a thing anybody asked.
 */
export component BoundaryReporter(path: string, boundaries: Map<string, RouteBoundary>) {
  useEffect(() => {
    current = { path, boundaries };
    installOnDemand();
    const next = signature(boundaries);
    const previous = seen.get(path);
    if (seen.size >= MEMORY_LIMIT && previous === undefined) {
      const oldest = seen.keys().next();
      if (!oldest.done) {
        seen.delete(oldest.value);
      }
    }
    seen.set(path, next);
    if (previous === undefined || previous === next) {
      return;
    }
    const document = globalThis.document;
    if (document == null) {
      return;
    }
    reportBoundaries(path, boundaries, document);
  });
  return null;
}

/**
 * The global object, under the one description this module has of it.
 *
 * A read-only indexer, which is what makes the annotation assignable at all:
 * `globalThis` is a namespace to the checker, and every one of its members is
 * read-only, so a writable indexer disagrees with all of them at once. The same
 * shape `@uniflowed/react-testing`'s `internal/dom.js` reads globals through,
 * and for the same reason — a name in, and no claim about what comes out.
 */
type Globals = { readonly [string]: mixed };

/** The global object, for reading. */
const globals: Globals = globalThis;

/**
 * Install `__ufBoundaries()`, once.
 *
 * The answer to "and how do I see the page I am looking at *now*", which the
 * change-driven report deliberately does not give. A function on the global
 * rather than a key binding or a panel: there is nothing to discover by
 * accident, nothing to intercept a page's own keystrokes, and the console is
 * already open in the window this is about. It returns the findings as well as
 * printing them, so the browser shows the tree and the terminal keeps the line.
 *
 * Defined rather than assigned, for the reason `internal/dom.js` gives about
 * `navigator`: a name the host declared as an accessor cannot be assigned to,
 * and a development affordance must not be able to throw on a page.
 */
function installOnDemand(): void {
  if (globals[BOUNDARY_GLOBAL] != null) {
    return;
  }
  Object.defineProperty(globalThis, BOUNDARY_GLOBAL, {
    value: () => {
      const live = current;
      const document = globalThis.document;
      if (live == null || document == null) {
        return [];
      }
      return reportBoundaries(live.path, live.boundaries, document);
    },
    writable: true,
    configurable: true,
  });
}

/**
 * Forget every route this module has seen.
 *
 * For tests, which share one module registry across files and would otherwise
 * inherit a route's history from whichever file rendered it first.
 */
export function forgetBoundaries(): void {
  seen.clear();
  current = null;
  marksAreLive = false;
}
