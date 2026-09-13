// @flow
//
// `@uniflowed/react-native-testing` queries React Native test trees without
// going through a DOM renderer.

import { describe, expect, it } from "@uniflowed/test";
import {
  NativeTestingUnsupportedError,
  accessibilityStateOf,
  accessibleName,
  createNativeScreen,
  render,
  roleOf,
  textContent,
  within,
} from "@uniflowed/react-native-testing";
import type { NativeElement } from "@uniflowed/react-native-testing";

const tree: NativeElement = {
  type: "View",
  props: { testID: "root" },
  children: [
    {
      type: "Text",
      props: {},
      children: ["Hello ", { type: "Text", props: {}, children: ["native"] }],
    },
    {
      type: "Pressable",
      props: {
        accessibilityLabel: "Save changes",
        accessibilityState: { disabled: true, busy: true },
        testID: "save",
      },
      children: [{ type: "Text", props: {}, children: ["Save"] }],
    },
    {
      type: "Switch",
      props: {
        accessibilityLabel: "Enabled",
        accessibilityState: { checked: "mixed", selected: true },
      },
      children: [],
    },
    {
      type: "TextInput",
      props: { accessibilityLabel: "Search", disabled: false },
      children: [],
    },
  ],
};

describe("@uniflowed/react-native-testing", () => {
  it("queries text from a React Native test tree", () => {
    const screen = createNativeScreen(tree);

    expect(screen.getByText("Hello native").type).toBe("Text");
    expect(screen.getByText("hello", { exact: false }).type).toBe("Text");
    expect(screen.queryByText("missing")).toBe(null);
  });

  it("queries roles and accessible names", () => {
    const screen = createNativeScreen(tree);

    expect(screen.getByRole("button", { name: "Save changes", disabled: true }).props?.testID).toBe(
      "save",
    );
    expect(screen.queryByRole("button", { disabled: false })).toBe(null);
    expect(screen.getByRole("textbox", { name: /Search/ }).type).toBe("TextInput");
    expect(screen.getByRole("switch", { checked: "mixed", selected: true }).type).toBe("Switch");
    expect(accessibleName(screen.getByTestId("save"))).toBe("Save changes");
    expect(accessibilityStateOf(screen.getByTestId("save"))).toEqual({
      busy: true,
      checked: false,
      disabled: true,
      expanded: false,
      selected: false,
    });
    expect(roleOf(screen.getByTestId("save"))).toBe("button");
  });

  it("scopes queries with within", () => {
    const root = createNativeScreen(tree).getByTestId("root");
    const scoped = within(root);

    expect(scoped.getAllByRole("text").length).toBe(3);
    expect(textContent(root)).toBe("Hello nativeSave");
  });

  it("accepts fragment-shaped root arrays from a native renderer", () => {
    const screen = createNativeScreen([
      { type: "Text", props: {}, children: ["First"] },
      { type: "Text", props: {}, children: ["Second"] },
    ]);

    expect(screen.getByText("First").type).toBe("Text");
    expect(screen.getByText("Second").type).toBe("Text");
  });

  it("refuses unknown native query options", () => {
    const screen = createNativeScreen(tree);
    let error = null;
    try {
      screen.getByRole("button", { pressed: true } as $FlowFixMe);
    } catch (caught) {
      error = caught;
    }

    expect(String(error?.message)).toContain('"pressed" is not an option this query takes');

    expect(() => screen.getByText("Hello native", { hidden: true } as $FlowFixMe)).toThrow(
      '"hidden" is not an option this query takes',
    );
  });

  it("refuses render until a native renderer exists", () => {
    let error = null;
    try {
      render();
    } catch (caught) {
      error = caught;
    }

    expect(error instanceof NativeTestingUnsupportedError).toBe(true);
    expect(String(error?.message)).toContain("native renderer and host config");
  });
});
