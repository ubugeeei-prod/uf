// @flow
//
// Internal to `@uniflowed/router`: what actually differed, when hydration
// failed.
//
// React's message is the same eleven words every time — the server rendered
// HTML did not match the client — followed by a list of the things that usually
// cause it. It is a good list. It is not an answer: the reader still has to
// find which node, in a tree of two thousand, was the one, and then work out
// which of the six suggestions applies to it.
//
// uf can do better than a list because it has both trees. `hydrate` takes a
// copy of the server's markup out of the document *before* `hydrateRoot`
// touches it, and when React reports a mismatch this module walks that copy
// against the live DOM and finds the first node where they part company. What
// the overlay then shows is not advice: it is the two values, the path to the
// node that holds them, and the component that rendered it.
//
// # Why the snapshot is taken before hydration and not after
//
// After hydration the server's markup is gone. React repairs a mismatched
// subtree by rendering the client's version over it, so by the time
// `onRecoverableError` runs, the only tree left is the one that disagreed. The
// bytes the server sent exist for exactly one moment — between the parser
// finishing and `hydrateRoot` starting — and this module's whole ability to
// answer the question depends on somebody having copied them then.
//
// It is a copy of what the *parser* made of the server's bytes rather than the
// bytes themselves, and that is the better of the two. Attribute order,
// quoting and whitespace normalisation are already applied on both sides, so a
// difference this module reports is a difference in the tree rather than in
// how two serialisers spell it — and a `<div>` the parser had to lift out of a
// `<p>` has already been lifted, which is what makes bad nesting visible as a
// difference instead of invisible as a re-parse.
//
// # Three causes, and what each one is recognised by
//
// React names six causes; three of them account for nearly everything, and each
// leaves a different shape of difference behind.
//
//   `variable-input`   Both sides rendered text, both are non-empty, and they
//                      are the same text with different numbers in it — a
//                      clock, a random number, a duration, a formatted date.
//   `browser-only`     One side rendered something and the other rendered
//                      nothing: the signature of a `typeof window !==
//                      "undefined"` branch, or of a value read out of
//                      `localStorage` during a render.
//   `invalid-nesting`  React said so. The parser moves a node the tree cannot
//                      hold, so the difference that reaches here is a
//                      structural one with no cause visible in it — but React
//                      raises a separate, specific error for the nesting
//                      itself, and that error's words are the reliable signal.
//
// Anything else is `unknown`, said plainly. A guess dressed up as a diagnosis
// costs more than no diagnosis: it sends the reader to look at the wrong line
// and makes the whole overlay less believable. The two values and the path are
// the facts, and they are what the overlay leads with; the cause is a hint
// under them.
//
// The classifier reads text with hand-written single-pass scans and
// `String.includes`, never a regular expression. React's message embeds the
// application's own content, and the project's rule for text it did not write
// is that it does not go through a backtracking engine — see `docs/security.md`.
//
// # Why the overlay is plain DOM, and inside a shadow root
//
// It is drawn without React on purpose. React has just failed to hydrate; a
// second root mounted into the same document to explain why is one more thing
// that can throw while the reader is trying to read an error message. Fifty
// lines of `createElement` do not fail.
//
// Every value that came from the page — the two pieces of markup, the path, the
// component names, React's message — is written with `textContent`. Not one is
// interpolated into HTML. An overlay that rendered the server's markup as
// markup would execute the page's own scripts a second time and, on a page
// whose mismatch is somebody's comment, would be a cross-site scripting hole in
// the tool that exists to find bugs. The shadow root is for the other
// direction: the page's stylesheet cannot reach in and hide the report.
//
// # What this deliberately does not do
//
// It does not stop the mismatch. Making most of these not happen at all is the
// job of the render envelope in `@uniflowed/hooks/render` — one instant and one
// seed decided once and read by both sides (ubugeeei-prod/uf#554) — and this is
// for the ones that still do.
//
// # It does reach the terminal
//
// It did not, once. `uf dev`'s diagnostics come up the driver's event channel
// from the Node process, and a hydration mismatch happens in the browser, which
// had no channel to send one back on — so this report existed in an overlay and
// in the console, and had to be noticed by somebody who knew to look. There is
// a channel now: `./diagnostics.js` posts to `/__uf/diagnostic`, `uf dev`
// answers it, and the same words this module formats for the overlay are
// printed by the same renderer that prints a type error. See
// ubugeeei-prod/uf#583, and #508 for where the gap was named.
//
// The overlay stays. It is in front of the reader who caused the mismatch by
// editing the page, and the terminal is for the one who did not.

