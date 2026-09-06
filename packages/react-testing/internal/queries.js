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
 * The keys of `MatcherOptions`, for the check that refuses the others.
 *
 * Written out beside the type rather than derived from it because Flow has no
 * way to produce one from the other: `$Keys` of an exact object is a type, and
 * this has to exist while the program runs. The two are short and adjacent so
 * that a reader can check them against each other by eye.
 */
export const MATCHER_OPTION_KEYS: $ReadOnlyArray<string> = ["exact"];

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
  /**
   * `true` also returns what the accessibility tree does not expose.
   *
   * Defaults to `false`, which is the question a role query is asking. The
   * other question — "is it in the document at all" — is a real one, and this
   * is how a test says it means that one.
   */
  readonly hidden?: boolean,
  /** The level a heading is announced at. Only a heading has one. */
  readonly level?: number,
  /**
   * What `aria-current` has to say: one of its tokens, `true`, or `false` for
   * the elements that are not the current one.
   */
  readonly current?: boolean | string,
|};

/** The keys of `RoleOptions`. See [`MATCHER_OPTION_KEYS`]. */
export const ROLE_OPTION_KEYS: $ReadOnlyArray<string> = [
  "current",
  "exact",
  "hidden",
  "level",
  "name",
];

/**
 * Raise unless every key of `options` is one this query takes.
 *
 * An option a query does not understand is the failure this module was worst
 * at: `getByRole("heading", { level: 3 })` read as an assertion about a
 * heading level, asserted nothing at all, and nothing anywhere said so. A
 * silently ignored option is worse than an unsupported one, because the test
 * that passes because of it is the test nobody looks at again.
 *
 * This is the half that can be right without a checker. The option types are
 * exact, so `uf check` will refuse the same key once ubugeeei-prod/uf#248
 * stops typing this package as `any` — and a test written against a published
 * build has no checker in the loop at all.
 */
export function rejectUnknownOptions(
  query: string,
  options: mixed,
  known: $ReadOnlyArray<string>,
): void {
  if (options == null) {
    return;
  }
  if (typeof options !== "object") {
    throw new Error(`${query}: the options are ${String(options)}, and an object was expected`);
  }
  for (const key of Object.keys(options)) {
    if (!known.includes(key)) {
      throw new Error(
        `${query}: "${key}" is not an option this query takes. It takes ${known.join(", ")}.`,
      );
    }
  }
}

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
  const level = options?.level;
  if (level != null && role !== "heading") {
    // Refused rather than ignored. An option a query accepts and does nothing
    // with turns a test into a decoration: it reads as if it checks the thing
    // it was written for and checks nothing.
    throw new Error(
      `getByRole("${role}", { level }): a level narrows a heading, and this query asked for "${role}"`,
    );
  }

  // Elements the accessibility tree does not expose are dropped, because a
  // role query asks what a reader is told and those are told to nobody: a
  // closed accordion panel, a `display: none` menu, the page behind an open
  // dialog. Returning them made "is this announced" unaskable, which is the
  // one thing the query is for.
  let found = candidates(root, "*").filter(
    (element) => roleOf(element) === role && (options?.hidden === true || exposed(element)),
  );

  if (level != null) {
    found = found.filter((element) => headingLevel(element) === level);
  }

  const current = options?.current;
  if (current != null) {
    found = found.filter((element) => currentOf(element) === current);
  }

  const name = options?.name;
  if (name == null) {
    return found;
  }
  return found.filter((element) =>
    matches(accessibleName(element), element, name, { exact: options?.exact ?? true }),
  );
}

/**
 * Whether the accessibility tree exposes this element.
 *
 * Up the ancestors, because each of these hides a subtree: an element under a
 * `display: none` parent is announced by nobody however plain its own style
 * is.
 *
 * `packages/test/internal/expect.js` carries a walk that looks like this one
 * and answers a different question. `toBeVisible` asks whether a reader would
 * *see* the element, so it counts `opacity: 0` as hidden — and a screen reader
 * announces an element at zero opacity, which is exactly why hiding text that
 * way is a bug rather than a technique. The two rules part company there, and
 * a role query wants this one. `aria-hidden` is the mirror image: the element
 * is on the screen and out of the tree.
 *
 * `hidden` is taken in every spelling, `hidden="until-found"` included. That
 * one applies `content-visibility: hidden`, whose subtree is not in the
 * accessibility tree; find-in-page being able to reach it does not make it
 * announced, and a closed accordion panel is the case that raised this.
 */
