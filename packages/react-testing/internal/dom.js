// @flow
//
// A document for a test process that has none.
//
// `uf test` runs on Node.js, Bun or Deno, and none of them has a DOM. React
// needs one before `react-dom` is imported, not after: `react-dom/client`
// reads `document` while it is being evaluated, so installing the globals has
// to happen first and exactly once per process.
//
// The window is created lazily rather than at import time, because a test file
// that imports this module and never renders should not pay for a DOM, and
// because `@uniflowed/lib`'s invariants forbid a package from doing work while
// it is being imported.

import { Window } from "happy-dom";

/**
 * A function this module found on a window and knows nothing else about.
 *
 * `mixed` in and `mixed` out is the whole contract: these are copied across
 * for React and for components to call, and nothing here ever calls one.
 */
type HostFunction = (...args: $ReadOnlyArray<mixed>) => mixed;

/**
 * Values kept by name, which is all this module knows about a window, the
 * global object, or a Storage.
 *
 * The indexer is the honest shape rather than a placeholder for a type nobody
 * wrote: the work this module does is to read names out of a list and hand the
 * values to `Object.defineProperty`. It does not call them, construct them, or
 * look inside them, so `mixed` is the amount it knows.
 */
type Named = { readonly [string]: mixed };

/**
 * A window, as this module uses one.
 *
 * A table of globals to copy, plus the three functions that are *bound* rather
 * than copied — and those are named because binding is the one thing an
 * indexer cannot describe. `typeof win[name] === "function"` refines a `mixed`
 * to a function whose parameters Flow does not know, and `.bind` is not
 * something that can be done to one of those:
 *
 *     error[incompatible-use]: Cannot call `value.bind` because
 *     `unknown function` is not a function type.
 *
 * So the list of functions to bind, which used to be a `FUNCTIONS` array
 * beside the other three arrays, is this part of the type instead. One list
 * rather than two, and it is the one the checker reads.
 *
 * `happy-dom` ships TypeScript rather than Flow, so `new Window(…)` is `any`.
 * This annotation is the first statement anywhere about what comes back, not a
 * cast that discards one.
 */
type HostWindow = {
  readonly getComputedStyle?: HostFunction,
  readonly requestAnimationFrame?: HostFunction,
  readonly cancelAnimationFrame?: HostFunction,
  readonly [string]: mixed,
};

/**
 * The global object, under the one description this module has of it.
 *
 * `globalThis` is a namespace to the checker rather than an object, so
 * `globalThis[name]` is not an expression that can be written —
 * `Cannot access namespace globalThis with computed property using string` —
 * and reading an installed global by a name from a list is what deciding
 * whether to install one requires. Naming the same object as a table says what
 * those reads are: a name in, and no claim at all about what comes out.
 *
 * Writes still go through `define`, because they have to be
 * `Object.defineProperty`; see the reason there.
 */
const globals: Named = globalThis;

/**
 * The document's own classes, which always replace whatever the host had.
 *
 * A document rejects an event built by a different implementation, and Node
 * defines `Event` and `CustomEvent` itself — `dispatchEvent` refused them with
 * "parameter 1 is not of type 'Event'" for every event this module had no more
 * specific constructor for. Whatever the host already had, the document's own
 * classes are the ones that work with the document.
 *
 * The list is also the list of questions the rest of the package can ask.
 * `internal/queries.js` and `internal/events.js` narrow an `Element` with
 * `instanceof HTMLInputElement` rather than reading `.value` off a cast, and
 * an `instanceof` against a name that was never installed is not a `false` —
 * it is `ReferenceError: HTMLFieldSetElement is not defined`, from a line
 * about clicking a tab. So a class this package needs to *recognise* belongs
 * here as much as one the document needs to accept.
 */
