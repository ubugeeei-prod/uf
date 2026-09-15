// @flow
//
// The scroll area `uf ui add scroll-area` writes: a named region a keyboard
// user can tab into, a drawn scrollbar kept from assistive technology, and the
// whole accessible.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen } from "@uniflowed/react-testing";

import { Example } from "./scroll-area.example.js";

afterEach(() => {
  cleanup();
});

describe("ScrollArea", () => {
  it("is a named region a keyboard user can tab into", () => {
    render(<Example />);
    expect(screen.getByRole("region", { name: "Releases" })).toHaveAttribute("tabindex", "0");
    expect(screen.getAllByRole("listitem")).toHaveLength(30);
  });

  it("draws one scrollbar down the box, kept from assistive technology", () => {
    const { container } = render(<Example />);
    const bars = Array.from(container.querySelectorAll("[data-orientation]"));
    expect(bars).toHaveLength(1);
    for (const bar of bars) {
      expect(bar).toHaveAttribute("data-orientation", "vertical");
      expect(bar).toHaveAttribute("aria-hidden", "true");
      expect(bar.getAttribute("class")).not.toBeNull();
    }
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
  });
});