function exposed(element: Element): boolean {
  let child: Element | null = null;
  let current: Element | null = element;
  while (current != null) {
    if (current.hasAttribute("hidden") || current.getAttribute("aria-hidden") === "true") {
      return false;
    }
    // A closed `<details>` renders its summary and nothing else, so the
    // summary is still announced and everything beside it is not — the half
    // that a walk looking only at the ancestor gets wrong.
    if (
      child != null &&
      current.tagName.toLowerCase() === "details" &&
      !current.hasAttribute("open") &&
      child.tagName.toLowerCase() !== "summary"
    ) {
      return false;
    }
    const style = current.ownerDocument?.defaultView?.getComputedStyle?.(current);
    if (style != null && (style.display === "none" || style.visibility === "hidden")) {
      return false;
    }
    child = current;
    current = current.parentElement;
  }
  return true;
}

/**
 * The level a heading is announced at, or `null` when it has none.
 *
 * `aria-level` first, because the ARIA attribute overrides what the host
 * language implies: `<h2 aria-level="4">` is a level four heading. Then the
 * tag. Then two, which is what browsers fall back to when `role="heading"` is
 * written without the `aria-level` ARIA requires with it — an authoring
 * mistake, and a query has to answer the way a reader would be told rather
 * than the way the author meant.
 *
 * `null` for an `aria-level` that is not a whole number of at least one, which
 * is what ARIA says the value is. A level of "big" is not a level, and
 * matching nothing says so.
 */
function headingLevel(element: Element): number | null {
  const written = element.getAttribute("aria-level");
  if (written != null && written.trim() !== "") {
    const level = Number(written);
    return Number.isInteger(level) && level >= 1 ? level : null;
  }
  const tag = element.tagName.toLowerCase();
  return tag.length === 2 && tag[0] === "h" && tag[1] >= "1" && tag[1] <= "6" ? Number(tag[1]) : 2;
}

/** The tokens `aria-current` is defined for, beside `true` and `false`. */
const CURRENT_TOKENS = ["date", "location", "page", "step", "time"];

/**
 * What `aria-current` says about this element.
 *
 * Absent, empty and `"false"` are one answer — not current — which is why the
 * option's `false` has to mean "and carries no such attribute" rather than
 * "and the attribute says false". Every element on a page is not-current.
 *
 * A token nobody has heard of is `true`. That is ARIA's rule rather than a
 * guess: any value outside the list is treated as if `aria-current="true"` had
 * been written, not as the default `false`. So a misspelt `aria-current="pge"`
 * *is* announced as the current item, `{ current: true }` is the query that
 * finds it, and `{ current: "pge" }` finds nothing — which is the answer a
 * person looking for their typo needs.
 */
function currentOf(element: Element): boolean | string {
  const written = element.getAttribute("aria-current");
  if (written == null || written === "" || written === "false") {
    return false;
  }
  if (written === "true") {
    return true;
  }
  return CURRENT_TOKENS.includes(written) ? written : true;
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

  const naming = namingChild(element);
  if (naming != null) {
    return textOf(naming);
  }

  return textOf(element);
}

/** The child that names its parent, for the three elements HTML-AAM gives one. */
const NAMING_CHILDREN: { readonly [string]: string } = {
  fieldset: "legend",
  figure: "figcaption",
  table: "caption",
};

/**
 * The element that names this one from inside it, or `null`.
 *
 * A `<table>` is named by its `<caption>`, a `<fieldset>` by its `<legend>`
 * and a `<figure>` by its `<figcaption>`. HTML-AAM says so and every browser
 * does it, and without it "the element's own text" is what a table falls back
 * to — which for a table is every cell in it, so `getByRole("table", { name:
 * "People" })` was asking whether the name was `"People Name Born Ada Lovelace
 * 1815 …"` and finding nothing.
 *
 * A direct child, which is what the three rules say: the `<caption>` of a
 * table rather than of a table nested in one of its cells.
 */
function namingChild(element: Element): Element | null {
  const wanted = NAMING_CHILDREN[element.tagName.toLowerCase()];
  if (wanted == null) {
    return null;
  }
  return (
    Array.from(element.children).find((child) => child.tagName.toLowerCase() === wanted) ?? null
  );
}

/**
 * `error`, reported where the query was written rather than where it gave up.
 *
 * A `findBy…` polls, and the attempt whose failure it keeps is the last one —
 * which runs from a timer, with nothing of the test on the stack under it. The
 * error is the right error; only its position is missing, and a failure with
 * no position is the one thing `ubugeeei-prod/uf#319` was about. `asked` is an
 * error built at the call, so its frames are the caller's; rebuilding the
 * stack from the name and message is what the engine itself would have written
 * had the failure been raised there.
 *
 * `mixed` rather than `Error` because a wait rethrows whatever the body threw,
 * and a body may throw a string. One that is not an error carries no stack to
 * correct and is handed back untouched.
 */
export function atCallSite(error: mixed, asked: Error): mixed {
  if (!(error instanceof Error)) {
    return error;
  }
  const frames = (asked.stack ?? "").split("\n").slice(1);
  if (frames.length > 0) {
    error.stack = [`${error.name}: ${error.message}`, ...frames].join("\n");
  }
  return error;
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
