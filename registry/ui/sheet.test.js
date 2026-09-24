// @flow
//
// The sheet `uf ui add sheet` writes. `@uniflowed/ui`'s own suite holds the
// behaviour; these hold that dressing it kept every piece of it reachable, that
// the edge the panel is drawn against is the edge the part reports, and that
// the dressing is accessible.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen, userEvent } from "@uniflowed/react-testing";
import type { Edge } from "@uniflowed/ui";

import * as Sheet from "./sheet.js";
import { Example } from "./sheet.example.js";

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

describe("Sheet", () => {
  it("opens a modal dialog named by its title, against the edge it was given", async () => {
    render(<Example />);
    await userEvent.click(html(screen.getByRole("button", { name: "Filters" })));

    const sheet = screen.getByRole("dialog", { name: "Filters" });
    expect(sheet).toHaveAttribute("aria-modal", "true");
    expect(sheet).toHaveAttribute("data-side", "right");
  });

  it("moves focus to its content rather than to the close button in the corner", async () => {
    render(<Example />);
    await userEvent.click(html(screen.getByRole("button", { name: "Filters" })));
    expect(screen.getByRole("checkbox", { name: "Only open issues" })).toHaveFocus();
  });

  it("closes on Escape and gives focus back to the trigger", async () => {
    render(<Example />);
    const trigger = html(screen.getByRole("button", { name: "Filters" }));
    await userEvent.click(trigger);
    await userEvent.keyboard("{Escape}");

    expect(screen.queryByRole("dialog")).toBeNull();
    expect(trigger).toHaveFocus();
  });

  it("closes from the named button in the corner and from its footer", async () => {
    render(<Example />);
    const trigger = html(screen.getByRole("button", { name: "Filters" }));
    await userEvent.click(trigger);
    await userEvent.click(html(screen.getByRole("button", { name: "Close" })));
    expect(screen.queryByRole("dialog")).toBeNull();

    await userEvent.click(trigger);
    await userEvent.click(html(screen.getByRole("button", { name: "Show results" })));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("reports every edge it can be drawn against", async () => {
    const sides: $ReadOnlyArray<Edge> = ["top", "right", "bottom", "left"];
    for (const side of sides) {
      render(
        <Sheet.Root side={side}>
          <Sheet.Trigger>Open</Sheet.Trigger>
          <Sheet.Content>
            <Sheet.Title>Panel</Sheet.Title>
          </Sheet.Content>
        </Sheet.Root>,
      );
      await userEvent.click(html(screen.getByRole("button", { name: "Open" })));
      expect(screen.getByRole("dialog", { name: "Panel" })).toHaveAttribute("data-side", side);
      cleanup();
    }
  });

  it("has no accessibility violations, closed or open", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
    await userEvent.click(html(screen.getByRole("button", { name: "Filters" })));
    await expect(screen.getByRole("dialog")).toHaveNoAxeViolations();
  });
});
