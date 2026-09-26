// @flow
//
// The popover `uf ui add popover` writes. `@uniflowed/ui`'s own suite holds the
// behaviour; these hold that dressing it kept a popover a popover — named by
// its trigger, focus in and back, `Tab` leaving — and that the dressing is
// accessible.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import { Example } from "./popover.example.js";

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

component Page() {
  return (
    <div>
      <Example />
      <button type="button">After</button>
    </div>
  );
}

describe("Popover", () => {
  it("is closed until its trigger opens it, and is named by that trigger", async () => {
    render(<Page />);
    expect(screen.queryByRole("dialog")).toBeNull();
    const trigger = html(screen.getByRole("button", { name: "Share" }));
    expect(trigger).toHaveAttribute("aria-expanded", "false");

    await userEvent.click(trigger);
    expect(trigger).toHaveAttribute("aria-expanded", "true");
    const popover = screen.getByRole("dialog", { name: "Share" });
    expect(popover).not.toHaveAttribute("aria-modal");
  });

  it("moves focus in, and gives it back on Escape", async () => {
    render(<Page />);
    const trigger = html(screen.getByRole("button", { name: "Share" }));
    await userEvent.click(trigger);
    expect(screen.getByRole("textbox", { name: "Link" })).toHaveFocus();

    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(trigger).toHaveFocus();
  });

  it("lets Tab leave, and closes behind it", async () => {
    render(<Page />);
    await userEvent.click(html(screen.getByRole("button", { name: "Share" })));
    html(screen.getByRole("button", { name: "Copy link" })).focus();

    await userEvent.tab();
    expect(screen.getByRole("button", { name: "After" })).toHaveFocus();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("dresses the trigger and the panel", async () => {
    render(<Page />);
    const trigger = html(screen.getByRole("button", { name: "Share" }));
    expect(trigger.getAttribute("class")).not.toBeNull();
    await userEvent.click(trigger);
    expect(screen.getByRole("dialog").getAttribute("class")).not.toBeNull();
  });

  it("has no accessibility violations, closed or open", async () => {
    const { container } = render(<Page />);
    await expect(container).toHaveNoAxeViolations();
    await userEvent.click(html(screen.getByRole("button", { name: "Share" })));
    await expect(container).toHaveNoAxeViolations();
  });
});
