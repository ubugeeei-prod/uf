// @flow
//
// The checkbox `uf ui add checkbox` writes: named by its words, checked,
// unchecked or mixed, toggled by a press, and still a checkbox while disabled.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import { Checkbox } from "./checkbox.js";
import { Example } from "./checkbox.example.js";

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

describe("Checkbox", () => {
  it("says checked, unchecked or mixed, named by its words", () => {
    render(<Example />);
    expect(screen.getByRole("checkbox", { name: "Every project" })).toHaveAttribute(
      "aria-checked",
      "mixed",
    );
    expect(screen.getByRole("checkbox", { name: "Include archived projects" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    expect(screen.getByRole("checkbox", { name: "Only projects I own" })).toHaveAttribute(
      "aria-checked",
      "false",
    );
  });

  it("toggles on a press", async () => {
    render(<Example />);
    const mine = html(screen.getByRole("checkbox", { name: "Only projects I own" }));
    await userEvent.click(mine);
    expect(mine).toHaveAttribute("aria-checked", "true");
  });

  it("reports a press on a mixed one as checked, and leaves mixed to its owner", async () => {
    let reported: boolean | null = null;
    render(
      <Checkbox
        indeterminate
        onCheckedChange={(next) => {
          reported = next;
        }}
      >
        Every project
      </Checkbox>,
    );
    const every = html(screen.getByRole("checkbox", { name: "Every project" }));
    await userEvent.click(every);
    expect(reported).toBe(true);
    expect(every).toHaveAttribute("aria-checked", "mixed");
  });

  it("does not toggle while it is disabled", async () => {
    render(<Example />);
    const deleted = html(screen.getByRole("checkbox", { name: "Include deleted projects" }));
    await userEvent.click(deleted);
    expect(deleted).toHaveAttribute("aria-checked", "false");
  });

  it("draws the checkbox", () => {
    render(<Example />);
    expect(
      screen.getByRole("checkbox", { name: "Only projects I own" }).getAttribute("class"),
    ).not.toBeNull();
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
  });
});
