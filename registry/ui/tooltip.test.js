// @flow
//
// The tooltip `uf ui add tooltip` writes. `@uniflowed/ui`'s own suite holds the
// timing; these hold that dressing it kept a tooltip a description of its
// trigger — waiting for the pointer, not for focus, gone on `Escape` — and that
// the dressing is accessible.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { act, cleanup, fireEvent, render, screen, userEvent } from "@uniflowed/react-testing";

import * as Tooltip from "./tooltip.js";
import { Example } from "./tooltip.example.js";

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

describe("Tooltip", () => {
  // How long it waits is `@uniflowed/ui`'s to hold, and its suite does; this
  // asks for no wait so that what is checked here is the dressing.
  it("opens for a pointer, and describes its trigger once it is there", () => {
    render(
      <Tooltip.Root openDelay={0}>
        <Tooltip.Trigger aria-label="Bold" size="icon" tone="ghost">
          B
        </Tooltip.Trigger>
        <Tooltip.Content>Bold (⌘B)</Tooltip.Content>
      </Tooltip.Root>,
    );
    const trigger = html(screen.getByRole("button", { name: "Bold" }));

    fireEvent.pointerEnter(trigger);
    const tip = screen.getByRole("tooltip");
    expect(tip).toHaveTextContent("Bold (⌘B)");
    expect(trigger.getAttribute("aria-describedby")).toBe(tip.id);
  });

  it("does not wait for focus, and is dismissed by Escape", async () => {
    render(<Example />);
    const trigger = html(screen.getByRole("button", { name: "Italic" }));
    act(() => {
      trigger.focus();
    });
    expect(screen.getByRole("tooltip")).toHaveTextContent("Italic (⌘I)");

    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("tooltip")).toBeNull();
    expect(trigger).not.toHaveAttribute("aria-describedby");
    expect(trigger).toHaveFocus();
  });

  it("keeps the trigger's own name, which is all a reader on a phone gets", () => {
    render(<Example />);
    expect(screen.getByRole("button", { name: "Bold" })).toHaveAttribute("aria-label", "Bold");
    expect(screen.queryByRole("tooltip")).toBeNull();
  });

  it("dresses the trigger and the phrase", () => {
    render(<Example />);
    const trigger = html(screen.getByRole("button", { name: "Bold" }));
    expect(trigger.getAttribute("class")).not.toBeNull();
    act(() => {
      trigger.focus();
    });
    expect(screen.getByRole("tooltip").getAttribute("class")).not.toBeNull();
  });

  it("has no accessibility violations, closed or open", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
    act(() => {
      html(screen.getByRole("button", { name: "Bold" })).focus();
    });
    expect(screen.getByRole("tooltip")).toBeInTheDocument();
    await expect(container).toHaveNoAxeViolations();
  });
});