import { reportDiagnostic } from "./diagnostics.js";

/** Which of the usual causes the difference looks like. */
export type HydrationCause = "variable-input" | "browser-only" | "invalid-nesting" | "unknown";

/** What kind of difference was found at the node. */
export type DifferenceKind =
  /** Both sides have a node here and they are not the same kind of node. */
  | "node-type"
  /** Both sides rendered text, and the text is not the same. */
  | "text"
  /** Both sides rendered an element, and not the same element. */
  | "tag"
  /** Same element, and one attribute's value differs or is only on one side. */
  | "attribute"
  /** The client rendered a node the server did not. */
  | "extra"
  /** The server rendered a node the client did not. */
  | "missing";

/** The first place the two trees stopped agreeing. */
export type HydrationDifference = {|
  readonly kind: DifferenceKind,
  /** Where in the tree, as a selector a reader can paste into the console. */
  readonly path: string,
  /** The attribute's name, for `kind: "attribute"`. */
  readonly attribute: string | null,
  /** What the server sent, or `null` where it sent nothing at all. */
  readonly server: string | null,
  /** What the client rendered, or `null` where it rendered nothing at all. */
  readonly client: string | null,
|};

/** Everything worth telling somebody about one failed hydration. */
export type HydrationReport = {|
  /** React's own message, kept because it is the thing people search for. */
  readonly message: string,
  readonly cause: HydrationCause,
  /** One sentence saying what that cause means, in the reader's own page. */
  readonly explanation: string,
  /** What to do about it. Empty for `unknown`, which has no advice worth giving. */
  readonly remedy: string,
  /** `null` when the two trees agreed, or when there was no snapshot to compare. */
  readonly difference: HydrationDifference | null,
  /** The components React named, outermost last. */
  readonly components: $ReadOnlyArray<string>,
  /** Why there is no difference, when there is none. */
  readonly note: string | null,
|};

/**
 * The most server markup worth keeping.
 *
 * Every hydration pays this, on every page, whether or not anything ever goes
 * wrong — so it is a memory cost taken out of a reader's browser for a
 * diagnostic they will probably never see. Half a megabyte is a very large
 * document and a very small amount of memory; a page above it gets no
 * comparison and is told why, which is a better trade than a dev server that
 * doubles the footprint of the largest pages.
 */
export const SERVER_MARKUP_LIMIT: number = 512 * 1024;

/** The most nodes one comparison will visit. */
const MAX_NODES = 20_000;

/** The deepest a comparison will go. */
const MAX_DEPTH = 200;

/** The most of one side's markup the report will carry. */
const MAX_SHOWN = 400;

/** The most components to name out of a stack that can be hundreds deep. */
const MAX_COMPONENTS = 12;

const TEXT_NODE = 3;
const ELEMENT_NODE = 1;
const COMMENT_NODE = 8;

type DetachedHeadStyle = {|
  readonly anchor: Comment,
  readonly style: HTMLStyleElement,
|};

/**
 * Hide development-only head artifacts before React compares the document.
 *
 * `uf dev` injects the React DevTools hook, the Fast Refresh preamble and
 * Vite's dev styles into the head before the client entry. By the time
 * `hydrate` runs the scripts have done their job; leaving them in a
 * document-root app makes React compare bytes the application never rendered
 * and report a hydration mismatch for uf's own instrumentation.
 *
 * The styles are different: the page still needs them. They are detached only
 * for the instant in which React claims the document, and the returned function
 * puts them back in the same order.
 */
export function prepareDevHeadForHydration(document: Document): () => void {
  for (const script of document.head.querySelectorAll("script")) {
    if (isDevHeadScript(script)) {
      script.remove();
    }
  }
  for (const child of Array.from(document.head.childNodes)) {
    if (isIgnorableHeadWhitespace(child)) {
      child.remove();
    }
  }
  const detached = [];
  for (const style of document.head.querySelectorAll("style")) {
    if (isViteDevStyle(style)) {
      const anchor = document.createComment("uf dev style");
      document.head.insertBefore(anchor, style);
      style.remove();
      detached.push({ anchor, style });
    }
  }
  return () => restoreDevHeadStyles(document, detached);
}

function isDevHeadScript(script: HTMLScriptElement): boolean {
  if (script.getAttribute("data-uf-dev-head-preamble") != null) {
    return true;
  }
  const src = script.getAttribute("src");
  if (src == null) {
    return false;
  }
  const path = src.startsWith("http") ? new URL(src).pathname : src;
  return path === "/@vite/client" || path === "/@id/__x00__virtual:uf/client";
}

