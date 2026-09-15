// @flow
//
// The toggle group `uf ui add toggle-group` writes, in both of its modes: a
// named group of pressed buttons, and a named radio group drawn as segments.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import { Example } from "./toggle-group.example.js";

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

describe("ToggleGroup", () => {
  it("is a named group of buttons that press independently", async () => {
    render(<Example />);
    expect(screen.getByRole("group", { name: "Formatting" })).toBeInTheDocument();
    const bold = html(screen.getByRole("button", { name: "Bold" }));
    const italic = html(screen.getByRole("button", { name: "Italic" }));
    expect(bold).toHaveAttribute("aria-pressed", "true");

    await userEvent.click(italic);
    expect(italic).toHaveAttribute("aria-pressed", "true");
    expect(bold).toHaveAttribute("aria-pressed", "true");
  });

  it("is a named radio group when it chooses one, and the arrows check as they move", async () => {
    render(<Example />);
    expect(screen.getByRole("radiogroup", { name: "Alignment" })).toBeInTheDocument();
    const left = html(screen.getByRole("radio", { name: "Left" }));
    expect(left).toHaveAttribute("aria-checked", "true");

    await userEvent.click(left);
    await userEvent.keyboard("{ArrowRight}");
    const centre = screen.getByRole("radio", { name: "Centre" });
    expect(centre).toHaveFocus();
    expect(centre).toHaveAttribute("aria-checked", "true");
    expect(left).toHaveAttribute("aria-checked", "false");
  });

  it("draws the rows and their segments", () => {
    render(<Example />);
    expect(screen.getByRole("group", { name: "Formatting" }).getAttribute("class")).not.toBeNull();
    expect(screen.getByRole("radio", { name: "Left" }).getAttribute("class")).not.toBeNull();
  });

  it("has no accessibility violations", async () => {
    render(<Example />);
    await expect(screen.getByRole("radiogroup", { name: "Alignment" })).toHaveNoAxeViolations();
    // `aria-allowed-attr` is off for the multiple-mode group alone: the part
    // writes `aria-orientation` on `role="group"`, which ARIA does not allow,
    // and that is `@uniflowed/ui`'s to fix, ubugeeei-prod/uf#1046. Every other
    // rule still runs over it.
    await expect(screen.getByRole("group", { name: "Formatting" })).toHaveNoAxeViolations({
      disabledRules: ["aria-allowed-attr"],
    });
  });
});
