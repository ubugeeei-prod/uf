// @flow
//
// The empty state `uf ui add empty` writes, held to what its header promises:
// a title that is a heading at the level asked for, a picture nobody hears, a
// next step a keyboard reaches, no role of its own, and nothing that fails an
// accessibility audit.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen } from "@uniflowed/react-testing";

import * as Empty from "./empty.js";
import { Example } from "./empty.example.js";

afterEach(() => {
  cleanup();
});

describe("Empty", () => {
  it("names what is empty with a heading, and offers the next step", () => {
    render(<Example />);
    expect(screen.getByRole("heading", { level: 2, name: "No invoices yet" })).toBeInTheDocument();
    expect(screen.getByText(/appear here/).tagName).toBe("P");
    expect(screen.getByRole("button", { name: "Create an invoice" })).toBeInTheDocument();
  });

  it("hides its picture and takes no role of its own", () => {
    const { container } = render(<Example />);
    const root = container.firstElementChild;
    expect(root?.getAttribute("role")).toBeNull();
    const media = container.querySelector("svg")?.parentElement;
    expect(media?.getAttribute("aria-hidden")).toBe("true");
  });

  it("puts its title at the level the page needs", () => {
    render(
      <Empty.Root>
        <Empty.Title level={3}>No results</Empty.Title>
      </Empty.Root>,
    );
    expect(screen.getByRole("heading", { level: 3, name: "No results" })).toBeInTheDocument();
  });

  it("puts every attribute it does not name on the element, and a class beside its own", () => {
    render(
      <Empty.Root className="mine" data-testid="empty" id="no-invoices">
        <Empty.Title>Nothing</Empty.Title>
      </Empty.Root>,
    );
    const root = screen.getByTestId("empty");
    expect(root).toHaveAttribute("id", "no-invoices");
    const classes = (root.getAttribute("class") ?? "").split(" ");
    expect(classes).toContain("mine");
    expect(classes.length).toBeGreaterThan(1);
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
  });
});
