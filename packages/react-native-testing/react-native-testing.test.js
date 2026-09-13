// @flow
//
// `@uniflowed/react-native-testing` queries React Native test trees without
// going through a DOM renderer.

import { describe, expect, it } from "@uniflowed/test";
import {
  NativeTestingUnsupportedError,
  accessibleName,
  createNativeScreen,
  render,
  roleOf,
  textContent,
  within,
} from "@uniflowed/react-native-testing";

const tree = {
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
      props: { accessibilityLabel: "Save changes", testID: "save" },
      children: [{ type: "Text", props: {}, children: ["Save"] }],
    },
    {
      type: "TextInput",
      props: { accessibilityLabel: "Search" },
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

    expect(screen.getByRole("button", { name: "Save changes" }).props?.testID).toBe("save");
    expect(screen.getByRole("textbox", { name: /Search/ }).type).toBe("TextInput");
    expect(accessibleName(screen.getByTestId("save"))).toBe("Save changes");
    expect(roleOf(screen.getByTestId("save"))).toBe("button");
  });

  it("scopes queries with within", () => {
    const root = createNativeScreen(tree).getByTestId("root");
    const scoped = within(root);

    expect(scoped.getAllByRole("text").length).toBe(3);
    expect(textContent(root)).toBe("Hello nativeSave");
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
