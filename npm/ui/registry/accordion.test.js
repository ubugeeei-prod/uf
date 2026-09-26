// @flow
//
// The accordion `uf ui add accordion` writes: real headings holding buttons,
// panels named by them, one section open at a time, and the dressing
// accessible.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import { Example } from "./accordion.example.js";

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

describe("Accordion", () => {
  it("puts each trigger inside a heading at the level it was given", () => {
    render(<Example />);
    const trigger = screen.getByRole("button", { name: "How long does shipping take?" });
    expect(trigger.parentElement?.tagName).toBe("H3");
    expect(trigger).toHaveAttribute("aria-expanded", "true");
  });

  it("names each open panel after its trigger", () => {
    render(<Example />);
    expect(screen.getByRole("region", { name: "How long does shipping take?" })).toHaveTextContent(
      "Two working days within the country, five abroad.",
    );
  });

  it("opens one section and closes the other", async () => {
    render(<Example />);
    const shipping = html(screen.getByRole("button", { name: "How long does shipping take?" }));
    const returns = html(screen.getByRole("button", { name: "Can I return an order?" }));
    await userEvent.click(returns);
    expect(returns).toHaveAttribute("aria-expanded", "true");
    expect(shipping).toHaveAttribute("aria-expanded", "false");
  });

  it("dresses every section", () => {
    render(<Example />);
    for (const name of ["How long does shipping take?", "Can I return an order?"]) {
      expect(screen.getByRole("button", { name }).getAttribute("class")).not.toBeNull();
    }
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
  });
});
