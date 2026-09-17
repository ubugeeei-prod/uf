// @flow
//
// The sidebar `uf ui add sidebar` writes: a named `<nav>` beside the page, the
// current item marked, a button that says whether the column is open, and
// items that keep their names once it collapses to icons, with the whole
// accessible open and collapsed.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { act, cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import { Example } from "./sidebar.example.js";

afterEach(() => {
  cleanup();
  answerMediaQueries(null);
});

/** The element a query found, as the element `userEvent` clicks. See #1017. */
function html(element: Element): HTMLElement {
  if (element instanceof HTMLElement) {
    return element;
  }
  throw new Error(`expected an HTML element, and found <${element.tagName.toLowerCase()}>`);
}

/** Answer every media query the same way, or stop answering them at all. */
function answerMediaQueries(matches: boolean | null): void {
  const host: $FlowFixMe = window;
  host.matchMedia =
    matches == null
      ? undefined
      : (query: string) => ({
          matches,
          media: query,
          addEventListener: () => {},
          removeEventListener: () => {},
        });
}

describe("Sidebar", () => {
  it("is a named navigation beside the page, open, with the current item marked", () => {
    render(<Example />);
    expect(screen.getByRole("navigation", { name: "Main" })).not.toHaveAttribute("data-collapsed");
    expect(screen.getByRole("button", { name: "Home" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("button", { name: "Toggle sidebar" })).toHaveAttribute(
      "aria-expanded",
      "true",
    );
  });

  it("collapses to icons from its button, and its items keep their names", async () => {
    render(<Example />);
    const toggle = html(screen.getByRole("button", { name: "Toggle sidebar" }));
    await userEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByRole("navigation", { name: "Main" })).toHaveAttribute(
      "data-collapsed",
      "true",
    );
    expect(screen.getByRole("button", { name: "Projects" })).toHaveAttribute(
      "aria-label",
      "Projects",
    );
  });

  it("dresses the column, its items and its button", () => {
    render(<Example />);
    expect(screen.getByRole("navigation", { name: "Main" }).getAttribute("class")).not.toBeNull();
    expect(screen.getByRole("button", { name: "Settings" }).getAttribute("class")).not.toBeNull();
    expect(
      screen.getByRole("button", { name: "Toggle sidebar" }).getAttribute("class"),
    ).not.toBeNull();
  });

  it("dresses the narrow panel and a collapsed tooltip", async () => {
    answerMediaQueries(true);
    render(<Example />);
    expect(screen.getByRole("dialog", { name: "Main" }).getAttribute("class")).not.toBeNull();

    cleanup();
    answerMediaQueries(null);
    render(<Example />);
    await userEvent.click(html(screen.getByRole("button", { name: "Toggle sidebar" })));
    act(() => html(screen.getByRole("button", { name: "Projects" })).focus());
    expect(screen.getByRole("tooltip").getAttribute("class")).not.toBeNull();
  });

  it("has no accessibility violations, open or collapsed", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
    await userEvent.click(html(screen.getByRole("button", { name: "Toggle sidebar" })));
    await expect(container).toHaveNoAxeViolations();
  });
});