function isViteDevStyle(style: HTMLStyleElement): boolean {
  return style.getAttribute("data-vite-dev-id") != null;
}

function restoreDevHeadStyles(
  document: Document,
  detached: $ReadOnlyArray<DetachedHeadStyle>,
): void {
  for (const { anchor, style } of detached) {
    const parent = anchor.parentNode;
    if (parent != null) {
      parent.insertBefore(style, anchor);
      parent.removeChild(anchor);
    } else {
      document.head.appendChild(style);
    }
  }
}

/**
 * What the server sent, taken out of the document before React touches it.
 *
 * `null` when there is nothing to take or too much of it, and the report says
 * which. Callers hold the string for the life of the page, so this is the one
 * place the size is bounded.
 *
 * A `Document` container — the shape an application whose root layout renders
 * `<html>` hydrates into — has no `innerHTML`, so the whole element is taken
 * instead. The two cases are put back together by `parseServerMarkup`, which is
 * why they are allowed to differ here.
 */
export function captureServerMarkup(container: Node): string | null {
  const markup = serialize(container);
  if (markup == null || markup.length > SERVER_MARKUP_LIMIT) {
    return null;
  }
  return markup;
}

function serialize(container: Node): string | null {
  const asDocument = container as $FlowFixMe as { documentElement?: ?Element, ... };
  if (asDocument.documentElement != null) {
    return asDocument.documentElement.outerHTML;
  }
  const asElement = container as $FlowFixMe as { innerHTML?: ?string, ... };
  return typeof asElement.innerHTML === "string" ? asElement.innerHTML : null;
}

/**
 * Read a snapshot back into a tree, beside the live one it will be compared to.
 *
 * Returns the two lists of roots, in step. A document's roots are its one
 * `<html>`; an element's are its children, because `innerHTML` is what was
 * kept. The doctype is not compared: React never renders one and no mismatch
 * has ever been about it.
 */
function parseServerMarkup(
  markup: string,
  container: Node,
  document: Document,
): {| server: $ReadOnlyArray<Node>, client: $ReadOnlyArray<Node> |} | null {
  const asDocument = container as $FlowFixMe as { documentElement?: ?Element, ... };
  const live = asDocument.documentElement;
  if (live != null) {
    const parsed = new DOMParser().parseFromString(markup, "text/html");
    const root = parsed.documentElement;
    return root == null ? null : { server: [root], client: [live] };
  }
  const template = document.createElement("template");
  template.innerHTML = markup;
  return { server: children(template.content), client: children(container) };
}

function children(node: Node): Array<Node> {
  const out = [];
  for (const child of node.childNodes) {
    if (isDevHeadArtifact(child)) {
      continue;
    }
    out.push(child);
  }
  return out;
}

function isDevHeadArtifact(node: Node): boolean {
  if (isIgnorableHeadWhitespace(node)) {
    return true;
  }
  if (node.nodeType === COMMENT_NODE && node.textContent === "uf dev style") {
    return true;
  }
  if (node.nodeType !== ELEMENT_NODE) {
    return false;
  }
  const element = node as $FlowFixMe as Element;
  if (element.tagName === "STYLE" && element.getAttribute("data-vite-dev-id") != null) {
    return true;
  }
  if (element.tagName !== "SCRIPT") {
    return false;
  }
  return isDevHeadScript(element as $FlowFixMe as HTMLScriptElement);
}

function isIgnorableHeadWhitespace(node: Node): boolean {
  return (
    node.nodeType === TEXT_NODE &&
    node.parentNode != null &&
    (node.parentNode as $FlowFixMe as Element).tagName === "HEAD" &&
    (node.textContent ?? "").trim() === ""
  );
}

/**
 * Whether a message React handed to `onRecoverableError` is about hydration.
 *
 * React sends more than mismatches through that callback — a Suspense boundary
 * that recovered by rendering on the client is the other common one — and an
 * overlay that opened for those would be an overlay people turn off. Matching
 * on the message rather than on an error class because React does not export
 * one, and because the wording is the part that has stayed stable across
 * versions when the internals have not.
 */
export function isHydrationMessage(message: string): boolean {
  return (
    message.includes("Hydration failed") ||
    message.includes("did not match") ||
    message.includes("didn't match") ||
    message.includes("cannot be a descendant of") ||
    message.includes("There was an error while hydrating")
  );
}

/** One frame of the comparison, on an explicit stack rather than the call stack. */
type Frame =
  | {| kind: "pair", server: Node, client: Node, path: string, depth: number |}
  | {| kind: "unmatched", server: Node | null, client: Node | null, path: string |};

