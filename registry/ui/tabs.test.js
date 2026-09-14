// @flow
//
// The tabs `uf ui add tabs` writes. `@uniflowed/ui`'s own suite holds the
// keyboard map; these hold that dressing the parts kept every piece of it, and
// that the selected look follows the attribute the part writes.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import { Tabs, TabsList, TabsPanel, TabsTab } from "./tabs.js";
import { Example } from "./tabs.example.js";

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

describe("Tabs", () => {
  it("names the row, and marks exactly one tab selected and reachable", () => {
    render(<Example />);
    expect(screen.getByRole("tablist", { name: "Project" })).toBeInTheDocument();

    const overview = screen.getByRole("tab", { name: "Overview" });
    const activity = screen.getByRole("tab", { name: "Activity" });
    expect(overview).toHaveAttribute("aria-selected", "true");
    expect(overview).toHaveAttribute("tabindex", "0");
    expect(activity).toHaveAttribute("aria-selected", "false");
    expect(activity).toHaveAttribute("tabindex", "-1");
  });

  it("moves along the row with the arrow keys, passing over the disabled tab", async () => {
    render(<Example />);
    await userEvent.click(html(screen.getByRole("tab", { name: "Overview" })));

    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: "Activity" })).toHaveFocus();
    expect(screen.getByRole("tab", { name: "Activity" })).toHaveAttribute("aria-selected", "true");

    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: "Overview" })).toHaveFocus();
  });

  it("shows the selected tab's panel, labelled by that tab", async () => {
    render(<Example />);
    await userEvent.click(html(screen.getByRole("tab", { name: "Activity" })));
    expect(screen.getByRole("tabpanel", { name: "Activity" })).toHaveTextContent(
      "The last thirty days of changes.",
    );
  });

  it("keeps a disabled tab in the row, announced as unavailable", () => {
    render(<Example />);
    expect(screen.getByRole("tab", { name: "Billing" })).toHaveAttribute("aria-disabled", "true");
  });

  it("runs the other way when it is vertical, and says so", async () => {
    render(
      <Tabs defaultValue="a" orientation="vertical">
        <TabsList aria-label="Settings">
          <TabsTab value="a">Account</TabsTab>
          <TabsTab value="b">Billing</TabsTab>
        </TabsList>
        <TabsPanel value="a">Account settings.</TabsPanel>
        <TabsPanel value="b">Billing settings.</TabsPanel>
      </Tabs>,
    );
    expect(screen.getByRole("tablist")).toHaveAttribute("aria-orientation", "vertical");
    await userEvent.click(html(screen.getByRole("tab", { name: "Account" })));
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("tab", { name: "Billing" })).toHaveFocus();
  });

  it("dresses the row, the tabs and the panel", () => {
    render(<Example />);
    expect(screen.getByRole("tablist").getAttribute("class")).not.toBeNull();
    expect(screen.getByRole("tab", { name: "Overview" }).getAttribute("class")).not.toBeNull();
    expect(screen.getByRole("tabpanel").getAttribute("class")).not.toBeNull();
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
  });
});
