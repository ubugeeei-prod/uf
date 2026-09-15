// @flow
//
// The navigation `uf ui add navigation-menu` writes: a named `<nav>` whose
// triggers disclose one panel at a time, a panel named by its trigger and gone
// while closed, a followed link closing its panel, and the whole accessible.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import { Example } from "./navigation-menu.example.js";

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

describe("NavigationMenu", () => {
  it("is a named navigation whose panels start closed", () => {
    render(<Example />);
    expect(screen.getByRole("navigation", { name: "Main" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Guides" })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
    expect(screen.queryByRole("link", { name: "Installation" })).toBeNull();
    expect(screen.getByRole("link", { name: "Blog" })).toHaveAttribute("href", "#blog");
  });

  it("opens a panel named by its trigger, one at a time", async () => {
    render(<Example />);
    const guides = html(screen.getByRole("button", { name: "Guides" }));
    await userEvent.click(guides);
    expect(guides).toHaveAttribute("aria-expanded", "true");
    expect(guides).toHaveAttribute(
      "aria-controls",
      screen.getByRole("list", { name: "Guides" }).id,
    );

    await userEvent.click(html(screen.getByRole("button", { name: "Reference" })));
    expect(guides).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByRole("link", { name: "CLI" })).toBeInTheDocument();
  });

  it("closes a panel when a link in it is followed", async () => {
    render(<Example />);
    await userEvent.click(html(screen.getByRole("button", { name: "Guides" })));
    await userEvent.click(html(screen.getByRole("link", { name: "Installation" })));
    expect(screen.queryByRole("link", { name: "Installation" })).toBeNull();
  });

  it("dresses the triggers, the panel and its links", async () => {
    render(<Example />);
    const guides = html(screen.getByRole("button", { name: "Guides" }));
    expect(guides.getAttribute("class")).not.toBeNull();
    await userEvent.click(guides);
    expect(screen.getByRole("list", { name: "Guides" }).getAttribute("class")).not.toBeNull();
    expect(screen.getByRole("link", { name: "Routing" }).getAttribute("class")).not.toBeNull();
  });

  it("has no accessibility violations, closed or open", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
    await userEvent.click(html(screen.getByRole("button", { name: "Reference" })));
    await expect(container).toHaveNoAxeViolations();
  });
});
