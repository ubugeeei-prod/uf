// @flow
//
// The collapsible `uf ui add collapsible` writes: a trigger that says whether
// its region is showing, a closed region that is still in the document, and
// the dressing accessible either way.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import { Example } from "./collapsible.example.js";

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

describe("Collapsible", () => {
  it("keeps a closed region in the document, hidden until it is found", () => {
    render(<Example />);
    const trigger = screen.getByRole("button", { name: "Show the 3 older replies" });
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    const region = document.getElementById(trigger.getAttribute("aria-controls") ?? "");
    expect(region).not.toBeNull();
    expect(region).toHaveAttribute("hidden");
  });

  it("shows the region on a press, and says so", async () => {
    render(<Example />);
    const trigger = html(screen.getByRole("button", { name: "Show the 3 older replies" }));
    await userEvent.click(trigger);
    expect(trigger).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText("Changed, thank you.")).toBeInTheDocument();
    const region = document.getElementById(trigger.getAttribute("aria-controls") ?? "");
    expect(region).not.toHaveAttribute("hidden");
  });

  it("dresses the trigger and the region", () => {
    render(<Example />);
    const trigger = screen.getByRole("button", { name: "Show the 3 older replies" });
    expect(trigger.getAttribute("class")).not.toBeNull();
    const region = document.getElementById(trigger.getAttribute("aria-controls") ?? "");
    expect(region?.getAttribute("class") ?? null).not.toBeNull();
  });

  it("has no accessibility violations, closed or open", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
    await userEvent.click(html(screen.getByRole("button", { name: "Show the 3 older replies" })));
    await expect(container).toHaveNoAxeViolations();
  });
});
