// @flow
//
// The item `uf ui add item` writes, held to what its header promises: rows
// that are a list when they belong together, a title that is text rather than
// a heading, media nobody hears twice, actions after what they act on, and
// nothing that fails an accessibility audit.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen, within } from "@uniflowed/react-testing";

import * as Item from "./item.js";
import { Example } from "./item.example.js";

afterEach(() => {
  cleanup();
});

describe("Item", () => {
  it("makes a group of rows a named list with one listitem per row", () => {
    render(<Example />);
    const list = screen.getByRole("list", { name: "Team members" });
    expect(list.tagName).toBe("UL");
    expect(within(list).getAllByRole("listitem")).toHaveLength(2);
  });

  it("reads a title as text, not as a heading, and a linked one as its link", () => {
    render(<Example />);
    expect(screen.queryAllByRole("heading")).toHaveLength(0);
    expect(screen.getByText("Ada Lovelace").tagName).toBe("P");
    expect(screen.getByRole("link", { name: "Grace Hopper" })).toHaveAttribute(
      "href",
      "?member=grace",
    );
  });

  it("puts the actions after what they act on", () => {
    render(<Example />);
    const row = screen.getAllByRole("listitem")[0];
    const title = within(row).getByText("Ada Lovelace");
    const action = within(row).getByRole("button", { name: "Manage" });
    expect(title.compareDocumentPosition(action) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("is a div on its own, and passes every attribute and a class through", () => {
    render(
      <Item.Root className="mine" data-testid="row" id="plan" variant="plain">
        <Item.Content>
          <Item.Title>Team plan</Item.Title>
        </Item.Content>
      </Item.Root>,
    );
    const row = screen.getByTestId("row");
    expect(row.tagName).toBe("DIV");
    expect(row).toHaveAttribute("id", "plan");
    const classes = (row.getAttribute("class") ?? "").split(" ");
    expect(classes).toContain("mine");
    expect(classes.length).toBeGreaterThan(1);
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
  });
});