/**
 * Walk both trees in document order and stop at the first disagreement.
 *
 * The stack is explicit and both the node count and the depth are bounded,
 * because the trees are the application's and a diagnostic is not a place to
 * find out what a deeply nested page does to the stack. Running out of either
 * reports no difference rather than a wrong one.
 */
function firstDifference(
  serverRoots: $ReadOnlyArray<Node>,
  clientRoots: $ReadOnlyArray<Node>,
): HydrationDifference | null {
  const stack: Array<Frame> = [];
  pushChildren(stack, serverRoots, clientRoots, "", 0);

  let visited = 0;
  while (stack.length > 0) {
    visited += 1;
    if (visited > MAX_NODES) {
      return null;
    }
    const frame = stack.pop();
    if (frame == null) {
      return null;
    }
    if (frame.kind === "unmatched") {
      return {
        kind: frame.server == null ? "extra" : "missing",
        path: frame.path,
        attribute: null,
        server: frame.server == null ? null : show(frame.server),
        client: frame.client == null ? null : show(frame.client),
      };
    }

    const { server, client, path, depth } = frame;
    if (server.nodeType !== client.nodeType) {
      return {
        kind: "node-type",
        path,
        attribute: null,
        server: show(server),
        client: show(client),
      };
    }
    if (server.nodeType === TEXT_NODE || server.nodeType === COMMENT_NODE) {
      if (server.textContent !== client.textContent) {
        return {
          kind: "text",
          path,
          attribute: null,
          server: show(server),
          client: show(client),
        };
      }
      continue;
    }
    if (server.nodeType !== ELEMENT_NODE) {
      continue;
    }

    const serverElement = server as $FlowFixMe as Element;
    const clientElement = client as $FlowFixMe as Element;
    if (serverElement.tagName !== clientElement.tagName) {
      return { kind: "tag", path, attribute: null, server: show(server), client: show(client) };
    }
    const attribute = differingAttribute(serverElement, clientElement);
    if (attribute != null) {
      return {
        kind: "attribute",
        path,
        attribute,
        server: attributeOf(serverElement, attribute),
        client: attributeOf(clientElement, attribute),
      };
    }
    if (depth < MAX_DEPTH) {
      pushChildren(stack, children(server), children(client), path, depth + 1);
    }
  }
  return null;
}

/**
 * Queue the children of a matched pair, in reverse so the stack pops them in
 * document order, with a marker where one side runs out before the other.
 */
function pushChildren(
  stack: Array<Frame>,
  server: $ReadOnlyArray<Node>,
  client: $ReadOnlyArray<Node>,
  path: string,
  depth: number,
): void {
  const most = Math.max(server.length, client.length);
  for (let index = most - 1; index >= 0; index -= 1) {
    const onServer = server[index] ?? null;
    const onClient = client[index] ?? null;
    if (onServer == null || onClient == null) {
      const present = onServer ?? onClient;
      stack.push({
        kind: "unmatched",
        server: onServer,
        client: onClient,
        path: present == null ? path : join(path, present, index),
      });
      continue;
    }
    stack.push({
      kind: "pair",
      server: onServer,
      client: onClient,
      path: join(path, onServer, index),
      depth,
    });
  }
}

/** One more step of the selector that names where the difference is. */
function join(path: string, node: Node, index: number): string {
  const step = describe(node, index);
  return path === "" ? step : `${path} > ${step}`;
}

function describe(node: Node, index: number): string {
  if (node.nodeType === TEXT_NODE) {
    return `text()[${String(index)}]`;
  }
  if (node.nodeType === COMMENT_NODE) {
    return `comment()[${String(index)}]`;
  }
  if (node.nodeType !== ELEMENT_NODE) {
    return `node()[${String(index)}]`;
  }
  const element = node as $FlowFixMe as Element;
  const tag = element.tagName.toLowerCase();
  const id = element.getAttribute("id");
  // An id is what a reader recognises, so it wins over a position. Without one
  // the position is what makes the selector select one node.
  return id == null || id === "" ? `${tag}:nth-child(${String(index + 1)})` : `${tag}#${id}`;
}

/** The name of the first attribute the two elements disagree about. */
function differingAttribute(server: Element, client: Element): string | null {
  const names = new Set<string>();
  for (const attribute of server.attributes) {
    names.add(attribute.name);
  }
  for (const attribute of client.attributes) {
    names.add(attribute.name);
  }
  // Sorted, so the answer does not depend on the order a parser happened to
  // record them in — two runs of the same page have to blame the same
  // attribute.
  for (const name of [...names].sort()) {
    if (attributeOf(server, name) !== attributeOf(client, name)) {
      return name;
    }
  }
  return null;
}

