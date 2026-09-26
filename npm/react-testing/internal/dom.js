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
 * A table of globals to copy, plus the three functions that are forwarded to
 * the window rather than copied — and those are named because binding is the one thing an
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
 * An interface rather than an object type, because a `happy-dom` `Window` is
 * a class instance, which is only ever a subtype of an interface. `uf check`
 * reads `happy-dom`'s TypeScript declarations now, so what `new Window(…)`
 * answers has to fit this, where it used to be `any`.
 *
 * The three are *methods* here, and are called through the window rather than
 * detached from it (see `apply`): reading a method off an instance as a value
 * is the `method-unbinding` error, which is the checker saying exactly what
 * `.bind` was there to prevent. Their parameters are `empty` because this
 * module passes through whatever React calls them with and never calls one
 * itself; `empty` is the one parameter type every window's own signature
 * accepts.
 */
interface HostWindow {
  getComputedStyle(...args: Array<empty>): mixed;
  requestAnimationFrame(...args: Array<empty>): mixed;
  cancelAnimationFrame(...args: Array<empty>): mixed;
  /** The window's document, which becomes the global `document`. */
  readonly document: mixed;
  readonly [string]: mixed;
}

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

/** The address a window this module creates starts at, and goes back to. */
const HOME = "http://localhost/";

/**
 * A page's own window, when the process already had a document.
 *
 * That window belongs to the page — a project running its tests in a browser —
 * and nothing here takes it back.
 */
let installed: HostWindow | null = null;

/** The window this module created, and what putting the process back needs. */
type Created = {|
  readonly win: HostWindow,
  /** Every own property of the window, as it was created. */
  readonly pristine: Map<string | symbol, PropertyDescriptor<mixed>>,
  /**
   * What each global this module defines was before it first did: its
   * descriptor, or `undefined` when the host had no such global.
   */
  readonly previous: Map<string, PropertyDescriptor<mixed> | void>,
  /** Whether the globals are this window's right now. */
  applied: boolean,
|};

/** The window this module created, once a render in this process needed one. */
let created: Created | null = null;

/**
 * Install a DOM on the global object.
 *
 * Returns the window, so a caller that wants the document can have it without
 * reaching through `globalThis`. The window is created once per process and
 * never replaced — replacing it mid-file would strand every React root already
 * mounted in the old one. What changes between files is whether the globals
 * point at it: `uf test`'s worker takes them back before the next file, and
 * the next render here puts them back on the same window. See
 * [`restoreBetweenFiles`].
 */
export function installDom(): HostWindow {
  installActEnvironment();
  const existing = created;
  if (existing != null) {
    if (!existing.applied) {
      apply(existing);
    }
    return existing.win;
  }
  if (installed != null) {
    return installed;
  }

  // A real browser is not required to be absent — a project may already be
  // running these tests in one, and then the page's own DOM is the right one.
  if (typeof globalThis.document !== "undefined") {
    installed = globalThis.window ?? globalThis;
    return installed;
  }

  const win: HostWindow = new Window({ url: HOME });
  const pristine: Map<string | symbol, PropertyDescriptor<mixed>> = new Map();
  for (const key of Reflect.ownKeys(win)) {
    const descriptor = Reflect.getOwnPropertyDescriptor(win, key);
    if (descriptor != null) {
      pristine.set(key, descriptor);
    }
  }
  created = { win, pristine, previous: new Map(), applied: false };
  apply(created);
  registerBetweenFiles();
  return win;
}

