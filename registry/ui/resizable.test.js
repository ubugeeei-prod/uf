// @flow
//
// The panels `uf ui add resizable` writes: a named handle that says where the
// boundary is and which panel it resizes, the keys that send it to either end,
// and the whole accessible.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { act, cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import { Example } from "./resizable.example.js";

afterEach(() => {
  cleanup();
});

/** Focus the handle the way a keyboard user reaches it. */
function focus(element: Element): void {
  if (!(element instanceof HTMLElement)) {
    throw new Error(`expected an HTML element, and found <${element.tagName.toLowerCase()}>`);
  }
  act(() => {
    element.focus();
  });
}

describe("Resizable", () => {
  it("names the handle and says where the boundary is, between its bounds", () => {
    render(<Example />);
    const handle = screen.getByRole("separator", { name: "Resize file list" });
    expect(handle).toHaveAttribute("aria-valuenow", "30");
    expect(handle).toHaveAttribute("aria-valuemin", "20");
    expect(handle).toHaveAttribute("aria-valuemax", "80");
    expect(handle).toHaveAttribute("aria-orientation", "vertical");
    expect(handle).toHaveAttribute("tabindex", "0");
  });

  it("controls the primary panel", () => {
    render(<Example />);
    const handle = screen.getByRole("separator", { name: "Resize file list" });
    const controlled = document.getElementById(handle.getAttribute("aria-controls") ?? "");
    expect(controlled).toHaveTextContent("Files");
  });

  it("sends the boundary to either bound with Home and End", async () => {
    render(<Example />);
    const handle = screen.getByRole("separator", { name: "Resize file list" });
    focus(handle);
    await userEvent.keyboard("{End}");
    expect(handle).toHaveAttribute("aria-valuenow", "80");
    await userEvent.keyboard("{Home}");
    expect(handle).toHaveAttribute("aria-valuenow", "20");
  });

  it("dresses the handle", () => {
    render(<Example />);
    expect(
      screen.getByRole("separator", { name: "Resize file list" }).getAttribute("class"),
    ).not.toBeNull();
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
  });
});
