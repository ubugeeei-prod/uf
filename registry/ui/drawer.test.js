// @flow
//
// The drawer `uf ui add drawer` writes. `@uniflowed/ui`'s own suite holds the
// behaviour; these hold that dressing it kept the handle a named, focusable
// slider over the snap points, that every drag has a way that is not a drag,
// and that the dressing is accessible.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import { Example } from "./drawer.example.js";

afterEach(() => {
  cleanup();
});

/**
 * The element a query found, as the element `userEvent` presses.
 *
 * `screen` answers with an `Element` and `userEvent` asks for an `HTMLElement`,
 * which is ubugeeei-prod/uf#1017 rather than anything about this component.
 */
function html(element: Element): HTMLElement {
  if (element instanceof HTMLElement) {
    return element;
  }
  throw new Error(`expected an HTML element, and found <${element.tagName.toLowerCase()}>`);
}

async function open(): Promise<HTMLElement> {
  render(<Example />);
  const trigger = html(screen.getByRole("button", { name: "Order details" }));
  await userEvent.click(trigger);
  return trigger;
}

describe("Drawer", () => {
  it("opens a modal dialog named by its title, against the bottom edge", async () => {
    await open();
    const drawer = screen.getByRole("dialog", { name: "Order 1024" });
    expect(drawer).toHaveAttribute("aria-modal", "true");
    expect(drawer).toHaveAttribute("data-side", "bottom");
  });

  it("moves focus to its content, which comes before the handle", async () => {
    await open();
    expect(screen.getByRole("button", { name: "Close" })).toHaveFocus();
  });

  it("gives its handle a name and every snap point the drag can reach", async () => {
    await open();
    const handle = html(screen.getByRole("slider", { name: "Resize the order details" }));
    expect(handle).toHaveAttribute("aria-valuenow", "0");
    expect(handle).toHaveAttribute("aria-valuetext", "50%");

    handle.focus();
    await userEvent.keyboard("{ArrowUp}");
    expect(handle).toHaveAttribute("aria-valuetext", "100%");
    await userEvent.keyboard("{Home}");
    expect(handle).toHaveAttribute("aria-valuenow", "0");
    await userEvent.keyboard("{End}");
    expect(handle).toHaveAttribute("aria-valuenow", "1");
  });

  it("closes from the keyboard at the smallest snap point, and gives focus back", async () => {
    const trigger = await open();
    html(screen.getByRole("slider", { name: "Resize the order details" })).focus();
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(trigger).toHaveFocus();
  });

  it("closes from a button, which is the drag's single-pointer alternative", async () => {
    await open();
    await userEvent.click(html(screen.getByRole("button", { name: "Close" })));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("dresses the panel and the handle", async () => {
    await open();
    expect(screen.getByRole("dialog").getAttribute("class")).not.toBeNull();
    expect(screen.getByRole("slider").getAttribute("class")).not.toBeNull();
  });

  it("has no accessibility violations, closed or open", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
    await userEvent.click(html(screen.getByRole("button", { name: "Order details" })));
    await expect(screen.getByRole("dialog")).toHaveNoAxeViolations();
  });
});
