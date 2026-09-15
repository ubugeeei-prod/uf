// @flow
//
// The switch `uf ui add switch` writes: named by its words, flipped by a press
// and by `Enter`, and still a switch while it is disabled.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import { Example } from "./switch.example.js";

afterEach(() => {
  cleanup();
});

/** The element a query found, as the element `userEvent` presses. See #1017. */
function html(element: Element): HTMLElement {
  if (element instanceof HTMLElement) {
    return element;
  }
  throw new Error(`expected an HTML element, and found <${element.tagName.toLowerCase()}>`);
}

describe("Switch", () => {
  it("is a switch named by its words, in the state it was given", () => {
    render(<Example />);
    expect(screen.getByRole("switch", { name: "Email me when someone replies" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    expect(screen.getByRole("switch", { name: "Show a preview of each message" })).toHaveAttribute(
      "aria-checked",
      "false",
    );
  });

  it("flips on a press and on Enter", async () => {
    render(<Example />);
    const preview = html(screen.getByRole("switch", { name: "Show a preview of each message" }));
    await userEvent.click(preview);
    expect(preview).toHaveAttribute("aria-checked", "true");

    await userEvent.keyboard("{Enter}");
    expect(preview).toHaveAttribute("aria-checked", "false");
  });

  it("does not flip while it is disabled", async () => {
    render(<Example />);
    const sync = html(screen.getByRole("switch", { name: "Sync with my calendar" }));
    await userEvent.click(sync);
    expect(sync).toHaveAttribute("aria-checked", "false");
  });

  it("draws the switch", () => {
    render(<Example />);
    const reply = screen.getByRole("switch", { name: "Email me when someone replies" });
    expect(reply.getAttribute("class")).not.toBeNull();
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
  });
});
