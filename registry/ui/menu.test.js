// @flow
//
// The menu `uf ui add menu` writes: a trigger that opens a menu it names,
// checkbox and radio rows that say what they are, a disabled row, a shortcut
// kept out of a row's name, Escape giving focus back, and the dressing
// accessible closed and open.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import { Example } from "./menu.example.js";

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

/** Render the example and open its menu. */
async function opened(): Promise<HTMLElement> {
  render(<Example />);
  const trigger = html(screen.getByRole("button", { name: "File" }));
  await userEvent.click(trigger);
  return trigger;
}

describe("Menu", () => {
  it("opens a menu its trigger names", async () => {
    const trigger = await opened();
    expect(trigger).toHaveAttribute("aria-haspopup", "menu");
    expect(trigger).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("menu", { name: "File" })).toBeInTheDocument();
  });

  it("hides a row's drawn shortcut, and says its keys with aria-keyshortcuts", async () => {
    await opened();
    // Found by `aria-keyshortcuts` rather than by name until #1059: the query
    // counts the hidden shortcut into the row's name, and a screen reader does not.
    const row = screen
      .getAllByRole("menuitem")
      .find((each) => each.getAttribute("aria-keyshortcuts") === "Control+N");
    expect(row?.textContent).toContain("New file");
    expect(row?.querySelector('[aria-hidden="true"]')?.textContent).toBe("Ctrl N");
  });

  it("says which rows are checked and chosen", async () => {
    await opened();
    const ruler = html(screen.getByRole("menuitemcheckbox", { name: "Show ruler" }));
    expect(ruler).toHaveAttribute("aria-checked", "true");
    await userEvent.click(ruler);
    expect(ruler).toHaveAttribute("aria-checked", "false");

    const compact = html(screen.getByRole("menuitemradio", { name: "Compact" }));
    await userEvent.click(compact);
    expect(compact).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("menuitemradio", { name: "Comfortable" })).toHaveAttribute(
      "aria-checked",
      "false",
    );
  });

  it("marks a disabled row", async () => {
    await opened();
    expect(screen.getByRole("menuitem", { name: "Save" })).toHaveAttribute("aria-disabled", "true");
  });

  it("closes on Escape and gives focus back to its trigger", async () => {
    const trigger = await opened();
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();
    expect(trigger).toHaveFocus();
  });

  it("dresses the panel and its rows", async () => {
    await opened();
    expect(screen.getByRole("menu", { name: "File" }).getAttribute("class")).not.toBeNull();
    expect(screen.getByRole("menuitem", { name: "Save" }).getAttribute("class")).not.toBeNull();
  });

  it("has no accessibility violations, closed or open", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
    await userEvent.click(html(screen.getByRole("button", { name: "File" })));
    await expect(screen.getByRole("menu", { name: "File" })).toHaveNoAxeViolations();
  });
});
