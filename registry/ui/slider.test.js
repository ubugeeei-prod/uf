// @flow
//
// The slider `uf ui add slider` writes: a named thumb for every value, the keys
// that move it, a range whose thumbs stop at each other, and the dressing
// accessible.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { act, cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import { Example } from "./slider.example.js";

afterEach(() => {
  cleanup();
});

/** Focus a thumb the way a keyboard user reaches it. */
function focus(element: Element): void {
  if (!(element instanceof HTMLElement)) {
    throw new Error(`expected an HTML element, and found <${element.tagName.toLowerCase()}>`);
  }
  act(() => {
    element.focus();
  });
}

describe("Slider", () => {
  it("gives every thumb a name, a value and the bounds its neighbour sets", () => {
    render(<Example />);
    expect(screen.getByRole("slider", { name: "Volume" })).toHaveAttribute("aria-valuenow", "50");
    const lower = screen.getByRole("slider", { name: "Minimum price" });
    expect(lower).toHaveAttribute("aria-valuenow", "20");
    expect(lower).toHaveAttribute("aria-valuemax", "80");
    expect(lower).toHaveAttribute("aria-valuetext", "$20");
  });

  it("moves a step on an arrow key, and to an end on End and Home", async () => {
    render(<Example />);
    const volume = screen.getByRole("slider", { name: "Volume" });
    focus(volume);
    await userEvent.keyboard("{ArrowRight}");
    expect(volume).toHaveAttribute("aria-valuenow", "51");
    await userEvent.keyboard("{End}");
    expect(volume).toHaveAttribute("aria-valuenow", "100");
    await userEvent.keyboard("{Home}");
    expect(volume).toHaveAttribute("aria-valuenow", "0");
  });

  it("stops a range's lower thumb at the upper one", async () => {
    render(<Example />);
    const lower = screen.getByRole("slider", { name: "Minimum price" });
    focus(lower);
    await userEvent.keyboard("{End}");
    expect(lower).toHaveAttribute("aria-valuenow", "80");
  });

  it("dresses every thumb", () => {
    render(<Example />);
    for (const thumb of screen.getAllByRole("slider")) {
      expect(thumb.getAttribute("class")).not.toBeNull();
    }
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
  });
});