/**
 * One attribute, as `null` when it is absent.
 *
 * `getAttribute` is declared as returning `string | void`, and an absent
 * attribute has to compare equal to an absent attribute: without this, `void`
 * on one side and `null` on the other would be reported as a difference between
 * two elements that both simply do not have it.
 */
function attributeOf(element: Element, name: string): string | null {
  return element.getAttribute(name) ?? null;
}

/** One node, as much of it as is worth putting in a message. */
function show(node: Node): string {
  const text =
    node.nodeType === ELEMENT_NODE
      ? (node as $FlowFixMe as Element).outerHTML
      : (node.textContent ?? "");
  return text.length > MAX_SHOWN ? `${text.slice(0, MAX_SHOWN)}…` : text;
}

/**
 * Whether two pieces of text are the same sentence with different numbers.
 *
 * The signature of a clock, a countdown, a random number and a date formatted
 * in somebody's locale: "3 minutes ago" against "5 minutes ago", "1/2/2026"
 * against "02/01/2026", "0.8102…" against "0.4471…". Every run of digits on
 * both sides is collapsed to one placeholder and what is left has to match
 * exactly, so "42 items" against "43 items" is variable input and "Sign in"
 * against "Sign out" is not.
 *
 * A single left-to-right scan with no regular expression, because both strings
 * came out of the application's own rendered page. See `docs/security.md`.
 */
function sameShape(left: string, right: string): boolean {
  return digitShape(left) === digitShape(right);
}

function digitShape(text: string): string {
  let out = "";
  let inDigits = false;
  for (let at = 0; at < text.length; at += 1) {
    const code = text.charCodeAt(at);
    const isDigit = code >= 48 && code <= 57;
    if (isDigit) {
      if (!inDigits) {
        out += "#";
        inDigits = true;
      }
      continue;
    }
    inDigits = false;
    out += text[at];
  }
  return out;
}

/** Decide which of the three usual causes this looks like. */
function classify(message: string, difference: HydrationDifference | null): HydrationCause {
  // React raises its own error for nesting the parser cannot honour, and its
  // words are a better signal than anything the resulting tree looks like:
  // by the time the tree exists the node has already been moved.
  if (
    message.includes("cannot be a descendant of") ||
    message.includes("cannot contain a nested")
  ) {
    return "invalid-nesting";
  }
  if (difference == null) {
    return "unknown";
  }
  // An attribute present on one side and absent on the other is a different
  // question from one whose value differs, and `?? ""` below erases it: the
  // empty strings exist for `sameShape`, which has nothing to compare when a
  // side rendered nothing at all.
  const oneSided = difference.server == null || difference.client == null;
  const server = difference.server ?? "";
  const client = difference.client ?? "";
  // Every kind is named, so a kind added to `DifferenceKind` stops this file
  // compiling rather than arriving in somebody's overlay as "unknown".
  return match (difference.kind) {
    "extra" | "missing" => "browser-only",
    // Text on one side and whitespace on the other is a node that only one
    // render produced, not two renders that disagreed about a value.
    "text" if (server.trim() === "" || client.trim() === "") => "browser-only",
    "text" => sameShape(server, client) ? "variable-input" : "unknown",
    "attribute" if (oneSided) => "browser-only",
    "attribute" => sameShape(server, client) ? "variable-input" : "unknown",
    // The two trees hold different nodes here, which says nothing about why.
    "node-type" | "tag" => "unknown",
  };
}

const EXPLANATIONS: { readonly [HydrationCause]: string } = {
  "variable-input":
    "The two renders computed the same text from a value that is different every time it is read — a clock, a random number, or a date formatted in a locale.",
  "browser-only":
    "One render produced something the other did not, which is what a value only a browser can read looks like: `window`, `localStorage`, `navigator`, or a `typeof window` branch.",
  "invalid-nesting":
    "The markup cannot nest the way the tree asked for, so the parser moved a node and the server's tree stopped being the shape the client rebuilt.",
  unknown: "",
};

const REMEDIES: { readonly [HydrationCause]: string } = {
  "variable-input":
    "Decide the value once, above the tree, and pass it down, so both renders read the same number instead of each working one out.",
  "browser-only":
    "Read it through `useSyncExternalStore` with a server snapshot, so the first render on both sides is the same value and the browser's answer arrives in the render after it.",
  "invalid-nesting":
    "Change the markup so the tree is one the parser can keep: the element named above cannot hold the element inside it.",
  unknown: "",
};

/**
 * Everything worth saying about one failed hydration.
 *
 * Pure, and takes the document it should work in, so the whole of the analysis
 * is testable without a hydration ever having happened. Nothing here reads a
 * global.
 */
