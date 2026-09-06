// @flow
//
// Finding an element the way a person would.
//
// Every query takes a *matcher* — a string, a regular expression, or a
// predicate — and every one comes in four forms, because those are the four
// different questions a test asks:
//
//   getBy…     it is there now, and there is one. Anything else is a failure.
//   queryBy…   it may not be there, and its absence is the thing being asked.
//   findBy…    it will be there shortly. Waits.
//   getAllBy…  there are several, and how many matters.
//
// The distinction matters because `getBy` failing with "found none" is a much
// better test failure than `queryBy` returning null and the assertion failing
// three lines later on `null.textContent`.
//
// # Where a query looks
//
// `Element`. Every function here used to say `ParentNode`, which reads as the
// right name — "something with children to search" is exactly what a root is —
// and is not a type: the DOM specification has a `ParentNode` mixin, and Flow
// folds it into `Document`, `DocumentFragment` and `Element` as comments
// rather than declaring anything by that name. So it was twelve
// `cannot-resolve-name` errors between here and `internal/screen.js`, and an
// unresolvable name is `any`: `root` answered every question, which is why the
// casts below it existed at all.
//
// `Element` is what the two callers actually pass — the document's body, and
// an element a test already found — and it carries `querySelector`,
// `querySelectorAll` and `innerHTML`, which is everything this module asks of
// a root.

/** What a query will accept as a description of the thing to find. */
export type Matcher = string | RegExp | ((content: string, element: Element) => boolean);

/** How exactly a string matcher has to match. */
export type MatcherOptions = {|
  /** `false` matches a substring, case-insensitively. Defaults to `true`. */
  readonly exact?: boolean,
|};

/**
 * How to narrow a role query.
 *
 * A role is shared by every button on the page, so `name` is the option that
 * makes the query mean something: "the button called Save", which is how a
 * person would say it and what a screen reader announces.
 */
export type RoleOptions = {|
  /** The accessible name the element must have. */
  readonly name?: Matcher,
  /** `false` matches a substring of the name. Defaults to `true`. */
  readonly exact?: boolean,
|};

/**
 * Collapse whitespace the way a browser does when it lays text out.
 *
 * A test asks for "Save changes"; the markup may hold a newline and eleven
 * spaces between the two words because that is how the JSX was indented. The
 * reader sees one space, so the query matches one space.
 */
export function normalize(text: string): string {
  return text.replace(/\s+/g, " ").trim();
}

function matches(
  content: string,
  element: Element,
  matcher: Matcher,
  options?: MatcherOptions,
): boolean {
  if (typeof matcher === "function") {
    return matcher(content, element);
  }
  if (matcher instanceof RegExp) {
    return matcher.test(content);
  }
  const exact = options?.exact ?? true;
  return exact
    ? content === normalize(matcher)
    : content.toLowerCase().includes(normalize(matcher).toLowerCase());
}

/** The text a reader would see in this element, whitespace collapsed. */
export function textOf(element: Element): string {
  return normalize(element.textContent ?? "");
}

function candidates(root: Element, selector: string): Array<Element> {
  return Array.from(root.querySelectorAll(selector));
}

/** Elements whose own visible text matches. */
export function allByText(
  root: Element,
  matcher: Matcher,
  options?: MatcherOptions,
): Array<Element> {
  // Only the element closest to the text, not every ancestor that contains it:
  // asking for "Save" should find the button, not the button and the form and
  // the body.
  return candidates(root, "*").filter((element) => {
    if (!matches(textOf(element), element, matcher, options)) {
      return false;
    }
    return !Array.from(element.children).some((child) =>
      matches(textOf(child), child, matcher, options),
    );
  });
}

/** Elements with this ARIA role, whether written down or implied by the tag. */
export function allByRole(root: Element, role: string, options?: RoleOptions): Array<Element> {
  const found = candidates(root, "*").filter((element) => roleOf(element) === role);
  const name = options?.name;
  if (name == null) {
    return found;
  }
  return found.filter((element) =>
    matches(accessibleName(element), element, name, { exact: options?.exact ?? true }),
  );
}