/** Point the globals a document needs at `dom`'s window. */
function apply(dom: Created): void {
  const { win } = dom;
  for (const name of CLASSES) {
    const value = win[name];
    if (value !== undefined) {
      defineFor(dom, name, value);
    }
  }
  // The three by name rather than from a list, each called through the window
  // it belongs to, which is what binding it did: see `HostWindow`.
  defineFor(dom, "getComputedStyle", (...args: Array<empty>) => win.getComputedStyle(...args));
  defineFor(dom, "requestAnimationFrame", (...args: Array<empty>) =>
    win.requestAnimationFrame(...args),
  );
  defineFor(dom, "cancelAnimationFrame", (...args: Array<empty>) =>
    win.cancelAnimationFrame(...args),
  );
  for (const name of OBJECTS) {
    const value = win[name];
    if (value !== undefined && globals[name] === undefined) {
      defineFor(dom, name, value);
    }
  }
  for (const name of STORAGE) {
    if (isUsableStorage(hostStorage(name))) {
      continue;
    }
    const value = win[name];
    if (isUsableStorage(value)) {
      defineFor(dom, name, value);
    }
  }

  // React reads these to decide it is in a browser and to pick its event
  // system, and they must be the objects the elements belong to.
  defineFor(dom, "window", win);
  defineFor(dom, "document", win.document);
  dom.applied = true;
}

/** [`define`], remembering what the global was before this module first set it. */
function defineFor(dom: Created, name: string, value: mixed): void {
  if (!dom.previous.has(name)) {
    dom.previous.set(name, Reflect.getOwnPropertyDescriptor(globalThis, name));
  }
  define(name, value);
}

/**
 * Where `@uniflowed/test`'s worker finds state other packages install for the
 * whole process, and how to put each back. See `restoreSharedState` in that
 * package's `internal/isolation.js`, which runs every entry before each file.
 *
 * A symbol from the global registry rather than an import: this package does
 * not depend on `@uniflowed/test`, and a project may render with it under
 * another runner, where nothing reads the entry and it costs nothing.
 */
const SHARED_STATE: symbol = Symbol.for("@uniflowed/test/shared-state");

/** Tell the worker, once, that this module has state for it to put back. */
function registerBetweenFiles(): void {
  let registry: mixed = Reflect.get(globalThis, SHARED_STATE);
  if (!(registry instanceof Map)) {
    registry = new Map();
    Reflect.defineProperty(globalThis, SHARED_STATE, {
      value: registry,
      writable: true,
      configurable: true,
      enumerable: false,
    });
  }
  registry.set("the window and document a render installed", restoreBetweenFiles);
}

/**
 * Hand the next file the process as it was before any file rendered.
 *
 * The window is shared by every file a worker runs, and a file changes it in
 * ways nothing else undoes. Found in this repository's own suite once
 * `uf test` started packing more files into each worker
 * (ubugeeei-prod/uf#944), each one naming the file that read rather than the
 * file that wrote:
 *
 * * `window.matchMedia = undefined`, which `npm/ui/ui.test.js` writes to
 *   say its document has no viewport, left every later component that asks a
 *   media query with `matchMedia is not a function`;
 * * `FormData`, which a render installs from the document because React's form
 *   actions build one from a form element — and whose document version refuses
 *   Node's `Blob`, so a file that never rendered failed to build a form it
 *   would have built in a fresh worker.
 *
 * So the window's own properties go back to the ones it was created with, the
 * attributes on `<html>` and `<head>` go the way the body's already do, and
 * each global goes back to what it was before this module defined it — which
 * for a file that never renders means no document at all, exactly as in a
 * worker that has not run one. The window object itself stays, and the next
 * render points the globals back at it.
 *
 * Two things are left where the last file put them, because other modules
 * keep state about them that outlives the file as well, and putting one half
 * back without the other is a new inconsistency rather than a fresh start:
 *
 * * the address and `history.state`, which the router keeps its own state in
 *   step with. Resetting them was tried on this repository's suite, run on one
 *   worker, and failed 31 cases across the router, dialogs, selects and tabs
 *   that pass without the reset;
 * * the head's elements. `@uniflowed/router` inserts nodes there and positions
 *   later ones against them (`internal/hydration.js`, `client.js`), and
 *   clearing the head fixed nothing in the same run.
 *
 * Event listeners a file added to the window are not reached either: a listener
 * is not an own property, and nothing short of a new window removes one.
 */
