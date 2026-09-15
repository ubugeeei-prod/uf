// @flow
//
// The menubar `uf ui add menubar` writes: a named bar of triggers with one tab
// stop, the arrow keys moving along it, a trigger that opens and names its
// menu, and the dressing accessible closed and open.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { act, cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import { Example } from "./menubar.example.js";

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

describe("Menubar", () => {
  it("is a named bar with one tab stop", () => {
    render(<Example />);
    expect(screen.getByRole("menubar", { name: "Editor" })).toBeInTheDocument();
    const stops = screen
      .getAllByRole("menuitem")
      .filter((trigger) => trigger.getAttribute("tabindex") === "0");
    expect(stops).toHaveLength(1);
  });

  it("moves along the bar with the arrow keys", async () => {
    render(<Example />);
    const file = html(screen.getByRole("menuitem", { name: "File" }));
    act(() => {
      file.focus();
    });
    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getByRole("menuitem", { name: "Edit" })).toHaveFocus();
  });

  it("opens the menu its trigger controls", async () => {
    render(<Example />);
    const file = html(screen.getByRole("menuitem", { name: "File" }));
    expect(file).toHaveAttribute("aria-haspopup", "menu");
    await userEvent.click(file);
    expect(file).toHaveAttribute("aria-expanded", "true");
    // Followed through `aria-controls` rather than found by the name "File"
    // until #1060: a menubar's trigger does not name the menu it opens yet.
    expect(screen.getByRole("menu").id).toBe(file.getAttribute("aria-controls"));
    expect(screen.getByRole("menuitem", { name: "Open" })).toBeInTheDocument();
  });

  it("dresses the bar and its triggers", () => {
    render(<Example />);
    expect(screen.getByRole("menubar", { name: "Editor" }).getAttribute("class")).not.toBeNull();
    for (const trigger of screen.getAllByRole("menuitem")) {
      expect(trigger.getAttribute("class")).not.toBeNull();
    }
  });

  it("has no accessibility violations, closed or open", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
    await userEvent.click(html(screen.getByRole("menuitem", { name: "File" })));
    await expect(screen.getByRole("menu")).toHaveNoAxeViolations();
  });
});
