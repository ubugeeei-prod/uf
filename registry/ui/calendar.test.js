// @flow
//
// The calendar `uf ui add calendar` writes: a grid named by its month, the
// chosen day and today marked, one day in the tab order, a click choosing a day,
// a day that cannot be chosen marked so, paging to the next month, and the whole
// accessible.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import { Example } from "./calendar.example.js";

afterEach(() => {
  cleanup();
});

/** The element a query found, as the element `userEvent` clicks. See #1017. */
function html(element: Element): HTMLElement {
  if (element instanceof HTMLElement) {
    return element;
  }
  throw new Error(`expected an HTML element, and found <${element.tagName.toLowerCase()}>`);
}

describe("Calendar", () => {
  it("is a grid named by its month, with the chosen day and today marked", () => {
    render(<Example />);
    expect(screen.getByRole("grid", { name: "September 2026" })).toBeInTheDocument();
    const chosen = screen.getByRole("gridcell", { name: "14" });
    expect(chosen).toHaveAttribute("aria-selected", "true");
    expect(chosen).toHaveAttribute("aria-current", "date");
    expect(chosen).toHaveAttribute("tabindex", "0");
  });

  it("chooses a day on a click", async () => {
    render(<Example />);
    const fifteenth = html(screen.getByRole("gridcell", { name: "15" }));
    await userEvent.click(fifteenth);
    expect(fifteenth).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("gridcell", { name: "14" })).not.toHaveAttribute("aria-selected");
  });

  it("marks a day that cannot be chosen", () => {
    render(<Example />);
    expect(screen.getByRole("gridcell", { name: "19" })).toHaveAttribute("aria-disabled", "true");
  });

  it("pages to the next month from its button", async () => {
    render(<Example />);
    await userEvent.click(html(screen.getByRole("button", { name: "Next month" })));
    expect(screen.getByRole("grid", { name: "October 2026" })).toBeInTheDocument();
  });

  it("dresses the days and the month buttons", () => {
    render(<Example />);
    expect(screen.getByRole("gridcell", { name: "14" }).getAttribute("class")).not.toBeNull();
    expect(
      screen.getByRole("button", { name: "Previous month" }).getAttribute("class"),
    ).not.toBeNull();
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    // `empty-table-header` is left out until ubugeeei-prod/uf#1084: the part's
    // weekday headings hide their only text behind an `aria-label`.
    await expect(container).toHaveNoAxeViolations({ disabledRules: ["empty-table-header"] });
  });
});
