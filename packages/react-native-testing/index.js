// @flow
//
// `@uniflowed/react-native-testing`: query helpers for React Native test trees.
//
// The native renderer itself is still intentionally absent: `uf test` refuses a
// React Native application target rather than running it against a document.
// These helpers are the first piece around that renderer boundary. A host
// config, React Test Renderer, or app-owned harness can hand us the JSON tree it
// produced, and tests can query that tree without pretending a DOM exists.

export type NativeText = string | number;

export type NativeProps = {
  readonly accessibilityLabel?: ?string,
  readonly accessibilityRole?: ?string,
  readonly role?: ?string,
  readonly testID?: ?string,
  readonly [key: string]: mixed,
};

export type NativeElement = {
  readonly type: string,
  readonly props?: ?NativeProps,
  readonly children?: ?$ReadOnlyArray<NativeNode>,
};

export type NativeNode = null | void | boolean | NativeText | NativeElement;

export type NativeMatcher = string | RegExp | ((value: string, node: NativeElement) => boolean);

export type NativeQueryOptions = {
  readonly exact?: boolean,
};

export type NativeRoleOptions = {
  readonly name?: NativeMatcher,
  readonly exact?: boolean,
};

export type NativeQueries = {
  readonly getByText: (matcher: NativeMatcher, options?: NativeQueryOptions) => NativeElement,
  readonly queryByText: (
    matcher: NativeMatcher,
    options?: NativeQueryOptions,
  ) => NativeElement | null,
  readonly getAllByText: (
    matcher: NativeMatcher,
    options?: NativeQueryOptions,
  ) => $ReadOnlyArray<NativeElement>,
  readonly getByRole: (role: string, options?: NativeRoleOptions) => NativeElement,
  readonly queryByRole: (role: string, options?: NativeRoleOptions) => NativeElement | null,
  readonly getAllByRole: (
    role: string,
    options?: NativeRoleOptions,
  ) => $ReadOnlyArray<NativeElement>,
  readonly getByTestId: (testID: string) => NativeElement,
  readonly queryByTestId: (testID: string) => NativeElement | null,
  readonly getAllByTestId: (testID: string) => $ReadOnlyArray<NativeElement>,
};

export class NativeTestingUnsupportedError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "NativeTestingUnsupportedError";
  }
}

export function render(): empty {
  throw new NativeTestingUnsupportedError(
    "@uniflowed/react-native-testing cannot render React Native components yet: " +
      "uf still needs a native renderer and host config. Use createNativeScreen() " +
      "or within() with an existing native test tree.",
  );
}

export function createNativeScreen(root: NativeNode): NativeQueries {
  return within(root);
}

export function within(root: NativeNode): NativeQueries {
  return {
    getByText: (matcher, options) =>
      one(byText(root, matcher, options), "text", describeMatcher(matcher)),
    queryByText: (matcher, options) =>
      optional(byText(root, matcher, options), "text", describeMatcher(matcher)),
    getAllByText: (matcher, options) =>
      many(byText(root, matcher, options), "text", describeMatcher(matcher)),
    getByRole: (role, options) => one(byRole(root, role, options), "role", role),
    queryByRole: (role, options) => optional(byRole(root, role, options), "role", role),
    getAllByRole: (role, options) => many(byRole(root, role, options), "role", role),
    getByTestId: (testID) => one(byTestId(root, testID), "testID", testID),
    queryByTestId: (testID) => optional(byTestId(root, testID), "testID", testID),
    getAllByTestId: (testID) => many(byTestId(root, testID), "testID", testID),
  };
}

export function textContent(node: NativeNode): string {
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (!isElement(node)) return "";
  return (node.children ?? []).map((child) => textContent(child)).join("");
}

export function accessibleName(node: NativeElement): string {
  const label = node.props?.accessibilityLabel;
  return typeof label === "string" ? label : textContent(node);
}

export function roleOf(node: NativeElement): string | null {
  const explicit = node.props?.accessibilityRole ?? node.props?.role;
  if (typeof explicit === "string" && explicit !== "") return explicit;
  switch (node.type) {
    case "Button":
    case "Pressable":
      return "button";
    case "Image":
      return "image";
    case "Switch":
      return "switch";
    case "TextInput":
      return "textbox";
    case "Text":
      return "text";
    default:
      return null;
  }
}

function byText(
  root: NativeNode,
  matcher: NativeMatcher,
  options?: NativeQueryOptions,
): $ReadOnlyArray<NativeElement> {
  return allElements(root).filter((node) => {
    const value = textContent(node);
    if (value === "") return false;
    if (!matches(matcher, value, node, options?.exact ?? true)) return false;
    return !(node.children ?? []).some(
      (child) =>
        isElement(child) && matches(matcher, textContent(child), child, options?.exact ?? true),
    );
  });
}

function byRole(
  root: NativeNode,
  role: string,
  options?: NativeRoleOptions,
): $ReadOnlyArray<NativeElement> {
  return allElements(root).filter((node) => {
    if (roleOf(node) !== role) return false;
    return options?.name == null
      ? true
      : matches(options.name, accessibleName(node), node, options.exact ?? true);
  });
}

function byTestId(root: NativeNode, testID: string): $ReadOnlyArray<NativeElement> {
  return allElements(root).filter((node) => node.props?.testID === testID);
}

function allElements(root: NativeNode): Array<NativeElement> {
  const found = [];
  const visit = (node: NativeNode) => {
    if (!isElement(node)) return;
    found.push(node);
    for (const child of node.children ?? []) visit(child);
  };
  visit(root);
  return found;
}

function isElement(node: NativeNode): boolean %checks {
  return node !== null && typeof node === "object" && typeof node.type === "string";
}

function matches(
  matcher: NativeMatcher,
  value: string,
  node: NativeElement,
  exact: boolean,
): boolean {
  if (typeof matcher === "function") return matcher(value, node);
  if (matcher instanceof RegExp) return matcher.test(value);
  return exact ? value === matcher : value.toLowerCase().includes(matcher.toLowerCase());
}

function one(values: $ReadOnlyArray<NativeElement>, kind: string, label: string): NativeElement {
  if (values.length === 1) return values[0];
  if (values.length === 0) {
    throw new Error(`@uniflowed/react-native-testing: found nothing for ${kind} ${label}`);
  }
  throw new Error(
    `@uniflowed/react-native-testing: found ${String(values.length)} nodes for ${kind} ${label}`,
  );
}

function optional(
  values: $ReadOnlyArray<NativeElement>,
  kind: string,
  label: string,
): NativeElement | null {
  if (values.length === 0) return null;
  return one(values, kind, label);
}

function many(
  values: $ReadOnlyArray<NativeElement>,
  kind: string,
  label: string,
): $ReadOnlyArray<NativeElement> {
  if (values.length > 0) return values;
  throw new Error(`@uniflowed/react-native-testing: found nothing for ${kind} ${label}`);
}

function describeMatcher(matcher: NativeMatcher): string {
  if (typeof matcher === "string") return matcher;
  if (matcher instanceof RegExp) return String(matcher);
  return "<predicate>";
}
