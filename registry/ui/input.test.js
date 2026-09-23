// @flow
//
// The input `uf ui add input` writes, held to what its header promises: named
// by its label, every attribute and the ref on the element, text unless it
// says otherwise, and nothing that fails an accessibility audit.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import { Input } from "./input.js";
import { Example } from "./input.example.js";

afterEach(() => {
  cleanup();
});

describe("Input", () => {
  it("is named by its label and is text unless it says otherwise", () => {
    render(<Example />);
    const search = screen.getByRole("searchbox", { name: "Search projects" });
    expect(search).toHaveAttribute("type", "search");
    expect(screen.getByRole("textbox", { name: "Slug" })).toHaveAttribute("type", "text");
  });

  it("puts every attribute it does not name on the element", () => {
    render(
      <Input
        aria-label="Email"
        autoComplete="email"
        inputMode="email"
        name="email"
        required
        type="email"
      />,
    );
    const input = screen.getByRole("textbox", { name: "Email" });
    expect(input).toHaveAttribute("autocomplete", "email");
    expect(input).toHaveAttribute("inputmode", "email");
    expect(input).toHaveAttribute("name", "email");
    expect(input).toHaveAttribute("type", "email");
    expect(input).toBeRequired();
  });

  it("hands its ref to the element and takes typing", async () => {
    let element: HTMLInputElement | null = null;
    render(
      <Input
        aria-label="Name"
        ref={(node: HTMLInputElement | null) => {
          element = node;
        }}
      />,
    );
    const input = screen.getByRole("textbox", { name: "Name" });
    expect(element).toBe(input);
    await userEvent.type(input, "Ada");
    expect(input).toHaveValue("Ada");
  });

  it("says it is invalid through the attribute its look follows", () => {
    render(<Example />);
    expect(screen.getByRole("textbox", { name: "Slug" })).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByRole("textbox", { name: "Project id" })).toBeDisabled();
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
  });
});
