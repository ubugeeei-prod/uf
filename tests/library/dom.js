// @flow
//
// The narrowings a component test needs, in one place.
//
// A DOM query answers with the widest thing it can: `querySelector` returns
// `Element | null` because the selector may match nothing, `parentElement` is
// `Element | null` because the root element has no parent, and `document.body`
// is `HTMLBodyElement | null` because a parsed document need not have one. A test
// that then reads `.textContent` or `.value` has to say which of those cases it
// is in, and for a while the way it said so was `const output: any = …`.
//
// That is the wrong tool twice over. It turns off the checker for everything
// downstream of the binding — `output.textCntent` was not a mistake anybody
// would catch — and when the selector matches nothing the failure arrives as
// `Cannot read properties of null (reading 'textContent')` at a line about an
// assertion, naming neither the selector nor the element that is missing.
//
// So each narrowing is a function that says what it wanted and what it found.
// `@uniflowed/react-testing` deliberately does not export these: its queries
// are the ones a *person* would use — by role, by label, by text — and reaching
// for a tag name is the test saying it is checking the markup rather than the
// experience. That is legitimate here and is not an API this repository wants
// to bless.

/**
 * The one element `selector` matches inside `root`.
 *
 * Throws naming the selector, because a query that matched nothing is the
 * failure and every later line is a consequence of it.
 */
export function elementIn(root: Element, selector: string): Element {
  const found = root.querySelector(selector);
  if (found == null) {
    throw new Error(`no element matches \`${selector}\``);
  }
  return found;
}

/**
 * Every element `selector` matches inside `root`, in document order.
 *
 * Spread rather than `Array.from`, which Flow types as `Array<void>` for a
 * `NodeList` — the overload it picks is the array-like one, and a `NodeList`
 * has no numeric indexer for it to read.
 */
export function elementsIn(root: Element, selector: string): $ReadOnlyArray<Element> {
  return [...root.querySelectorAll(selector)];
}

/**
 * The element `element` is inside.
 *
 * Only the document's root element has no parent, and a test that renders into
 * a container is never holding it.
 */
export function parentOf(element: Element): Element {
  const parent = element.parentElement;
  if (parent == null) {
    throw new Error("the element has no parent");
  }
  return parent;
}

/**
 * The document's body.
 *
 * `@uniflowed/react-testing` installs a document on first render and that one
 * has a body; the throw is for a test that reaches here before rendering
 * anything, and it says so rather than leaving `null` to be read as a style.
 */
export function bodyOf(): HTMLElement {
  const body = globalThis.document.body;
  if (body == null) {
    throw new Error("the document has no <body>; render something first");
  }
  return body;
}

/** A control that has a `value`, which `Element` does not. */
type ValueControl = HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement;

/**
 * The control `element` is, so a test can read or write its `value`.
 *
 * `value` belongs to the three control classes rather than to `Element`, so
 * this is the `instanceof` a test would otherwise write inline — the same
 * narrowing `@uniflowed/react-testing`'s own queries do, for the same reason:
 * reading it off a cast types the whole expression as `any` and takes the rest
 * of the assertion with it.
 */
export function controlIn(element: Element): ValueControl {
  if (
    element instanceof HTMLInputElement ||
    element instanceof HTMLTextAreaElement ||
    element instanceof HTMLSelectElement
  ) {
    return element;
  }
  throw new Error(`<${element.tagName.toLowerCase()}> has no value`);
}

/** What a control currently holds. */
export function valueIn(element: Element): string {
  return controlIn(element).value;
}
