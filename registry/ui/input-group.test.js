// @flow
//
// The input group `uf ui add input-group` writes, held to what its header
// promises: an input named by its label rather than by its addons, addons that
// are heard or hidden as the caller says, every attribute and the ref on the
// input, and nothing that fails an accessibility audit.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import * as InputGroup from "./input-group.js";
import { Example } from "./input-group.example.js";

afterEach(() => {
  cleanup();
});

describe("InputGroup", () => {
  it("names each input by its label, and reads a prefix with it when asked", () => {
    render(<Example />);
    expect(screen.getByRole("searchbox", { name: "Search" })).toBeInTheDocument();
    const website = screen.getByRole("textbox", { name: "Website" });
    const described = website.getAttribute("aria-describedby") ?? "";
    expect(document.getElementById(described)?.textContent).toBe("https://");
    expect(screen.getByRole("textbox", { name: "Amount in USD" })).toHaveAttribute(
      "aria-invalid",
      "true",
    );
  });

  it("puts the input inside the frame, with its addons in reading order", () => {
    render(<Example />);
    const website = screen.getByRole("textbox", { name: "Website" });
    const frame = website.parentElement;
    expect(frame?.firstElementChild?.textContent).toBe("https://");
    expect(frame?.lastElementChild).toBe(website);
  });

  it("puts every attribute and the ref on the input, and takes typing", async () => {
    let element: HTMLInputElement | null = null;
    render(
      <InputGroup.Root>
        <InputGroup.Input
          aria-label="Handle"
          autoComplete="username"
          className="mine"
          name="handle"
          ref={(node: HTMLInputElement | null) => {
            element = node;
          }}
        />
        <InputGroup.Addon>.uf.dev</InputGroup.Addon>
      </InputGroup.Root>,
    );
    const handle = screen.getByRole("textbox", { name: "Handle" });
    expect(element).toBe(handle);
    expect(handle).toHaveAttribute("type", "text");
    expect(handle).toHaveAttribute("autocomplete", "username");
    expect((handle.getAttribute("class") ?? "").split(" ")).toContain("mine");
    await userEvent.type(handle, "ada");
    expect(handle).toHaveValue("ada");
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
  });
});
