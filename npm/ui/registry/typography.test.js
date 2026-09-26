// @flow
//
// The type scale `uf ui add typography` writes, held to what its header
// promises: every part the element its name says, headings at the level their
// name says, lists that stay lists, every attribute on the element, and
// nothing that fails an accessibility audit.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen } from "@uniflowed/react-testing";

import * as Typography from "./typography.js";
import { Example } from "./typography.example.js";

afterEach(() => {
  cleanup();
});

describe("Typography", () => {
  it("renders each heading at the level its name says", () => {
    render(<Example />);
    expect(screen.getByRole("heading", { level: 1, name: "Release notes" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 2, name: "Upgrading" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 3, name: "What moved" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 4, name: "Steps" })).toBeInTheDocument();
  });

  it("renders every other part as the element its name says", () => {
    const { container } = render(<Example />);
    expect(screen.getByText("uf ui update").tagName).toBe("CODE");
    expect(screen.getByText(/not a font size/).tagName).toBe("BLOCKQUOTE");
    expect(screen.getByText(/What changed/).tagName).toBe("P");
    expect(screen.getByText(/Published/).tagName).toBe("P");
    const lists = screen.getAllByRole("list");
    expect(lists.map((list) => list.tagName)).toEqual(["UL", "OL"]);
    expect(container.querySelectorAll("li")).toHaveLength(4);
  });

  it("draws each part differently", () => {
    render(<Example />);
    const classes = [
      screen.getByRole("heading", { level: 1 }),
      screen.getByRole("heading", { level: 2 }),
      screen.getByRole("heading", { level: 3 }),
      screen.getByRole("heading", { level: 4 }),
      screen.getByText(/What changed/),
      screen.getByText(/Published/),
      screen.getByText(/not a font size/),
      screen.getByText("uf ui update"),
    ].map((element) => element.getAttribute("class") ?? "");
    expect(new Set(classes).size).toBe(classes.length);
  });

  it("puts every attribute it does not name on the element, and a class beside its own", () => {
    render(
      <Typography.H2 className="mine" id="install">
        Install
      </Typography.H2>,
    );
    const heading = screen.getByRole("heading", { level: 2, name: "Install" });
    expect(heading).toHaveAttribute("id", "install");
    const classes = (heading.getAttribute("class") ?? "").split(" ");
    expect(classes).toContain("mine");
    expect(classes.length).toBeGreaterThan(1);
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
  });
});
