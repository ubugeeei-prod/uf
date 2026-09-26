// @flow
//
// The alert dialog `uf ui add alert-dialog` writes. `@uniflowed/ui`'s own suite
// holds the behaviour; these hold that dressing it kept every piece of it
// reachable, and that the dressing is accessible.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, fireEvent, render, screen, userEvent } from "@uniflowed/react-testing";

import { Example } from "./alert-dialog.example.js";

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
  const trigger = html(screen.getByRole("button", { name: "Delete project" }));
  await userEvent.click(trigger);
  return trigger;
}

describe("AlertDialog", () => {
  it("announces an alert dialog, named by its question and described by its cost", async () => {
    await open();
    const dialog = screen.getByRole("alertdialog", { name: "Delete this project?" });
    expect(dialog).toHaveAttribute("aria-modal", "true");
    const described = dialog.getAttribute("aria-describedby") ?? "";
    expect(document.getElementById(described)).toHaveTextContent(
      "Its pages, its history and its settings go with it, and this cannot be undone.",
    );
  });

  it("starts focus on the answer that does least", async () => {
    await open();
    expect(screen.getByRole("button", { name: "Keep it" })).toHaveFocus();
    expect(screen.getByRole("button", { name: "Delete" })).not.toHaveFocus();
  });

  it("stays open for a press beside it, and declines on Escape", async () => {
    const trigger = await open();
    const page = document.body;
    if (page == null) {
      throw new Error("the test document has no body");
    }
    fireEvent.pointerDown(page);
    expect(screen.getByRole("alertdialog")).toBeInTheDocument();

    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(trigger).toHaveFocus();
  });

  it("closes from either answer", async () => {
    await open();
    await userEvent.click(html(screen.getByRole("button", { name: "Delete" })));
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("dresses the trigger, the panel and both answers", async () => {
    await open();
    expect(screen.getByRole("alertdialog").getAttribute("class")).not.toBeNull();
    expect(screen.getByRole("button", { name: "Keep it" }).getAttribute("class")).not.toBeNull();
    expect(screen.getByRole("button", { name: "Delete" }).getAttribute("class")).not.toBeNull();
  });

  it("has no accessibility violations, closed or open", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
    await userEvent.click(html(screen.getByRole("button", { name: "Delete project" })));
    await expect(screen.getByRole("alertdialog")).toHaveNoAxeViolations();
  });
});
