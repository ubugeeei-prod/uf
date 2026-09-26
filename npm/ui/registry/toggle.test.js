// @flow
//
// The toggle `uf ui add toggle` writes: pressed or not, flipped by a press, and
// named the same way in both states.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import { Example } from "./toggle.example.js";

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

describe("Toggle", () => {
  it("says whether it is pressed", () => {
    render(<Example />);
    expect(screen.getByRole("button", { name: "Bold" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "Italic" })).toHaveAttribute("aria-pressed", "false");
  });

  it("flips on a press, and keeps its name", async () => {
    render(<Example />);
    const hidden = html(screen.getByRole("button", { name: "Show hidden files" }));
    await userEvent.click(hidden);
    expect(hidden).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "Show hidden files" })).toBe(hidden);
  });

  it("draws every tone and size", () => {
    render(<Example />);
    for (const name of ["Bold", "Italic", "Show hidden files"]) {
      expect(screen.getByRole("button", { name }).getAttribute("class")).not.toBeNull();
    }
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
  });
});
