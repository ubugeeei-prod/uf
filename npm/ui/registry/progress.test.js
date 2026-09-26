// @flow
//
// The progress bar `uf ui add progress` writes: a named bar whose fill is drawn
// from the value it announces, and an unknown amount announced as unknown.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen } from "@uniflowed/react-testing";

import { Progress } from "./progress.js";
import { Example } from "./progress.example.js";

afterEach(() => {
  cleanup();
});

/** The element a query found, as an element with a `style`. See #1017. */
function html(element: Element): HTMLElement {
  if (element instanceof HTMLElement) {
    return element;
  }
  throw new Error(`expected an HTML element, and found <${element.tagName.toLowerCase()}>`);
}

describe("Progress", () => {
  it("announces how far it has got, in numbers and in words", () => {
    render(<Example />);
    const upload = screen.getByRole("progressbar", { name: "Uploading photos" });
    expect(upload).toHaveAttribute("aria-valuenow", "3");
    expect(upload).toHaveAttribute("aria-valuemax", "10");
    expect(upload).toHaveAttribute("aria-valuetext", "3 of 10 photos");
  });

  it("draws the fill from the value it announces", () => {
    render(<Example />);
    const upload = html(screen.getByRole("progressbar", { name: "Uploading photos" }));
    expect(upload.style.getPropertyValue("--uf-progress")).toBe("0.3");
  });

  it("says nothing about an amount it does not know, and draws no fill for it", () => {
    render(<Example />);
    const pending = html(screen.getByRole("progressbar", { name: "Preparing the export" }));
    expect(pending).not.toHaveAttribute("aria-valuenow");
    expect(pending.style.getPropertyValue("--uf-progress")).toBe("");
  });

  it("keeps a value inside the range it draws", () => {
    render(<Progress aria-label="Over" max={4} value={9} />);
    const over = html(screen.getByRole("progressbar", { name: "Over" }));
    expect(over.style.getPropertyValue("--uf-progress")).toBe("1");
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
  });
});