/** Form controls labelled by this text. */
export function allByLabelText(
  root: Element,
  matcher: Matcher,
  options?: MatcherOptions,
): Array<Element> {
  const found = [];
  for (const label of candidates(root, "label")) {
    if (!matches(textOf(label), label, matcher, options)) {
      continue;
    }
    const control = controlFor(root, label);
    if (control != null) {
      found.push(control);
    }
  }
  // `aria-label` names a control with no label element of its own.
  for (const element of candidates(root, "[aria-label]")) {
    const label = element.getAttribute("aria-label") ?? "";
    if (matches(normalize(label), element, matcher, options) && !found.includes(element)) {
      found.push(element);
    }
  }
  return found;
}

/** Elements with this placeholder. */
export function allByPlaceholderText(
  root: Element,
  matcher: Matcher,
  options?: MatcherOptions,
): Array<Element> {
  return candidates(root, "[placeholder]").filter((element) =>
    matches(normalize(element.getAttribute("placeholder") ?? ""), element, matcher, options),
  );
}

/** Elements marked for tests, which is the query of last resort. */
export function allByTestId(
  root: Element,
  matcher: Matcher,
  options?: MatcherOptions,
): Array<Element> {
  return candidates(root, "[data-testid]").filter((element) =>
    matches(normalize(element.getAttribute("data-testid") ?? ""), element, matcher, options),
  );
}

/** Elements whose value matches, for inputs and selects. */
export function allByDisplayValue(
  root: Element,
  matcher: Matcher,
  options?: MatcherOptions,
): Array<Element> {
  return candidates(root, "input, textarea, select").filter((element) =>
    matches(normalize(displayValue(element) ?? ""), element, matcher, options),
  );
}

/**
 * The value a control is showing, or `null` for an element that has none.
 *
 * The three classes rather than `element.value`, because `value` is not a
 * property of `Element` — it belongs to each control class — and the selector
 * that produced this element is a string the checker cannot read. An
 * `instanceof` is the same fact stated where the checker can see it, and it is
 * true of the elements this is called with for the reason `internal/dom.js`
 * installs the document's own classes as the global ones: every element in the
 * document under test is an instance of them.
 *
 * A cast was the other answer, and it is what was here. `(element as any).value`
 * types this function's whole result as `any`, which then flows into
 * `normalize` and out through `accessibleName` — a published function whose
 * return type stopped being checked because of an expression three calls away.
 *
 * Exported so that `internal/events.js` asks the same question the same way:
 * typing into a control and finding a control by its value have to agree about
 * which elements have one, or `userEvent.type` would write a value that
 * `getByDisplayValue` could not then find.
 */
export function displayValue(element: Element): string | null {
  if (
    element instanceof HTMLInputElement ||
    element instanceof HTMLTextAreaElement ||
    element instanceof HTMLSelectElement
  ) {
    return element.value;
  }
  return null;
}

/**
 * The control a label labels.
 *
 * `for` first, because it is explicit; then a control nested inside the label,
 * which is the other way HTML allows it.
 */
function controlFor(root: Element, label: Element): Element | null {
  const id = label.getAttribute("for");
  if (id != null && id !== "") {
    const byId = root.querySelector(`#${cssEscape(id)}`);
    if (byId != null) {
      return byId;
    }
  }
  return label.querySelector("input, textarea, select, button, [role]");
}

/** Escape an id for use in a selector, since an id may contain anything. */
function cssEscape(value: string): string {
  return value.replace(/([^\w-])/g, "\\$1");
}

/** Roles a tag has without being told. */
const IMPLICIT_ROLES: { readonly [string]: string } = {
  a: "link",
  article: "article",
  aside: "complementary",
  button: "button",
  dialog: "dialog",
  footer: "contentinfo",
  form: "form",
  h1: "heading",
  h2: "heading",
  h3: "heading",
  h4: "heading",
  h5: "heading",
  h6: "heading",
  header: "banner",
  hr: "separator",
  img: "img",
  li: "listitem",
  main: "main",
  nav: "navigation",
  ol: "list",
  option: "option",
  progress: "progressbar",
  section: "region",
  select: "combobox",
  table: "table",
  tbody: "rowgroup",
  td: "cell",
  textarea: "textbox",
  th: "columnheader",
  tr: "row",
  ul: "list",
};

