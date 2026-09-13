// @flow
//
// `@uniflowed/react-native-testing` queries React Native test trees without
// going through a DOM renderer.

import { describe, expect, it } from "@uniflowed/test";
import {
  NativeTestingUnsupportedError,
  accessibilityStateOf,
  accessibilityValueOf,
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
      type: "View",
      props: {
        accessibilityLabel: "Upload",
        accessibilityRole: "progressbar",
        accessibilityValue: { min: 0, max: 100, now: 75, text: "75 percent" },
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
    expect(screen.getByRole("progressbar", { value: { now: 75, text: /percent/ } }).type).toBe(
      "View",
    );
    expect(screen.queryByRole("progressbar", { value: { now: 25 } })).toBe(null);
    expect(accessibleName(screen.getByTestId("save"))).toBe("Save changes");
    expect(accessibilityStateOf(screen.getByTestId("save"))).toEqual({
      busy: true,
      checked: false,
      disabled: true,
      expanded: false,
      selected: false,
    });
    expect(accessibilityValueOf(screen.getByRole("progressbar"))).toEqual({
      min: 0,
      max: 100,
      now: 75,
      text: "75 percent",
    });
    expect(roleOf(screen.getByTestId("save"))).toBe("button");
  });

  it("queries accessible labels directly", () => {
    const screen = createNativeScreen(tree);

    expect(screen.getByLabelText("Save changes").props?.testID).toBe("save");
    expect(screen.queryByLabelText("save")).toBe(null);
    expect(screen.getByLabelText("save", { exact: false }).props?.testID).toBe("save");
    expect(screen.getByLabelText("Hello native").type).toBe("Text");
    expect(screen.queryByLabelText("missing")).toBe(null);
    expect(
      screen.getByLabelText((name, node) => name === "Search" && node.type === "TextInput").type,
    ).toBe("TextInput");
  });

  it("uses accessibilityLabel before descendant text for label queries", () => {
    const screen = createNativeScreen({
      type: "Pressable",
      props: { accessibilityLabel: "Save changes", testID: "save" },
      children: ["Save"],
    });

    expect(screen.getByLabelText("Save changes").props?.testID).toBe("save");
    expect(screen.queryByLabelText("Save")).toBe(null);
  });

  it("uses text content as the label fallback for native controls", () => {
    const screen = createNativeScreen({
      type: "Pressable",
      props: { testID: "plain-action" },
      children: [{ type: "Text", props: {}, children: ["Plain action"] }],
    });

    expect(screen.getByLabelText("Plain action").props?.testID).toBe("plain-action");
  });

  it("does not duplicate a roleless container and its text child for label fallback", () => {
    const screen = createNativeScreen({
      type: "View",
      props: {},
      children: [{ type: "Text", props: {}, children: ["Plain label"] }],
    });

    const label = screen.getByLabelText("Plain label");
    expect(label.type).toBe("Text");
    expect(screen.getAllByLabelText("Plain label").length).toBe(1);
  });

  it("scopes queries with within", () => {
    const root = createNativeScreen(tree).getByTestId("root");
    const scoped = within(root);

    expect(scoped.getAllByRole("text").length).toBe(3);
    expect(scoped.getByLabelText("Search").type).toBe("TextInput");
    expect(textContent(root)).toBe("Hello nativeSave");
  });

  it("scopes label queries to the subtree passed to within", () => {
    const screen = createNativeScreen({
      type: "View",
      props: {},
      children: [
        {
          type: "View",
          props: { testID: "inside" },
          children: [
            {
              type: "Pressable",
              props: { accessibilityLabel: "Inside action" },
              children: [],
            },
          ],
        },
        {
          type: "Pressable",
          props: { accessibilityLabel: "Outside action" },
          children: [],
        },
      ],
    });
    const scoped = within(screen.getByTestId("inside"));

    expect(scoped.getByLabelText("Inside action").type).toBe("Pressable");
    expect(scoped.queryByLabelText("Outside action")).toBe(null);
  });

  it("accepts fragment-shaped root arrays from a native renderer", () => {
    const screen = createNativeScreen([
      { type: "Text", props: {}, children: ["First"] },
      { type: "Text", props: {}, children: ["Second"] },
    ]);

    expect(screen.getByText("First").type).toBe("Text");
    expect(screen.getByText("Second").type).toBe("Text");
  });

  it("uses the same one, optional and many rules for label queries", () => {
    const screen = createNativeScreen([
      {
        type: "Pressable",
        props: { accessibilityLabel: "Repeat" },
        children: [],
      },
      {
        type: "Pressable",
        props: { accessibilityLabel: "Repeat" },
        children: [],
      },
    ]);

    expect(screen.getAllByLabelText("Repeat").length).toBe(2);
    expect(() => screen.getByLabelText("Repeat")).toThrow("found 2 nodes for label Repeat");
    expect(() => screen.queryByLabelText("Repeat")).toThrow("found 2 nodes for label Repeat");
    expect(() => screen.getAllByLabelText("Missing")).toThrow("found nothing for label Missing");
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
    expect(() =>
      screen.getByRole("progressbar", { value: { percentage: 75 } } as $FlowFixMe),
    ).toThrow('"percentage" is not an option this query takes');
    expect(() => screen.getByRole("progressbar", { value: [] } as $FlowFixMe)).toThrow(
      "getByRole.value: an object was expected",
    );

    expect(() => screen.getByText("Hello native", { hidden: true } as $FlowFixMe)).toThrow(
      '"hidden" is not an option this query takes',
    );
    expect(() => screen.getByLabelText("Search", { selector: "TextInput" } as $FlowFixMe)).toThrow(
      '"selector" is not an option this query takes',
    );
    expect(() =>
      screen.queryByLabelText("Search", { selector: "TextInput" } as $FlowFixMe),
    ).toThrow('"selector" is not an option this query takes');
    expect(() =>
      screen.getAllByLabelText("Search", { selector: "TextInput" } as $FlowFixMe),
    ).toThrow('"selector" is not an option this query takes');
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
