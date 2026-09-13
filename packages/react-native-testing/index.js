// @flow
//
// `@uniflowed/react-native-testing`: query helpers for React Native test trees.
//
// The native renderer itself is still intentionally absent: `uf test` refuses a
// React Native application target rather than running it against a document.
// These helpers are the first piece around that renderer boundary. A host
// config, React Test Renderer, or app-owned harness can hand us the JSON tree it
// produced, and tests can query that tree without pretending a DOM exists.

import type * as React from "react";

import { loadTestRenderer } from "./internal/test-renderer.js";

export type NativeText = string | number;

export type NativeCheckedState = boolean | "mixed";

export type NativeAccessibilityState = {
  readonly busy?: ?boolean,
  readonly checked?: ?NativeCheckedState,
  readonly disabled?: ?boolean,
  readonly expanded?: ?boolean,
  readonly selected?: ?boolean,
  readonly [key: string]: mixed,
};

export type NativeAccessibilityValue = {
  readonly min?: ?number,
  readonly max?: ?number,
  readonly now?: ?number,
  readonly text?: ?string,
  readonly [key: string]: mixed,
};

export type NativeAccessibilityValueMatcher = {
  readonly min?: number,
  readonly max?: number,
  readonly now?: number,
  readonly text?: NativeMatcher,
};