/** The input types that are not a textbox. */
const INPUT_ROLES: { readonly [string]: string } = {
  button: "button",
  checkbox: "checkbox",
  email: "textbox",
  image: "button",
  number: "spinbutton",
  radio: "radio",
  range: "slider",
  reset: "button",
  search: "searchbox",
  submit: "button",
  tel: "textbox",
  text: "textbox",
  url: "textbox",
};

/** This element's role: what it says, or what its tag implies. */
export function roleOf(element: Element): string | null {
  const explicit = element.getAttribute("role");
  if (explicit != null && explicit !== "") {
    return explicit.trim().split(/\s+/)[0];
  }
  const tag = element.tagName.toLowerCase();
  if (tag === "input") {
    const type = (element.getAttribute("type") ?? "text").toLowerCase();
    return INPUT_ROLES[type] ?? "textbox";
  }
  if (tag === "a" && element.getAttribute("href") == null) {
    // A link without a destination is not a link.
    return "generic";
  }
  if (tag === "th") {
    // A `<th>` is a `columnheader` or a `rowheader` depending on what it
    // heads, and `scope` is how the document says which. Mapping every `th`
    // to `columnheader` made `getByRole("rowheader")` find nothing in a table
    // of records — where every row has one, and where it is the cell that
    // makes a screen reader say "Ada Lovelace, 1815" instead of "1815".
    const scope = (element.getAttribute("scope") ?? "").toLowerCase();
    return scope === "row" || scope === "rowgroup" ? "rowheader" : "columnheader";
  }
  return IMPLICIT_ROLES[tag] ?? null;
}

/**
 * The name a screen reader would announce.
 *
 * `aria-label`, then the element `aria-labelledby` points at, then a label
 * element, then the element's own text. Not the whole specification — that is
 * a document of its own — but the order that decides almost every real case.
 */
export function accessibleName(element: Element): string {
  const label = element.getAttribute("aria-label");
  if (label != null && label !== "") {
    return normalize(label);
  }

  const labelledBy = element.getAttribute("aria-labelledby");
  if (labelledBy != null && labelledBy !== "") {
    // A loop rather than `.map().filter(Boolean).map()`: `filter(Boolean)`
    // removes the nulls at runtime and not from the type, so the second `map`
    // saw `HTMLElement | null` and the cast that hid it also hid whether
    // `textOf` was being handed an element at all.
    const parts = [];
    for (const id of labelledBy.split(/\s+/)) {
      const target = element.ownerDocument.getElementById(id);
      if (target != null) {
        parts.push(textOf(target));
      }
    }
    if (parts.length > 0) {
      return normalize(parts.join(" "));
    }
  }

  const id = element.getAttribute("id");
  if (id != null && id !== "") {
    const own = element.ownerDocument?.querySelector(`label[for="${cssEscape(id)}"]`);
    if (own != null) {
      return textOf(own);
    }
  }

  if (element.tagName.toLowerCase() === "input") {
    const type = (element.getAttribute("type") ?? "").toLowerCase();
    if (type === "submit" || type === "button" || type === "reset") {
      return normalize(displayValue(element) ?? "");
    }
  }

  return textOf(element);
}

/** Why a query failed, with enough of the DOM to see why. */
export function queryFailure(kind: string, matcher: Matcher, root: Element, found: number): Error {
  const description =
    typeof matcher === "function"
      ? "the given predicate"
      : matcher instanceof RegExp
        ? String(matcher)
        : JSON.stringify(matcher);
  const html = root.innerHTML;
  const shown = html.length > 2000 ? `${html.slice(0, 2000)}\n…` : html;
  const count = found === 0 ? "found nothing" : `found ${found} elements and needed exactly one`;
  return new Error(`${kind} ${description}: ${count}\n\n${shown}`);
}