const CLASSES = [
  "Node",
  "Element",
  "HTMLElement",
  "HTMLInputElement",
  "HTMLTextAreaElement",
  "HTMLSelectElement",
  "HTMLButtonElement",
  "HTMLFieldSetElement",
  "HTMLAnchorElement",
  "SVGElement",
  "Event",
  "CustomEvent",
  "MouseEvent",
  "KeyboardEvent",
  "InputEvent",
  "FocusEvent",
  "PointerEvent",
  "SubmitEvent",
  // React form Actions construct FormData from this document's form element.
  "FormData",
  "DOMParser",
  "MutationObserver",
  "ResizeObserver",
  "IntersectionObserver",
];

/**
 * Objects a page has, installed only where the host has none.
 *
 * `navigator` is the reason for the distinction: on Node it is an accessor
 * with no setter, and assigning to it throws. A test does not need it
 * replaced — it needs it to exist.
 */
// React ViewTransition uses CSS.escape when naming DOM transition participants.
const OBJECTS = ["location", "history", "navigator", "CSS"];

/**
 * Storage, which is installed where the host has none *or has one that does
 * not work*.
 *
 * Node 25 defines `globalThis.localStorage` and leaves it unusable unless the
 * process was started with `--localstorage-file`:
 *
 * ```text
 * typeof globalThis.localStorage        // "object"
 * globalThis.localStorage.setItem       // undefined
 * ```
 *
 * So "the host already has one" is the wrong question, and asking it left
 * every `useStorage` test writing into an object with no `setItem` —
 * `globalThis.localStorage.setItem is not a function`, from a line that had
 * nothing to do with the hook under test. The question is whether it works,
 * and [`hostStorage`] is how it is asked without Node answering out loud.
 */
const STORAGE = ["localStorage", "sessionStorage"];

/**
 * The Storage the host has under `name`, without asking an accessor for it.
 *
 * The question is still "does the host's work", and it is still answered by
 * `isUsableStorage`. What changed is how the value is fetched, because on
 * Node 25 fetching it is not free: `localStorage` is an own accessor there, and
 * calling its getter prints
 *
 * ```text
 * (node:62028) Warning: `--localstorage-file` was provided without a valid path
 * ```
 *
 * once per process — into the middle of a `uf test` report, from a component
 * test that never mentions storage. Node 24 has no `localStorage` at all, so
 * the paragraph appeared the day a reader upgraded their runtime and nowhere
 * in this repository's own suite. See ubugeeei-prod/uf#308.
 *
 * So the descriptor is read instead of the property, and only a **data**
 * property's value is looked at. An accessor is reported as nothing there,
 * which is the right answer for the case that exists: Node's getter hands back
 * an object with no `setItem`, which `isUsableStorage` was going to reject
 * anyway. It is also the right answer for the case that does not — a Node
 * started with a valid `--localstorage-file`, whose storage is a file shared
 * with every other process using it, which is not the per-process state a test
 * suite means by `localStorage`.
 *
 * A real browser never reaches here: `installDom` returns early when the host
 * already has a `document`, and a page's `localStorage` is on `Window.prototype`
 * rather than an own property in any case.
 */
function hostStorage(name: string): mixed {
  const descriptor = Object.getOwnPropertyDescriptor(globalThis, name);
  if (descriptor == null || descriptor.get != null || descriptor.set != null) {
    return undefined;
  }
  return descriptor.value;
}

/**
 * Whether a value is a Storage a test can actually use.
 *
 * The four methods, not one: a half-implemented shim that has `getItem` and
 * no `removeItem` fails later and further away than one that is absent.
 */
function isUsableStorage(value: mixed): boolean {
  if (value == null || typeof value !== "object") {
    return false;
  }
  const storage: Named = value;
  return (
    typeof storage.getItem === "function" &&
    typeof storage.setItem === "function" &&
    typeof storage.removeItem === "function" &&
    typeof storage.clear === "function"
  );
}

let installed: HostWindow | null = null;

/**
 * Install a DOM on the global object, once.
 *
 * Returns the window, so a caller that wants the document can have it without
 * reaching through `globalThis`. Calling this a second time is free and does
 * not replace the document — replacing it mid-process would strand every React
 * root already mounted in the old one.
 */
