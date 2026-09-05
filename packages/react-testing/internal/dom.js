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
 * The document's own classes, which always replace whatever the host had.
 *
 * A document rejects an event built by a different implementation, and Node
 * defines `Event` and `CustomEvent` itself — `dispatchEvent` refused them with
 * "parameter 1 is not of type 'Event'" for every event this module had no more
 * specific constructor for. Whatever the host already had, the document's own
 * classes are the ones that work with the document.
 */
const CLASSES = [
  "Node",
  "Element",
  "HTMLElement",
  "HTMLInputElement",
  "HTMLTextAreaElement",
  "HTMLSelectElement",
  "HTMLButtonElement",
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
  "DOMParser",
  "MutationObserver",
  "ResizeObserver",
  "IntersectionObserver",
];

/**
 * Functions that read the window they came from, so they are bound to it.
 */
const FUNCTIONS = ["getComputedStyle", "requestAnimationFrame", "cancelAnimationFrame"];

/**
 * Objects a page has, installed only where the host has none.
 *
 * `navigator` is the reason for the distinction: on Node it is an accessor
 * with no setter, and assigning to it throws. A test does not need it
 * replaced — it needs it to exist.
 */
const OBJECTS = ["location", "history", "navigator"];

/**
 * Storage, which is installed where the host has none *or has one that does
 * not work*.
 *
 * Node defines `globalThis.localStorage` and leaves it empty unless the
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
 * nothing to do with the hook under test. The question is whether it works.
 */
const STORAGE = ["localStorage", "sessionStorage"];

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
  const storage: { [string]: mixed } = value as any;
  return (
    typeof storage.getItem === "function" &&
    typeof storage.setItem === "function" &&
    typeof storage.removeItem === "function" &&
    typeof storage.clear === "function"
  );
}

let installed: mixed = null;

/**
 * Install a DOM on the global object, once.
 *
 * Returns the window, so a caller that wants the document can have it without
 * reaching through `globalThis`. Calling this a second time is free and does
 * not replace the document — replacing it mid-process would strand every React
 * root already mounted in the old one.
 */
export function installDom(): mixed {
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

  const win = new Window({ url: "http://localhost/" });

  for (const name of CLASSES) {
    const value = (win as any)[name];
    if (value !== undefined) {
      define(name, value);
    }
  }
  for (const name of FUNCTIONS) {
    const value = (win as any)[name];
    if (typeof value === "function") {
      define(name, value.bind(win));
    }
  }
  for (const name of OBJECTS) {
    const value = (win as any)[name];
    if (value !== undefined && globalThis[name] === undefined) {
      define(name, value);
    }
  }
  for (const name of STORAGE) {
    if (isUsableStorage(globalThis[name])) {
      continue;
    }
    const value = (win as any)[name];
    if (isUsableStorage(value)) {
      define(name, value);
    }
  }

  // React reads these to decide it is in a browser and to pick its event
  // system, and they must be the objects the elements belong to.
  define("window", win as any);
  define("document", (win as any).document);

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

/** The document tests query, installing one if the process has none. */
export function documentOf(): Document {
  installDom();
  return globalThis.document as any;
}