export function hydrationReport(input: {|
  readonly message: string,
  readonly serverMarkup: string | null,
  readonly container: Node | null,
  readonly document: Document,
  readonly componentStack: string | null,
|}): HydrationReport {
  const components = componentsOf(input.componentStack);
  const { difference, note } = compare(input);
  const cause = classify(input.message, difference);
  return {
    message: input.message,
    cause,
    explanation: EXPLANATIONS[cause],
    remedy: REMEDIES[cause],
    difference,
    components,
    note,
  };
}

function compare(input: {|
  readonly message: string,
  readonly serverMarkup: string | null,
  readonly container: Node | null,
  readonly document: Document,
  readonly componentStack: string | null,
|}): {| difference: HydrationDifference | null, note: string | null |} {
  if (input.serverMarkup == null || input.container == null) {
    return {
      difference: null,
      note: `No copy of the server's markup was kept, so there is nothing to compare against. A document over ${String(SERVER_MARKUP_LIMIT)} bytes is not snapshotted.`,
    };
  }
  let trees = null;
  try {
    trees = parseServerMarkup(input.serverMarkup, input.container, input.document);
  } catch {
    // A snapshot that will not parse back is a broken snapshot, not a broken
    // page: the report is worth less and the page is not worth failing for it.
    trees = null;
  }
  if (trees == null) {
    return { difference: null, note: "The copy of the server's markup could not be read back." };
  }
  const difference = firstDifference(trees.server, trees.client);
  if (difference != null) {
    return { difference, note: null };
  }
  return {
    difference: null,
    // React repairs the tree as it goes, so a mismatch reported late enough can
    // have been patched before this runs. Saying so is better than showing two
    // identical trees and letting the reader wonder what they missed.
    note: "The two trees agree by the time they were compared, which means React had already repaired the node it reported.",
  };
}

/**
 * The component names out of React's stack, innermost first.
 *
 * Each frame is `    at Name (url:line:column)`, and the name is all of it that
 * is worth showing: the url is a bundler's, and the reader is being asked
 * "which of my components", not "which of my chunks".
 *
 * The host elements are dropped, and that is the difference between an answer
 * and a list. React's stack interleaves them with the components — `p`, `main`,
 * `Posted` — so the innermost frame of a text mismatch is always the `<p>` the
 * text is in, which the reader can already see in the path above it. What they
 * cannot see is which of their own components rendered that `<p>`. JSX's own
 * rule decides which is which: a name beginning with a lower-case letter is an
 * element, and everything else is a component.
 */
export function componentsOf(componentStack: string | null): $ReadOnlyArray<string> {
  if (componentStack == null) {
    return [];
  }
  const names = [];
  for (const line of componentStack.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed.startsWith("at ")) {
      continue;
    }
    const rest = trimmed.slice(3).trim();
    const space = rest.indexOf(" ");
    const name = space === -1 ? rest : rest.slice(0, space);
    if (name !== "" && !isHostElement(name)) {
      names.push(name);
    }
    if (names.length >= MAX_COMPONENTS) {
      break;
    }
  }
  return names;
}

/** JSX's rule: a lower-case first letter is an element, anything else a component. */
function isHostElement(name: string): boolean {
  const first = name.charCodeAt(0);
  return first >= 97 && first <= 122;
}

/**
 * The report as text.
 *
 * One formatter for the overlay and the console, so the two never drift into
 * saying different things about the same failure — and so a test can assert the
 * words without a document.
 */
export function formatHydrationReport(report: HydrationReport): string {
  const lines = [];
  lines.push(reportTitle(report));
  lines.push("");
  lines.push(
    "The server HTML and the browser's first render disagreed. React repaired the page, but the first paint may not be the UI you meant to ship.",
  );
  lines.push("");
  const difference = report.difference;
  if (difference == null) {
    lines.push("What changed");
    lines.push(report.note ?? "There is nothing to compare.");
  } else {
    lines.push("What changed");
    lines.push(`path             ${difference.path}`);
    if (difference.attribute != null) {
      lines.push(`attribute        ${difference.attribute}`);
    }
    lines.push(`server rendered  ${difference.server ?? "(nothing)"}`);
    lines.push(`browser rendered ${difference.client ?? "(nothing)"}`);
  }
  if (report.explanation !== "") {
    lines.push("");
    lines.push("Likely cause");
    lines.push(report.explanation);
    lines.push("");
    lines.push("Try this next");
    lines.push(report.remedy);
  }
  if (report.components.length > 1) {
    lines.push("");
    lines.push(`Component trail: ${report.components.join(" < ")}`);
  }
  lines.push("");
  lines.push(`React's original message: ${headline(report.message)}`);
  return lines.join("\n");
}

