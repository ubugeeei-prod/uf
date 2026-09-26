// @flow
//
// The spinner `uf ui add spinner` writes, held to what its header promises: a
// named progress bar with no value, silent where the words beside it already
// say it, a mark that turns and a mark that only fades for reduced motion, and
// nothing that fails an accessibility audit.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen } from "@uniflowed/react-testing";

import { Spinner } from "./spinner.js";
import { Example } from "./spinner.example.js";

afterEach(() => {
  cleanup();
});

describe("Spinner", () => {
  it("is a progress bar named Loading, with no value to announce", () => {
    render(<Spinner />);
    const spinner = screen.getByRole("progressbar", { name: "Loading" });
    expect(spinner.tagName).toBe("SPAN");
    expect(spinner.hasAttribute("aria-valuenow")).toBe(false);
    expect(spinner.getAttribute("tabindex")).toBeNull();
  });

  it("takes the name of what is loading", () => {
    render(<Example />);
    expect(screen.getByRole("progressbar", { name: "Loading invoices" })).toBeInTheDocument();
  });

  it("is silent inside a button whose words already say it", () => {
    render(<Example />);
    const button = screen.getByRole("button", { name: "Saving…" });
    expect(button.querySelector("[role=progressbar]")).toBeNull();
    const drawing = button.querySelector("svg")?.parentElement;
    expect(drawing?.getAttribute("aria-hidden")).toBe("true");
    expect(screen.getAllByRole("progressbar")).toHaveLength(3);
  });

  it("puts every attribute it does not name on the element, and keeps its role", () => {
    render(<Spinner className="mine" data-state="busy" id="busy" role="img" />);
    const spinner = screen.getByRole("progressbar", { name: "Loading" });
    expect(spinner).toHaveAttribute("id", "busy");
    expect(spinner).toHaveAttribute("data-state", "busy");
    expect((spinner.getAttribute("class") ?? "").split(" ")).toContain("mine");
  });

  it("turns one mark and only fades the other, and shows them under different media", () => {
    const { container } = render(<Spinner />);
    const groups = Array.from(container.querySelectorAll("svg > g"));
    expect(groups).toHaveLength(2);
    const [turning, still] = groups;
    expect(turning.querySelector("animateTransform")?.getAttribute("type")).toBe("rotate");
    expect(turning.querySelector("animate")).toBeNull();
    expect(still.querySelector("animateTransform")).toBeNull();
    expect(still.querySelector("animate")?.getAttribute("attributeName")).toBe("opacity");
    expect(turning.getAttribute("class")).not.toBe(still.getAttribute("class"));
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
  });
});
