// @flow
//
// The label `uf ui add label` writes, held to what its header promises: a real
// `<label>` that names its control, with nothing that fails an accessibility
// audit. What a click on a label does is the browser's, and is not retested
// here.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen } from "@uniflowed/react-testing";

import { Label } from "./label.js";
import { Example } from "./label.example.js";

afterEach(() => {
  cleanup();
});

describe("Label", () => {
  it("names the control its htmlFor points at", () => {
    render(<Example />);
    const input = screen.getByLabelText("Email (required)");
    expect(input).toHaveAttribute("id", "label-example-email");
    expect(screen.getByText("Email (required)").tagName).toBe("LABEL");
  });

  it("puts every attribute it does not name on the element", () => {
    render(
      <Label data-part="caption" htmlFor="x" id="caption">
        Caption
      </Label>,
    );
    const label = screen.getByText("Caption");
    expect(label).toHaveAttribute("for", "x");
    expect(label).toHaveAttribute("id", "caption");
    expect(label).toHaveAttribute("data-part", "caption");
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
  });
});
