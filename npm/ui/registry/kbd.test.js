// @flow
//
// The key `uf ui add kbd` writes, held to what its header promises: a `<kbd>`
// with no role and nothing to focus, a combination written as the HTML
// specification writes it, every attribute on the element, and nothing that
// fails an accessibility audit.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen } from "@uniflowed/react-testing";

import { Kbd } from "./kbd.js";
import { Example } from "./kbd.example.js";

afterEach(() => {
  cleanup();
});

describe("Kbd", () => {
  it("is a kbd read as its words, with no role and nothing to focus", () => {
    render(<Kbd>Esc</Kbd>);
    const key = screen.getByText("Esc");
    expect(key.tagName).toBe("KBD");
    expect(key.getAttribute("role")).toBeNull();
    expect(key.getAttribute("tabindex")).toBeNull();
  });

  it("writes a combination as one kbd holding a kbd for each key", () => {
    const { container } = render(<Kbd data-testid="combo" keys={["Ctrl", "Shift", "K"]} />);
    const outer = screen.getByTestId("combo");
    expect(outer.tagName).toBe("KBD");
    const inner = Array.from(outer.children);
    expect(inner.map((element) => element.tagName)).toEqual(["KBD", "KBD", "KBD"]);
    expect(inner.map((element) => element.textContent)).toEqual(["Ctrl", "Shift", "K"]);
    expect(outer.textContent).toBe("Ctrl+Shift+K");
    expect(container.querySelectorAll("kbd kbd kbd")).toHaveLength(0);
  });

  it("puts every attribute it does not name on the element, and a class beside its own", () => {
    render(
      <Kbd className="mine" id="close-key" title="Escape">
        Esc
      </Kbd>,
    );
    const key = screen.getByText("Esc");
    expect(key).toHaveAttribute("id", "close-key");
    expect(key).toHaveAttribute("title", "Escape");
    const classes = (key.getAttribute("class") ?? "").split(" ");
    expect(classes).toContain("mine");
    expect(classes.length).toBeGreaterThan(1);
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
  });
});
