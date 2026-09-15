// @flow
//
// The skeleton `uf ui add skeleton` writes: a busy region, shapes nobody hears,
// a status that says the page is loading, and every shape drawn.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen } from "@uniflowed/react-testing";

import { Example } from "./skeleton.example.js";

afterEach(() => {
  cleanup();
});

describe("Skeleton", () => {
  it("marks the region busy and hides every shape", () => {
    const { container } = render(<Example />);
    expect(container.querySelector('[aria-busy="true"]')).not.toBeNull();
    const shapes = container.querySelectorAll('[aria-hidden="true"]');
    expect(shapes.length).toBe(4);
    for (const shape of shapes) {
      expect(shape.getAttribute("class")).not.toBeNull();
    }
  });

  it("says out loud that the page is loading", () => {
    render(<Example />);
    expect(screen.getByRole("status")).toHaveTextContent("Loading…");
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
  });
});
