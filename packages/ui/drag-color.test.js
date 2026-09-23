// @flow
import * as React from "@uniflowed/react";
import { afterEach, expect, fn, it } from "@uniflowed/test";
import { act, cleanup, fireEvent, render, screen, userEvent } from "@uniflowed/react-testing";
import { ColorPicker, I18nProvider, ListBox, useDragAndDrop } from "./index.js";

afterEach(cleanup);
const items = [
  { key: "a", textValue: "Alpha" },
  { key: "b", textValue: "Beta" },
  { key: "c", textValue: "Gamma" },
];

it("reorders a collection by keyboard and lets Escape cancel", async () => {
  const reorder = fn();
  render(<ListBox items={items} aria-label="Order" onReorder={reorder} />);
  const list = screen.getByRole("listbox");
  list.focus();
  fireEvent.keyDown(list, { key: " ", ctrlKey: true });
  expect(screen.getByRole("status").textContent).toContain("Picked up Alpha");
  await userEvent.keyboard("{ArrowDown}");
  await userEvent.keyboard("{ArrowDown}");
  await userEvent.keyboard("{Enter}");
  expect(reorder).toHaveBeenLastCalledWith(["b", "a", "c"]);
  reorder.mockClear();
  fireEvent.keyDown(list, { key: " ", ctrlKey: true });
  await userEvent.keyboard("{Escape}");
  expect(reorder).not.toHaveBeenCalled();
  expect(screen.getByRole("status").textContent).toBe("Drag cancelled");
});
it("accepts the native drag data path and rejects malformed payloads", async () => {
  const reorder = fn();
  render(<ListBox items={items} aria-label="Order" onReorder={reorder} />);
  const data = new Map();
  const transfer = {
    types: ["application/x-uf-collection"],
    setData: (key, value) => data.set(key, value),
    getData: (key) => data.get(key) ?? "",
    effectAllowed: "",
    dropEffect: "",
  };
  const dispatch = async (element, name) => {
    const event = new Event(name, { bubbles: true, cancelable: true });
    Object.defineProperty(event, "dataTransfer", { value: transfer });
    await act(() => element.dispatchEvent(event));
  };
  await dispatch(screen.getByRole("option", { name: "Alpha" }), "dragstart");
  await dispatch(screen.getByRole("option", { name: "Gamma" }), "dragover");
  await dispatch(screen.getByRole("option", { name: "Gamma" }), "drop");
  expect(reorder).toHaveBeenLastCalledWith(["b", "a", "c"]);
  reorder.mockClear();
  data.set("application/x-uf-collection", "{}");
  await dispatch(screen.getByRole("option", { name: "Gamma" }), "drop");
  expect(reorder).not.toHaveBeenCalled();
});
component DropExample(onDrop: $FlowFixMe) {
  const drag = useDragAndDrop({ onDrop });
  return (
    <>
      <button {...drag.getDragProps("one", "One")}>One</button>
      <button {...drag.getDropProps("target")}>Target</button>
      <span role="status">{drag.announcement}</span>
    </>
  );
}
it("exposes keyboard drag hooks outside a collection", async () => {
  const drop = fn();
  render(<DropExample onDrop={drop} />);
  screen.getByRole("button", { name: "One" }).focus();
  await userEvent.keyboard(" ");
  await userEvent.tab();
  await userEvent.keyboard("{Enter}");
  expect(drop).toHaveBeenLastCalledWith({ keys: ["one"], target: "target" });
});
it("edits hex colors, preserves invalid text, and steps channels in RTL", async () => {
  const changed = fn();
  const { container } = render(
    <main>
      <h1>Palette</h1>
      <I18nProvider locale="ar">
        <ColorPicker.Root defaultValue="#00000080" onValueChange={changed}>
          <ColorPicker.Field aria-label="Color code" />
          <ColorPicker.Input aria-label="Choose color" />
          <ColorPicker.Channel channel="red" />
          <ColorPicker.Channel channel="alpha" />
          <ColorPicker.Swatch data-testid="swatch" />
        </ColorPicker.Root>
      </I18nProvider>
    </main>,
  );
  const red = screen.getByRole("slider", { name: "Red" });
  red.focus();
  await userEvent.keyboard("{ArrowLeft}");
  expect(changed).toHaveBeenLastCalledWith("#01000080");
  const field = screen.getByRole("textbox", { name: "Color code" });
  await userEvent.clear(field);
  await userEvent.type(field, "#f00");
  await userEvent.keyboard("{Enter}");
  expect(changed).toHaveBeenLastCalledWith("#ff0000");
  changed.mockClear();
  await userEvent.clear(field);
  await userEvent.type(field, "nonsense");
  await userEvent.keyboard("{Enter}");
  expect(field.getAttribute("aria-invalid")).toBe("true");
  expect(changed).not.toHaveBeenCalled();
  await expect(container).toHaveNoAxeViolations();
});
