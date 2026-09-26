// @flow
//
// The separator `uf ui add separator` writes: a boundary announced with its
// orientation, a decorative rule kept out of the accessibility tree, and both
// drawn.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen } from "@uniflowed/react-testing";

import { Example } from "./separator.example.js";

afterEach(() => {
  cleanup();
});

describe("Separator", () => {
  it("announces a boundary between groups, with its orientation", () => {
    render(<Example />);
    expect(screen.getByRole("separator")).toHaveAttribute("aria-orientation", "horizontal");
  });

  it("keeps a decorative rule out of the accessibility tree", () => {
    const { container } = render(<Example />);
    expect(container.querySelectorAll('[role="separator"]').length).toBe(1);
    const rules = container.querySelectorAll('[aria-hidden="true"]');
    expect(rules.length).toBe(1);
    expect(rules[0]?.getAttribute("class")).not.toBeNull();
  });

  it("draws the boundary", () => {
    render(<Example />);
    expect(screen.getByRole("separator").getAttribute("class")).not.toBeNull();
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
  });
});
