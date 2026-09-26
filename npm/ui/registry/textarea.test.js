// @flow
//
// The text area `uf ui add textarea` writes, held to what its header promises:
// named by its label, every attribute and the ref on the element, and nothing
// that fails an accessibility audit.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import { Textarea } from "./textarea.js";
import { Example } from "./textarea.example.js";

afterEach(() => {
  cleanup();
});

describe("Textarea", () => {
  it("is a multi-line text box named by its label", () => {
    render(<Example />);
    const note = screen.getByRole("textbox", { name: "Release note" });
    expect(note.tagName).toBe("TEXTAREA");
    expect(note).toHaveAttribute("rows", "4");
  });

  it("puts every attribute it does not name on the element, and its ref", async () => {
    let element: HTMLTextAreaElement | null = null;
    render(
      <Textarea
        aria-describedby="hint"
        aria-label="Bio"
        maxLength={160}
        name="bio"
        ref={(node: HTMLTextAreaElement | null) => {
          element = node;
        }}
      />,
    );
    const bio = screen.getByRole("textbox", { name: "Bio" });
    expect(element).toBe(bio);
    expect(bio).toHaveAttribute("aria-describedby", "hint");
    expect(bio).toHaveAttribute("maxlength", "160");
    expect(bio).toHaveAttribute("name", "bio");
    await userEvent.type(bio, "Hello");
    expect(bio).toHaveValue("Hello");
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
  });
});
