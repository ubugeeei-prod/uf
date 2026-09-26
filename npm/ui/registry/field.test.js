// @flow
//
// The field `uf ui add field` writes: a control named by its label, described
// by its help, invalid and explained only while it is invalid, and every piece
// dressed and accessible.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen } from "@uniflowed/react-testing";

import { Example } from "./field.example.js";

afterEach(() => {
  cleanup();
});

/** The ids an element's `aria-describedby` names, as text. */
function described(element: Element): string {
  return (element.getAttribute("aria-describedby") ?? "")
    .split(" ")
    .map((id) => document.getElementById(id)?.textContent ?? "")
    .join(" | ");
}

describe("Field", () => {
  it("names each control by its label, and describes it by its help", () => {
    render(<Example />);
    const name = screen.getByRole("textbox", { name: "Name" });
    expect(name).toHaveAttribute("aria-required", "true");
    expect(name).not.toHaveAttribute("aria-invalid");
    expect(described(name)).toBe("As it should appear on receipts.");
  });

  it("says a field is invalid, and why, only while it is", () => {
    render(<Example />);
    const email = screen.getByRole("textbox", { name: "Email" });
    expect(email).toHaveAttribute("aria-invalid", "true");
    expect(described(email)).toContain("Enter an address with a domain");
  });

  it("wires a text area the same way", () => {
    render(<Example />);
    expect(screen.getByRole("textbox", { name: "Note" }).tagName).toBe("TEXTAREA");
  });

  it("dresses every control", () => {
    render(<Example />);
    for (const name of ["Name", "Email", "Note"]) {
      expect(screen.getByRole("textbox", { name }).getAttribute("class")).not.toBeNull();
    }
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
  });
});
