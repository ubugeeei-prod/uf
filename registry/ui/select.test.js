// @flow
//
// The select `uf ui add select` writes. `@uniflowed/ui`'s own suite holds the
// keyboard map; these hold that dressing the parts kept it, that the cursor and
// the choice are drawn from the attributes the part writes, and that a form
// still receives the value.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import { Example } from "./select.example.js";

afterEach(() => {
  cleanup();
});

/**
 * The element a query found, as the element `userEvent` presses.
 *
 * `screen` answers with an `Element` and `userEvent` asks for an `HTMLElement`,
 * which is two halves of `@uniflowed/react-testing` disagreeing rather than
 * anything about this component: every control these tests press is an HTML
 * element, and this is the one place that is said.
 */
function html(element: Element): HTMLElement {
  if (element instanceof HTMLElement) {
    return element;
  }
  throw new Error(`expected an HTML element, and found <${element.tagName.toLowerCase()}>`);
}

describe("Select", () => {
  it("names the trigger after its label", () => {
    render(<Example />);
    expect(screen.getByRole("combobox", { name: "Country" })).toHaveTextContent("Choose a country");
  });

  it("opens a list named after the field, with its cursor on the first option", async () => {
    render(<Example />);
    const trigger = html(screen.getByRole("combobox", { name: "Country" }));
    await userEvent.click(trigger);

    expect(screen.getByRole("listbox", { name: "Country" })).toBeInTheDocument();
    const france = screen.getByRole("option", { name: "France" });
    expect(france).toHaveAttribute("data-active", "true");
    expect(trigger).toHaveAttribute("aria-activedescendant", france.id);
    expect(trigger).toHaveFocus();
  });

  it("moves the cursor with the arrow keys and takes an option on Enter", async () => {
    render(<Example />);
    const trigger = html(screen.getByRole("combobox", { name: "Country" }));
    await userEvent.click(trigger);
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("option", { name: "Germany" })).toHaveAttribute("data-active", "true");

    await userEvent.keyboard("{Enter}");
    expect(screen.queryByRole("listbox")).toBeNull();
    expect(trigger).toHaveTextContent("Germany");
    expect(trigger).toHaveFocus();
  });

  it("marks the chosen option when the list opens again", async () => {
    render(<Example />);
    const trigger = html(screen.getByRole("combobox", { name: "Country" }));
    await userEvent.click(trigger);
    await userEvent.click(html(screen.getByRole("option", { name: "Japan" })));
    await userEvent.click(trigger);
    expect(screen.getByRole("option", { name: "Japan" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("option", { name: "France" })).toHaveAttribute(
      "aria-selected",
      "false",
    );
  });

  it("puts the value, not the label, where a form can submit it", async () => {
    const { container } = render(<Example />);
    await userEvent.click(html(screen.getByRole("combobox", { name: "Country" })));
    await userEvent.click(html(screen.getByRole("option", { name: "United Kingdom" })));
    expect(container.querySelector('input[name="country"]')).toHaveAttribute("value", "gb");
  });

  it("closes on Escape without changing the value", async () => {
    render(<Example />);
    const trigger = html(screen.getByRole("combobox", { name: "Country" }));
    await userEvent.click(trigger);
    await userEvent.keyboard("{ArrowDown}");
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("listbox")).toBeNull();
    expect(trigger).toHaveTextContent("Choose a country");
  });

  it("dresses the trigger, the list and its options", async () => {
    render(<Example />);
    const trigger = html(screen.getByRole("combobox", { name: "Country" }));
    expect(trigger.getAttribute("class")).not.toBeNull();
    await userEvent.click(trigger);
    expect(screen.getByRole("listbox").getAttribute("class")).not.toBeNull();
    expect(screen.getByRole("option", { name: "France" }).getAttribute("class")).not.toBeNull();
  });

  it("has no accessibility violations, closed or open", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
    await userEvent.click(html(screen.getByRole("combobox", { name: "Country" })));
    await expect(container).toHaveNoAxeViolations();
  });
});