export function installDom(): HostWindow {
  installActEnvironment();
  if (installed != null) {
    return installed;
  }

  // A real browser is not required to be absent — a project may already be
  // running these tests in one, and then the page's own DOM is the right one.
  if (typeof globalThis.document !== "undefined") {
    installed = globalThis.window ?? globalThis;
    return installed;
  }

  const win: HostWindow = new Window({ url: "http://localhost/" });

  for (const name of CLASSES) {
    const value = win[name];
    if (value !== undefined) {
      define(name, value);
    }
  }
  // The three by name rather than from a list: see `HostWindow`.
  defineBound("getComputedStyle", win.getComputedStyle, win);
  defineBound("requestAnimationFrame", win.requestAnimationFrame, win);
  defineBound("cancelAnimationFrame", win.cancelAnimationFrame, win);
  for (const name of OBJECTS) {
    const value = win[name];
    if (value !== undefined && globals[name] === undefined) {
      define(name, value);
    }
  }
  for (const name of STORAGE) {
    if (isUsableStorage(hostStorage(name))) {
      continue;
    }
    const value = win[name];
    if (isUsableStorage(value)) {
      define(name, value);
    }
  }

  // React reads these to decide it is in a browser and to pick its event
  // system, and they must be the objects the elements belong to.
  define("window", win);
  define("document", win.document);

  installed = win;
  return installed;
}

/**
 * Tell React that this process is running tests.
 *
 * React cannot tell a test from a production render, so `act` warns "The
 * current testing environment is not configured to support act(...)" unless
 * the harness says so. Every render in this package goes through `act`, so
 * without this every component test printed the warning — 73 times in one
 * file of this repository — and a warning worth reading was lost among them.
 *
 * Separate from the document because the two are independent: a project
 * already running in a browser has a DOM and still has to say it is testing.
 */
export function installActEnvironment(): void {
  if (declared) {
    return;
  }
  declared = true;
  define("IS_REACT_ACT_ENVIRONMENT", true);
}

/**
 * Turn the act environment on or off.
 *
 * `waitFor` stands it down for the length of a wait; see the reason there.
 */
export function setActEnvironment(active: boolean): void {
  declared = true;
  define("IS_REACT_ACT_ENVIRONMENT", active);
}

/**
 * Whether the flag has been installed, tracked separately from its value.
 *
 * Every query calls `installDom`, which installs the act environment, and
 * every query inside a `waitFor` therefore ran while `waitFor` had stood the
 * environment down. Reading the flag to decide whether to set it turned it
 * back on at the first assertion, so only the first poll of a wait was quiet.
 */
let declared = false;

/**
 * Assign a global, even where the host declared it as a getter.
 *
 * `navigator` on Node is an accessor with no setter, so a plain assignment
 * throws; anything installed here has to be defined rather than assigned.
 */
function define(name: string, value: mixed): void {
  Object.defineProperty(globalThis, name, {
    value,
    writable: true,
    configurable: true,
    enumerable: true,
  });
}

/** Install one of the window's own functions, still reading that window. */
function defineBound(name: string, fn: HostFunction | void, win: HostWindow): void {
  if (typeof fn === "function") {
    define(name, fn.bind(win));
  }
}

/** The document tests query, installing one if the process has none. */
export function documentOf(): Document {
  installDom();
  return globalThis.document;
}

/**
 * The body a test renders into and queries, installing a document first.
 *
 * Separate from `documentOf` because `Document.body` is `HTMLBodyElement |
 * null` — a document with no `<body>` is a document a parser can produce — and
 * both callers want the element rather than the question. A document this
 * module installed has a body, and a document the host already had is a page,
 * which also has one; the throw is for the third case, and it says what is
 * missing rather than leaving `Cannot read properties of null (reading
 * 'appendChild')` to be read at a line about rendering.
 */
export function bodyOf(): HTMLElement {
  const body = documentOf().body;
  if (body == null) {
    throw new Error("@uniflowed/react-testing: the document has no <body> to render into");
  }
  return body;
}