function reportTitle(report: HydrationReport): string {
  return report.components.length > 0
    ? `Hydration mismatch in <${report.components[0]}>`
    : "Hydration mismatch";
}

/**
 * React's sentence, without the list of causes underneath it.
 *
 * The list is six bullets long and it is the thing this whole panel exists to
 * replace: a reader who has just been told which node, which two values and
 * which component does not need to be asked to consider whether it might have
 * been a browser extension. The first line is kept because it is the string
 * people paste into a search engine.
 */
function headline(message: string): string {
  const end = message.indexOf("\n");
  return (end === -1 ? message : message.slice(0, end)).trim();
}

/** The element the overlay lives in, so a second mismatch replaces the first. */
const OVERLAY_ID = "uf-hydration-overlay";

const OVERLAY_STYLE = `
:host { all: initial; }
.panel {
  position: fixed;
  inset: auto 1rem 1rem 1rem;
  z-index: 2147483647;
  max-height: min(78vh, 720px);
  overflow: auto;
  padding: 1rem;
  border: 1px solid #fecaca;
  border-radius: 8px;
  background: #fffafa;
  color: #1f2937;
  font: 14px/1.55 ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  box-shadow: 0 18px 60px rgba(15, 23, 42, 0.24);
}
.top { display: grid; gap: 0.35rem; padding-right: 2rem; }
.badge {
  color: #be123c;
  font-size: 0.75rem;
  font-weight: 800;
  letter-spacing: 0;
  text-transform: uppercase;
}
h2 { margin: 0; color: #111827; font-size: 1.05rem; line-height: 1.25; }
h3 { margin: 0 0 0.45rem; color: #9f1239; font-size: 0.82rem; line-height: 1.25; }
.lead { margin: 0; color: #4b5563; }
section {
  margin-top: 0.85rem;
  padding-top: 0.85rem;
  border-top: 1px solid #fee2e2;
}
dl {
  display: grid;
  grid-template-columns: max-content minmax(0, 1fr);
  gap: 0.35rem 0.85rem;
  margin: 0;
}
dt { color: #9f1239; font-weight: 800; }
dd {
  margin: 0;
  color: #111827;
  overflow-wrap: anywhere;
  white-space: pre-wrap;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 0.82rem;
}
p { margin: 0; color: #374151; }
.react { color: #6b7280; font-size: 0.82rem; }
button {
  position: absolute; top: 0.65rem; right: 0.65rem;
  border: 1px solid #fecaca;
  border-radius: 8px;
  background: #ffffff;
  cursor: pointer;
  font: inherit;
  color: #9f1239;
  padding: 0.3rem 0.55rem;
}
@media (min-width: 760px) {
  .panel { left: auto; width: min(680px, calc(100vw - 2rem)); }
}
`;

/**
 * Draw the report over the page.
 *
 * Every value out of the page goes in with `textContent`, never as markup —
 * see the header. The panel replaces itself, so a page that reports six
 * mismatches shows the first one it found rather than six stacked panels, and
 * the console still has all six.
 */
