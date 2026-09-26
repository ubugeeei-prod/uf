// @flow
//
// The combobox `uf ui add combobox` writes: a named field whose focus never
// leaves it, a cursor drawn from `data-active`, an option taken on `Enter`, a
// count a reader hears, and the dressing accessible closed and open.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import { Example } from "./combobox.example.js";

afterEach(() => {
  cleanup();
});

/** The element a query found, as the element `userEvent` types into. See #1017. */
function html(element: Element): HTMLElement {
  if (element instanceof HTMLElement) {
    return element;
  }
  throw new Error(`expected an HTML element, and found <${element.tagName.toLowerCase()}>`);
}

describe("Combobox", () => {
  it("is a field named by its label", () => {
    render(<Example />);
    expect(screen.getByRole("combobox", { name: "Country" })).toBeInTheDocument();
  });

  it("lists what the page matched, and moves a cursor without moving focus", async () => {
    render(<Example />);
    const field = html(screen.getByRole("combobox", { name: "Country" }));
    await userEvent.type(field, "an");
    expect(screen.getByRole("option", { name: "France" })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: "Kenya" })).toBeNull();

    await userEvent.keyboard("{ArrowDown}");
    const first = screen.getByRole("option", { name: "France" });
    expect(first).toHaveAttribute("data-active", "true");
    expect(field).toHaveAttribute("aria-activedescendant", first.id);
    expect(field).toHaveFocus();
  });

  it("takes the option under the cursor on Enter", async () => {
    render(<Example />);
    const field = html(screen.getByRole("combobox", { name: "Country" }));
    await userEvent.type(field, "jap");
    await userEvent.keyboard("{ArrowDown}");
    await userEvent.keyboard("{Enter}");
    expect(field).toHaveAttribute("aria-expanded", "false");
    expect((field as $FlowFixMe).value).toBe("Japan");
  });

  it("counts the results for a reader who cannot see them", async () => {
    render(<Example />);
    await userEvent.type(html(screen.getByRole("combobox", { name: "Country" })), "an");
    expect(screen.getByRole("status")).toHaveTextContent("3 results available.");
  });

  it("dresses the field, the list and its options", async () => {
    render(<Example />);
    const field = html(screen.getByRole("combobox", { name: "Country" }));
    expect(field.getAttribute("class")).not.toBeNull();
    await userEvent.type(field, "a");
    expect(screen.getByRole("listbox").getAttribute("class")).not.toBeNull();
    expect(screen.getByRole("option", { name: "France" }).getAttribute("class")).not.toBeNull();
  });

  it("has no accessibility violations, closed or open", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
    await userEvent.type(html(screen.getByRole("combobox", { name: "Country" })), "an");
    await expect(container).toHaveNoAxeViolations();
  });
});
