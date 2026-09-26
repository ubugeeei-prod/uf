// @flow
//
// The radio group `uf ui add radio-group` writes: a named group with one tab
// stop, arrow keys that check as they move, and the value a form submits.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import { Example } from "./radio-group.example.js";

afterEach(() => {
  cleanup();
});

/** The element a query found, as the element `userEvent` presses. See #1017. */
function html(element: Element): HTMLElement {
  if (element instanceof HTMLElement) {
    return element;
  }
  throw new Error(`expected an HTML element, and found <${element.tagName.toLowerCase()}>`);
}

describe("RadioGroup", () => {
  it("is a named group whose chosen answer holds the one tab stop", () => {
    render(<Example />);
    expect(screen.getByRole("radiogroup", { name: "Plan" })).toBeInTheDocument();
    const team = screen.getByRole("radio", { name: "Team" });
    expect(team).toHaveAttribute("aria-checked", "true");
    expect(team).toHaveAttribute("tabindex", "0");
    expect(screen.getByRole("radio", { name: "Personal" })).toHaveAttribute("tabindex", "-1");
  });

  it("checks the answer the arrow keys move to", async () => {
    render(<Example />);
    await userEvent.click(html(screen.getByRole("radio", { name: "Team" })));
    await userEvent.keyboard("{ArrowUp}");
    const personal = screen.getByRole("radio", { name: "Personal" });
    expect(personal).toHaveFocus();
    expect(personal).toHaveAttribute("aria-checked", "true");
  });

  it("puts the chosen value where a form can submit it", async () => {
    const { container } = render(<Example />);
    expect(container.querySelector('input[name="plan"]')).toHaveAttribute("value", "team");
    await userEvent.click(html(screen.getByRole("radio", { name: "Personal" })));
    expect(container.querySelector('input[name="plan"]')).toHaveAttribute("value", "personal");
  });

  it("draws every answer", () => {
    render(<Example />);
    for (const name of ["Personal", "Team", "Enterprise"]) {
      expect(screen.getByRole("radio", { name }).getAttribute("class")).not.toBeNull();
    }
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
  });
});
