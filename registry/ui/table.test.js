// @flow
//
// The table `uf ui add table` writes: a table named by its caption, headings
// that sort and say which way, rows chosen one at a time or all at once, sort
// buttons dressed through the part, and the whole accessible.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import { Example } from "./table.example.js";

afterEach(() => {
  cleanup();
});

/** The element a query found, as the element `userEvent` clicks. See #1017. */
function html(element: Element): HTMLElement {
  if (element instanceof HTMLElement) {
    return element;
  }
  throw new Error(`expected an HTML element, and found <${element.tagName.toLowerCase()}>`);
}

/** The names in the rows, in the order the table shows them. */
function customers(): $ReadOnlyArray<string> {
  return screen.getAllByRole("rowheader").map((cell) => cell.textContent ?? "");
}

describe("Table", () => {
  it("is a table named by its caption", () => {
    render(<Example />);
    expect(screen.getByRole("table", { name: "Invoices this month" })).toBeInTheDocument();
  });

  it("sorts by a heading, and says which way", async () => {
    render(<Example />);
    const button = html(screen.getByRole("button", { name: "Customer" }));
    await userEvent.click(button);
    const heading = screen.getByRole("columnheader", { name: "Customer" });
    expect(heading).toHaveAttribute("aria-sort", "ascending");
    expect(customers()).toEqual(["Ada Lovelace", "Alan Turing", "Grace Hopper"]);
    await userEvent.click(button);
    expect(heading).toHaveAttribute("aria-sort", "descending");
    expect(customers()).toEqual(["Grace Hopper", "Alan Turing", "Ada Lovelace"]);
  });

  it("chooses rows, and marks select-all mixed while only some are chosen", async () => {
    render(<Example />);
    await userEvent.click(html(screen.getByRole("checkbox", { name: "Select Ada Lovelace" })));
    const all = html(screen.getByRole("checkbox", { name: "Select all rows" }));
    expect(all).toHaveAttribute("aria-checked", "mixed");
    await userEvent.click(all);
    expect(screen.getByRole("checkbox", { name: "Select Grace Hopper" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
  });

  it("dresses the sort buttons the part builds", () => {
    render(<Example />);
    expect(screen.getByRole("button", { name: "Amount" }).getAttribute("class")).not.toBeNull();
  });

  it("has no accessibility violations, unsorted or sorted", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
    await userEvent.click(html(screen.getByRole("button", { name: "Amount" })));
    await expect(container).toHaveNoAxeViolations();
  });
});
