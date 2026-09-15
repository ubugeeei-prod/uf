// @flow
//
// The breadcrumb `uf ui add breadcrumb` writes: a named navigation landmark,
// links up the trail, the current page marked and not linked, and separators
// nobody hears.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen } from "@uniflowed/react-testing";

import { Example } from "./breadcrumb.example.js";

afterEach(() => {
  cleanup();
});

describe("Breadcrumb", () => {
  it("is a named navigation landmark with links up the trail", () => {
    render(<Example />);
    expect(screen.getByRole("navigation", { name: "Breadcrumb" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Atlas" })).toHaveAttribute("href", "/projects/atlas");
  });

  it("marks the current page, and does not link to it", () => {
    render(<Example />);
    const page = screen.getByText("Settings");
    expect(page).toHaveAttribute("aria-current", "page");
    expect(screen.queryByRole("link", { name: "Settings" })).toBeNull();
  });

  it("keeps the separators out of the accessibility tree", () => {
    const { container } = render(<Example />);
    const separators = container.querySelectorAll('li[aria-hidden="true"]');
    expect(separators.length).toBe(2);
  });

  it("dresses the links and the page", () => {
    render(<Example />);
    expect(screen.getByRole("link", { name: "Home" }).getAttribute("class")).not.toBeNull();
    expect(screen.getByText("Settings").getAttribute("class")).not.toBeNull();
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
  });
});