function restoreBetweenFiles(): void {
  const dom = created;
  let failure: mixed = null;
  if (dom != null && dom.applied) {
    // While the window is still installed. A root the last file left mounted
    // unmounts through React, and React reads `window` as it does — the next
    // file's own `cleanup()` met exactly that, before it had rendered anything.
    // A step that throws does not stop the rest: a half-restored process is the
    // defect this function exists to end.
    for (const step of beforeRestore) {
      try {
        step();
      } catch (thrown) {
        failure ??= thrown;
      }
    }
    const { win } = dom;
    const document: $FlowFixMe = win.document;
    for (const element of [document?.documentElement, document?.head]) {
      for (const name of [...(element?.getAttributeNames?.() ?? [])]) {
        element.removeAttribute(name);
      }
    }
    restoreOwnProperties(win, dom.pristine);
    for (const [name, descriptor] of dom.previous) {
      if (descriptor === undefined) {
        Reflect.deleteProperty(globalThis, name);
      } else {
        Object.defineProperty(globalThis, name, descriptor);
      }
    }
    dom.applied = false;
  }
  if (declared) {
    if (actFlagBefore === undefined) {
      Reflect.deleteProperty(globalThis, ACT_ENVIRONMENT);
    } else {
      Object.defineProperty(globalThis, ACT_ENVIRONMENT, actFlagBefore);
    }
    declared = false;
  }
  if (failure != null) {
    throw failure;
  }
}

/** What has to happen while the window is still installed, in the order it was asked for. */
const beforeRestore: Array<() => void> = [];

/**
 * Run `step` before the worker takes the window back, each time it does.
 *
 * For state another module of this package keeps about the window — the roots
 * `render` mounted — which has to be let go of through the window rather than
 * after it is gone. Asking twice with the same function asks once.
 */
export function beforeWindowRestore(step: () => void): void {
  if (!beforeRestore.includes(step)) {
    beforeRestore.push(step);
  }
}

/** Put `target`'s own properties back to `pristine`, adding, replacing and deleting. */
function restoreOwnProperties(
  target: HostWindow,
  pristine: Map<string | symbol, PropertyDescriptor<mixed>>,
): void {
  for (const key of Reflect.ownKeys(target)) {
    if (!pristine.has(key)) {
      Reflect.deleteProperty(target, key);
    }
  }
  for (const [key, descriptor] of pristine) {
    const now = Reflect.getOwnPropertyDescriptor(target, key);
    if (now == null || !sameDescriptor(now, descriptor)) {
      Object.defineProperty(target, key, descriptor);
    }
  }
}

/** Whether two descriptors describe the same property. */
function sameDescriptor(a: PropertyDescriptor<mixed>, b: PropertyDescriptor<mixed>): boolean {
  return (
    Object.is(a.value, b.value) &&
    a.get === b.get &&
    a.set === b.set &&
    a.writable === b.writable &&
    a.enumerable === b.enumerable &&
    a.configurable === b.configurable
  );
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
  declareActEnvironment(true);
}

/**
 * Turn the act environment on or off.
 *
 * `waitFor` stands it down for the length of a wait; see the reason there.
 */
export function setActEnvironment(active: boolean): void {
  declareActEnvironment(active);
}

/**
 * Set the flag, remembering what it was the first time this process — or this
 * file, after the worker put it back — set it.
 */
function declareActEnvironment(active: boolean): void {
  if (!declared) {
    actFlagBefore = Reflect.getOwnPropertyDescriptor(globalThis, ACT_ENVIRONMENT);
    registerBetweenFiles();
  }
  declared = true;
  define(ACT_ENVIRONMENT, active);
}

/**
 * The global React reads to decide whether updates outside `act` deserve a
 * warning.
 *
 * Named once, as a `string`, because every use is a property this module adds
 * to and removes from the global object by name. Flow checks a literal key
 * against `globalThis`'s declared members, and this one is React's, declared
 * nowhere.
 */
const ACT_ENVIRONMENT: string = "IS_REACT_ACT_ENVIRONMENT";

/** What `IS_REACT_ACT_ENVIRONMENT` was before this module set it. */
let actFlagBefore: PropertyDescriptor<mixed> | void = undefined;

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