export type NativeProps = {
  readonly accessibilityLabel?: ?string,
  readonly accessibilityRole?: ?string,
  readonly accessibilityState?: ?NativeAccessibilityState,
  readonly accessibilityValue?: ?NativeAccessibilityValue,
  readonly disabled?: ?boolean,
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

export type NativeTree = NativeNode | $ReadOnlyArray<NativeNode>;

export type NativeMatcher = string | RegExp | ((value: string, node: NativeElement) => boolean);

export type NativeQueryOptions = {
  readonly exact?: boolean,
};

export type NativeRoleOptions = {
  readonly name?: NativeMatcher,
  readonly exact?: boolean,
  readonly busy?: boolean,
  readonly checked?: NativeCheckedState,
  readonly disabled?: boolean,
  readonly expanded?: boolean,
  readonly selected?: boolean,
  readonly value?: NativeAccessibilityValueMatcher,
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
  readonly getByLabelText: (matcher: NativeMatcher, options?: NativeQueryOptions) => NativeElement,
  readonly queryByLabelText: (
    matcher: NativeMatcher,
    options?: NativeQueryOptions,
  ) => NativeElement | null,
  readonly getAllByLabelText: (
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

export type NativeRenderResult = {
  ...NativeQueries,
  readonly rerender: (ui: React.Node) => void,
  readonly unmount: () => void,
  readonly toJSON: () => NativeTree,
};

export class NativeTestingUnsupportedError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "NativeTestingUnsupportedError";
  }
}

export function render(ui: React.Node): NativeRenderResult {
  const testRenderer = requireTestRenderer();
  installReactActEnvironment();

  let renderer: ReactTestRendererInstance | null = null;
  testRenderer.act(() => {
    renderer = testRenderer.create(ui);
  });

  const currentTree = () => {
    const current = renderer;
    return current == null ? null : nativeTreeOf(current.toJSON());
  };

  return {
    ...liveWithin(currentTree),
    rerender: (next: React.Node) => {
      const current = renderer;
      if (current == null) {
        throw new Error("@uniflowed/react-native-testing: cannot rerender an unmounted tree");
      }
      testRenderer.act(() => {
        current.update(next);
      });
    },
    unmount: () => {
      const current = renderer;
      if (current == null) return;
      testRenderer.act(() => {
        current.unmount();
      });
      renderer = null;
    },
    toJSON: currentTree,
  };
}

export function createNativeScreen(root: NativeTree): NativeQueries {
  return within(root);
}

export function within(root: NativeTree): NativeQueries {
  return liveWithin(() => root);
}

function liveWithin(root: () => NativeTree): NativeQueries {
  return {
    getByText: (matcher, options) => {
      rejectUnknownOptions("getByText", options, NATIVE_QUERY_OPTION_KEYS);
      return one(byText(root(), matcher, options), "text", describeMatcher(matcher));
    },
    queryByText: (matcher, options) => {
      rejectUnknownOptions("queryByText", options, NATIVE_QUERY_OPTION_KEYS);
      return optional(byText(root(), matcher, options), "text", describeMatcher(matcher));
    },
    getAllByText: (matcher, options) => {
      rejectUnknownOptions("getAllByText", options, NATIVE_QUERY_OPTION_KEYS);
      return many(byText(root(), matcher, options), "text", describeMatcher(matcher));
    },
    getByLabelText: (matcher, options) => {
      rejectUnknownOptions("getByLabelText", options, NATIVE_QUERY_OPTION_KEYS);
      return one(byLabelText(root(), matcher, options), "label", describeMatcher(matcher));
    },
    queryByLabelText: (matcher, options) => {
      rejectUnknownOptions("queryByLabelText", options, NATIVE_QUERY_OPTION_KEYS);
      return optional(byLabelText(root(), matcher, options), "label", describeMatcher(matcher));
    },
    getAllByLabelText: (matcher, options) => {
      rejectUnknownOptions("getAllByLabelText", options, NATIVE_QUERY_OPTION_KEYS);
      return many(byLabelText(root(), matcher, options), "label", describeMatcher(matcher));
    },
    getByRole: (role, options) => {
      rejectUnknownRoleOptions("getByRole", options);
      return one(byRole(root(), role, options), "role", role);
    },
    queryByRole: (role, options) => {
      rejectUnknownRoleOptions("queryByRole", options);
      return optional(byRole(root(), role, options), "role", role);
    },
    getAllByRole: (role, options) => {
      rejectUnknownRoleOptions("getAllByRole", options);
      return many(byRole(root(), role, options), "role", role);
    },
    getByTestId: (testID) => one(byTestId(root(), testID), "testID", testID),
    queryByTestId: (testID) => optional(byTestId(root(), testID), "testID", testID),
    getAllByTestId: (testID) => many(byTestId(root(), testID), "testID", testID),
  };
}

const NATIVE_QUERY_OPTION_KEYS: $ReadOnlyArray<string> = ["exact"];
const NATIVE_ROLE_OPTION_KEYS: $ReadOnlyArray<string> = [
  "busy",
  "checked",
  "disabled",
  "exact",
  "expanded",
  "name",
  "selected",
  "value",
];
const NATIVE_ROLE_VALUE_OPTION_KEYS: $ReadOnlyArray<string> = ["max", "min", "now", "text"];

type ReactTestJSONNode =
  | string
  | {
      readonly type: string,
      readonly props: NativeProps,
      readonly children?: null | $ReadOnlyArray<ReactTestJSONNode>,
    };

type ReactTestJSONTree = null | ReactTestJSONNode | $ReadOnlyArray<ReactTestJSONNode>;

type ReactTestRendererInstance = {|
  readonly toJSON: () => ReactTestJSONTree,
  update(ui: React.Node): void,
  unmount(): void,
|};

type ReactTestRendererModule = {|
  readonly act: <T>(() => T) => T,
  readonly create: (ui: React.Node) => ReactTestRendererInstance,
|};

let testRendererModule: ReactTestRendererModule | null = null;

function requireTestRenderer(): ReactTestRendererModule {
  if (testRendererModule == null) {
    try {
      testRendererModule = assertTestRendererModule(loadTestRenderer());
    } catch (error) {
      if (isMissingTestRenderer(error)) {
        throw new NativeTestingUnsupportedError(
          "@uniflowed/react-native-testing render() needs the optional peer " +
            "`react-test-renderer`. Install the 19.3.x `react` and " +
            "`react-test-renderer` releases together, or pass an existing native test tree to " +
            "createNativeScreen().",
        );
      }
      throw error;
    }
  }
  return testRendererModule;
}

function assertTestRendererModule(value: mixed): ReactTestRendererModule {
  if (value == null || typeof value !== "object") {
    throw new NativeTestingUnsupportedError(
      "@uniflowed/react-native-testing expected react-test-renderer to export create() and act().",
    );
  }
  const module: $FlowFixMe = value;
  if (typeof module.create !== "function" || typeof module.act !== "function") {
    throw new NativeTestingUnsupportedError(
      "@uniflowed/react-native-testing expected react-test-renderer to export create() and act().",
    );
  }
  return module;
}

function isMissingTestRenderer(error: mixed): boolean {
  if (error == null || typeof error !== "object") return false;
  const moduleError: $FlowFixMe = error;
  return (
    moduleError.code === "MODULE_NOT_FOUND" &&
    typeof moduleError.message === "string" &&
    moduleError.message.includes("react-test-renderer")
  );
}

function installReactActEnvironment(): void {
  const global: $FlowFixMe = globalThis;
  global.IS_REACT_ACT_ENVIRONMENT = true;
}

function nativeTreeOf(tree: ReactTestJSONTree): NativeTree {
  if (Array.isArray(tree)) {
    return tree.map((child) => nativeNodeOf(child));
  }
  return nativeNodeOf(tree);
}

function nativeNodeOf(node: null | ReactTestJSONNode): NativeNode {
  if (node == null || typeof node === "string") {
    return node;
  }
  return {
    type: node.type,
    props: node.props,
    children: node.children == null ? [] : node.children.map((child) => nativeNodeOf(child)),
  };
}

export function textContent(node: NativeTree): string {
  if (Array.isArray(node)) return node.map((child) => textContent(child)).join("");
  if (typeof node === "string" || typeof node === "number") return String(node);
  const element = elementOf(node);
  if (element == null) return "";
  return (element.children ?? []).map((child) => textContent(child)).join("");
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

export function accessibilityStateOf(node: NativeElement): NativeAccessibilityState {
  const state = node.props?.accessibilityState ?? {};
  const disabled = state.disabled ?? node.props?.disabled;
  return {
    busy: state.busy ?? false,
    checked: state.checked ?? false,
    disabled: disabled ?? false,
    expanded: state.expanded ?? false,
    selected: state.selected ?? false,
  };
}

export function accessibilityValueOf(node: NativeElement): NativeAccessibilityValue {
  const value = node.props?.accessibilityValue ?? {};
  return {
    min: value.min ?? null,
    max: value.max ?? null,
    now: value.now ?? null,
    text: value.text ?? null,
  };
}

function rejectUnknownOptions(query: string, options: mixed, known: $ReadOnlyArray<string>): void {
  if (options == null) return;
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

function rejectUnknownRoleOptions(query: string, options: mixed): void {
  rejectUnknownOptions(query, options, NATIVE_ROLE_OPTION_KEYS);
  if (options == null || typeof options !== "object") return;
  const roleOptions: $FlowFixMe = options;
  if (Array.isArray(roleOptions.value)) {
    throw new Error(`${query}.value: an object was expected`);
  }
  rejectUnknownOptions(`${query}.value`, roleOptions.value, NATIVE_ROLE_VALUE_OPTION_KEYS);
}

function byText(
  root: NativeTree,
  matcher: NativeMatcher,
  options?: NativeQueryOptions,
): $ReadOnlyArray<NativeElement> {
  return allElements(root).filter((node) => {
    const value = textContent(node);
    if (value === "") return false;
    if (!matches(matcher, value, node, options?.exact ?? true)) return false;
    return !(node.children ?? []).some((child) => {
      const element = elementOf(child);
      return (
        element != null && matches(matcher, textContent(element), element, options?.exact ?? true)
      );
    });
  });
}

function byLabelText(
  root: NativeTree,
  matcher: NativeMatcher,
  options?: NativeQueryOptions,
): $ReadOnlyArray<NativeElement> {
  const exact = options?.exact ?? true;
  const found: Array<NativeElement> = [];
  const visit = (node: NativeTree, insideMatch: boolean) => {
    if (Array.isArray(node)) {
      for (const child of node) visit(child, insideMatch);
      return;
    }
    const element = elementOf(node);
    if (element == null) return;
    const value = accessibleName(element);
    const matched = value !== "" && matches(matcher, value, element, exact);
    const queryable = hasExplicitLabel(element) || roleOf(element) != null;
    const current = matched && queryable && !insideMatch;
    if (current) {
      found.push(element);
    }
    for (const child of element.children ?? []) visit(child, insideMatch || current);
  };
  visit(root, false);
  return found;
}

function byRole(
  root: NativeTree,
  role: string,
  options?: NativeRoleOptions,
): $ReadOnlyArray<NativeElement> {
  return allElements(root).filter((node) => {
    if (roleOf(node) !== role) return false;
    if (!stateMatches(node, options)) return false;
    if (!valueMatches(node, options?.value)) return false;
    return options?.name == null
      ? true
      : matches(options.name, accessibleName(node), node, options.exact ?? true);
  });
}

function byTestId(root: NativeTree, testID: string): $ReadOnlyArray<NativeElement> {
  return allElements(root).filter((node) => node.props?.testID === testID);
}

function hasExplicitLabel(node: NativeElement): boolean {
  const label = node.props?.accessibilityLabel;
  return typeof label === "string" && label !== "";
}

function allElements(root: NativeTree): Array<NativeElement> {
  const found: Array<NativeElement> = [];
  const visit = (node: NativeTree) => {
    if (Array.isArray(node)) {
      for (const child of node) visit(child);
      return;
    }
    const element = elementOf(node);
    if (element == null) return;
    found.push(element);
    for (const child of element.children ?? []) visit(child);
  };
  visit(root);
  return found;
}

function elementOf(node: NativeNode): NativeElement | null {
  if (node === null || typeof node !== "object") return null;
  const candidate: $FlowFixMe = node;
  return typeof candidate.type === "string" ? candidate : null;
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

function stateMatches(node: NativeElement, options?: NativeRoleOptions): boolean {
  if (options == null) return true;
  const state = accessibilityStateOf(node);
  return (
    (options.busy == null || state.busy === options.busy) &&
    (options.checked == null || state.checked === options.checked) &&
    (options.disabled == null || state.disabled === options.disabled) &&
    (options.expanded == null || state.expanded === options.expanded) &&
    (options.selected == null || state.selected === options.selected)
  );
}

function valueMatches(node: NativeElement, expected?: NativeAccessibilityValueMatcher): boolean {
  if (expected == null) return true;
  const value = accessibilityValueOf(node);
  return (
    (expected.min == null || value.min === expected.min) &&
    (expected.max == null || value.max === expected.max) &&
    (expected.now == null || value.now === expected.now) &&
    (expected.text == null ||
      (value.text != null && matches(expected.text, value.text, node, true)))
  );
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
