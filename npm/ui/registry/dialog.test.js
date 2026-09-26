// @flow
//
// The dialog `uf ui add dialog` writes. `@uniflowed/ui`'s own suite holds the
// behaviour; these hold that dressing it kept every piece of that behaviour
// reachable, and that the dressing itself is accessible.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import { Example } from "./dialog.example.js";

afterEach(() => {
  cleanup();
});

/**
 * The element a query found, as the element `userEvent` presses.
 *
 * `screen` answers with an `Element` and `userEvent` asks for an `HTMLElement`,
 * which is two halves of `@uniflowed/react-testing` disagreeing rather than
 * anything about this component: every control these tests press is an HTML
 * element, and this is the one place that is said.
 */
function html(element: Element): HTMLElement {
  if (element instanceof HTMLElement) {
    return element;
  }
  throw new Error(`expected an HTML element, and found <${element.tagName.toLowerCase()}>`);
}

describe("Dialog", () => {
  it("opens from its trigger, named by its title and described by its description", async () => {
    render(<Example />);
    await userEvent.click(html(screen.getByRole("button", { name: "Rename project" })));

    const dialog = screen.getByRole("dialog", { name: "Rename project" });
    expect(dialog).toHaveAttribute("aria-modal", "true");
    const described = dialog.getAttribute("aria-describedby") ?? "";
    expect(document.getElementById(described)).toHaveTextContent(
      "The new name is what everyone on the team sees.",
    );
  });

  it("moves focus to the task rather than to the close button in the corner", async () => {
    render(<Example />);
    await userEvent.click(html(screen.getByRole("button", { name: "Rename project" })));
    expect(screen.getByRole("textbox")).toHaveFocus();
  });

  it("closes on Escape and gives focus back to the trigger", async () => {
    render(<Example />);
    const trigger = html(screen.getByRole("button", { name: "Rename project" }));
    await userEvent.click(trigger);
    await userEvent.keyboard("{Escape}");

    expect(screen.queryByRole("dialog")).toBeNull();
    expect(trigger).toHaveFocus();
  });

  it("names the close button in the corner, and closes from it", async () => {
    render(<Example />);
    await userEvent.click(html(screen.getByRole("button", { name: "Rename project" })));
    await userEvent.click(html(screen.getByRole("button", { name: "Close" })));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("closes from an action in the footer", async () => {
    render(<Example />);
    await userEvent.click(html(screen.getByRole("button", { name: "Rename project" })));
    await userEvent.click(html(screen.getByRole("button", { name: "Save" })));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("dresses the trigger and the panel", async () => {
    render(<Example />);
    const trigger = html(screen.getByRole("button", { name: "Rename project" }));
    expect(trigger.getAttribute("class")).not.toBeNull();
    await userEvent.click(trigger);
    expect(screen.getByRole("dialog").getAttribute("class")).not.toBeNull();
  });

  it("has no accessibility violations, closed or open", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
    await userEvent.click(html(screen.getByRole("button", { name: "Rename project" })));
    await expect(screen.getByRole("dialog")).toHaveNoAxeViolations();
  });
});
