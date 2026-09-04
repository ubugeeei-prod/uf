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

/** What a query will accept as a description of the thing to find. */
export type Matcher = string | RegExp | ((content: string, element: Element) => boolean);

/** How exactly a string matcher has to match. */
export type MatcherOptions = {|
  /** `false` matches a substring, case-insensitively. Defaults to `true`. */
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

function candidates(root: ParentNode, selector: string): Array<Element> {
  return Array.from(root.querySelectorAll(selector));
}

/** Elements whose own visible text matches. */
export function allByText(
  root: ParentNode,
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
export function allByRole(
  root: ParentNode,
  role: string,
  options?: {| readonly name?: Matcher, readonly exact?: boolean |},
): Array<Element> {
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
  root: ParentNode,
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
  root: ParentNode,
  matcher: Matcher,
  options?: MatcherOptions,
): Array<Element> {
  return candidates(root, "[placeholder]").filter((element) =>
    matches(normalize(element.getAttribute("placeholder") ?? ""), element, matcher, options),
  );
}

/** Elements marked for tests, which is the query of last resort. */
export function allByTestId(
  root: ParentNode,
  matcher: Matcher,
  options?: MatcherOptions,
): Array<Element> {
  return candidates(root, "[data-testid]").filter((element) =>
    matches(normalize(element.getAttribute("data-testid") ?? ""), element, matcher, options),
  );
}

/** Elements whose value matches, for inputs and selects. */
export function allByDisplayValue(
  root: ParentNode,
  matcher: Matcher,
  options?: MatcherOptions,
): Array<Element> {
  return candidates(root, "input, textarea, select").filter((element) =>
    matches(normalize((element as any).value ?? ""), element, matcher, options),
  );
}

/**
 * The control a label labels.
 *
 * `for` first, because it is explicit; then a control nested inside the label,
 * which is the other way HTML allows it.
 */
function controlFor(root: ParentNode, label: Element): Element | null {
  const id = label.getAttribute("for");
  if (id != null && id !== "") {
    const byId = (root as any).querySelector?.(`#${cssEscape(id)}`);
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
    const parts = labelledBy
      .split(/\s+/)
      .map((id) => element.ownerDocument?.getElementById(id))
      .filter(Boolean)
      .map((target) => textOf(target as any));
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
      return normalize((element as any).value ?? "");
    }
  }

  return textOf(element);
}

/** Why a query failed, with enough of the DOM to see why. */
export function queryFailure(
  kind: string,
  matcher: Matcher,
  root: ParentNode,
  found: number,
): Error {
  const description =
    typeof matcher === "function"
      ? "the given predicate"
      : matcher instanceof RegExp
        ? String(matcher)
        : JSON.stringify(matcher);
  const html = (root as any).innerHTML ?? "";
  const shown = html.length > 2000 ? `${html.slice(0, 2000)}\n…` : html;
  const count = found === 0 ? "found nothing" : `found ${found} elements and needed exactly one`;
  return new Error(`${kind} ${description}: ${count}\n\n${shown}`);
}