export function showHydrationReport(report: HydrationReport, document: Document): void {
  const body = document.body;
  if (body == null) {
    return;
  }
  const existing = document.getElementById(OVERLAY_ID);
  if (existing != null) {
    return;
  }

  const host = document.createElement("div");
  host.id = OVERLAY_ID;
  const root = host.attachShadow({ mode: "open" });

  const style = document.createElement("style");
  style.textContent = OVERLAY_STYLE;
  root.appendChild(style);

  const panel = document.createElement("div");
  panel.className = "panel";
  panel.setAttribute("role", "dialog");
  panel.setAttribute("aria-modal", "false");
  panel.setAttribute("aria-labelledby", "uf-hydration-title");
  panel.setAttribute("aria-describedby", "uf-hydration-lead");

  const top = document.createElement("div");
  top.className = "top";
  const badge = document.createElement("div");
  badge.className = "badge";
  badge.textContent = "uf dev hydration report";
  top.appendChild(badge);
  const heading = document.createElement("h2");
  heading.id = "uf-hydration-title";
  heading.textContent = reportTitle(report);
  top.appendChild(heading);
  const lead = paragraph(
    document,
    "The server HTML and the browser's first render did not match. React repaired it, but this is the first place to look.",
    "lead",
  );
  lead.id = "uf-hydration-lead";
  top.appendChild(lead);
  panel.appendChild(top);

  const dismiss = document.createElement("button");
  dismiss.type = "button";
  dismiss.textContent = "Close";
  dismiss.setAttribute("aria-label", "Close hydration report");
  dismiss.addEventListener("click", () => host.remove());
  panel.appendChild(dismiss);

  const rows = document.createElement("dl");
  const difference = report.difference;
  if (difference == null) {
    addRow(document, rows, "note", report.note ?? "There is nothing to compare.");
  } else {
    addRow(document, rows, "at", difference.path);
    if (difference.attribute != null) {
      addRow(document, rows, "attribute", difference.attribute);
    }
    addRow(document, rows, "server", difference.server ?? "(nothing)");
    addRow(document, rows, "client", difference.client ?? "(nothing)");
  }
  panel.appendChild(section(document, "What changed", rows));

  if (report.explanation !== "") {
    panel.appendChild(
      section(document, "Likely cause", paragraph(document, report.explanation, null)),
    );
    panel.appendChild(section(document, "Try this next", paragraph(document, report.remedy, null)));
  }
  if (report.components.length > 1) {
    panel.appendChild(
      section(
        document,
        "Component trail",
        paragraph(document, report.components.join(" < "), null),
      ),
    );
  }
  panel.appendChild(
    section(
      document,
      "React's original message",
      paragraph(document, headline(report.message), "react"),
    ),
  );

  root.appendChild(panel);
  body.appendChild(host);
}

function section(document: Document, title: string, child: Element): Element {
  const element = document.createElement("section");
  const heading = document.createElement("h3");
  heading.textContent = title;
  element.appendChild(heading);
  element.appendChild(child);
  return element;
}

function addRow(document: Document, rows: Element, label: string, value: string): void {
  const term = document.createElement("dt");
  term.textContent = label;
  const detail = document.createElement("dd");
  detail.textContent = value;
  rows.appendChild(term);
  rows.appendChild(detail);
}

function paragraph(document: Document, text: string, className: string | null): Element {
  const element = document.createElement("p");
  element.textContent = text;
  if (className != null) {
    element.className = className;
  }
  return element;
}

/**
 * The `onRecoverableError` `hydrate` installs in development.
 *
 * Everything React sends that is not a mismatch goes back to the host's own
 * `reportError`, which is what React would have done: this callback replaces
 * React's default rather than adding to it, so anything it swallows is
 * swallowed for good.
 *
 * A mismatch goes to three places, and all three say the same words because all
 * three come out of [`formatHydrationReport`]: the overlay, for the reader
 * looking at the page; the console, for a headless run, a CI browser and a
 * reader who closed the panel; and `uf dev`'s terminal, through
 * [`reportDiagnostic`], for the reader who is not looking at the browser at
 * all. See ubugeeei-prod/uf#583.
 */
export function hydrationErrorHandler(
  container: Node,
  serverMarkup: string | null,
  document: Document,
): (error: mixed, info: { componentStack?: ?string, ... }) => void {
  return (error, info) => {
    const message = error instanceof Error ? error.message : String(error);
    if (!isHydrationMessage(message)) {
      reportOrThrow(error);
      return;
    }
    let text = "";
    try {
      const report = hydrationReport({
        message,
        serverMarkup,
        container,
        document,
        componentStack: info?.componentStack ?? null,
      });
      text = formatHydrationReport(report);
      showHydrationReport(report, document);
    } catch (failure) {
      // The diagnostic failed. React's own error is the thing the reader
      // actually needs, and losing it because the explanation threw would be
      // the worst outcome of the whole module.
      reportOrThrow(error);
      reportOrThrow(failure);
      return;
    }
    // eslint of any kind is not what stops this being noise: it is that the
    // console is where a headless run, a CI browser and a reader who closed
    // the panel all still see the report.
    console.error(text);
    // And the terminal, where every other uf diagnostic already is. The
    // headline is the first line and the rest is the detail, which is the
    // shape the channel carries and is why the formatter puts the sentence
    // first: one report, one wording, three places. No position goes with it —
    // a mismatch is a fact about a DOM node rather than about a line of a
    // file, and inventing a file and a line to earn a code frame would send
    // the reader somewhere that is not the answer.
    const [headlineLine, ...rest] = text.split("\n");
    reportDiagnostic({
      severity: "error",
      message: headlineLine,
      detail: rest,
    });
  };
}

function reportOrThrow(error: mixed): void {
  const host = globalThis as $FlowFixMe as { reportError?: (error: mixed) => void, ... };
  if (typeof host.reportError === "function") {
    host.reportError(error);
    return;
  }
  console.error(error);
}
