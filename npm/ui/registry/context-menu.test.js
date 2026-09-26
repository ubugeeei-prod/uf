// @flow
//
// The context menu `uf ui add context-menu` writes: an area that says it has a
// menu, a named menu where it is right-clicked, rows dressed as `menu.js`
// dresses them, Escape closing it, and the whole accessible closed and open.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, fireEvent, render, screen, userEvent } from "@uniflowed/react-testing";

import { Example } from "./context-menu.example.js";

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

/** Render the example and right-click its area. */
function opened(): void {
  render(<Example />);
  fireEvent.contextMenu(screen.getByText("Right-click here"), { clientX: 40, clientY: 30 });
}

describe("ContextMenu", () => {
  it("says the area has a menu", () => {
    render(<Example />);
    expect(screen.getByText("Right-click here")).toHaveAttribute("aria-haspopup", "menu");
  });

  it("opens a named menu where the area is right-clicked", () => {
    opened();
    expect(screen.getByRole("menu", { name: "Page actions" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Forward" })).toHaveAttribute(
      "aria-disabled",
      "true",
    );
  });

  it("toggles a checkbox row and stays open", async () => {
    opened();
    const bookmarks = html(screen.getByRole("menuitemcheckbox", { name: "Show bookmarks" }));
    await userEvent.click(bookmarks);
    expect(bookmarks).toHaveAttribute("aria-checked", "false");
    expect(screen.getByRole("menu", { name: "Page actions" })).toBeInTheDocument();
  });

  it("closes on Escape", async () => {
    opened();
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("dresses the panel and its rows", () => {
    opened();
    expect(screen.getByRole("menu", { name: "Page actions" }).getAttribute("class")).not.toBeNull();
    expect(screen.getByRole("menuitem", { name: "Reload" }).getAttribute("class")).not.toBeNull();
  });

  it("has no accessibility violations, closed or open", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
    fireEvent.contextMenu(screen.getByText("Right-click here"), { clientX: 40, clientY: 30 });
    await expect(screen.getByRole("menu", { name: "Page actions" })).toHaveNoAxeViolations();
  });
});
