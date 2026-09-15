// @flow
//
// The pagination `uf ui add pagination` writes: a named navigation landmark,
// one current page, previous and next named in words, a disabled end that is
// not a link, and the dressing accessible.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen } from "@uniflowed/react-testing";

import {
  Pagination,
  PaginationContent,
  PaginationItem,
  PaginationNext,
  PaginationPrevious,
} from "./pagination.js";
import { Example } from "./pagination.example.js";

afterEach(() => {
  cleanup();
});

/** The first of three pages, so there is no page before it. */
component FirstPage() {
  return (
    <Pagination page={1} pageCount={3}>
      <PaginationContent>
        <PaginationPrevious disabled />
        <PaginationItem current href="?page=1">
          1
        </PaginationItem>
        <PaginationItem href="?page=2">2</PaginationItem>
        <PaginationNext href="?page=2" />
      </PaginationContent>
    </Pagination>
  );
}

describe("Pagination", () => {
  it("is a named navigation landmark with one current page", () => {
    render(<Example />);
    expect(screen.getByRole("navigation", { name: "Pagination" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "2" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("link", { name: "1" })).not.toHaveAttribute("aria-current");
  });

  it("names previous and next in words", () => {
    render(<Example />);
    expect(screen.getByRole("link", { name: "Previous page" })).toHaveAttribute("href", "?page=1");
    expect(screen.getByRole("link", { name: "Next page" })).toHaveAttribute("href", "?page=3");
  });

  it("does not link past the first page", () => {
    const { container } = render(<FirstPage />);
    expect(screen.queryByRole("link", { name: "Previous page" })).toBeNull();
    expect(container.querySelector('[aria-disabled="true"]')).not.toBeNull();
  });

  it("dresses every link", () => {
    render(<Example />);
    for (const name of ["1", "2", "Previous page", "Next page"]) {
      expect(screen.getByRole("link", { name }).getAttribute("class")).not.toBeNull();
    }
  });

  it("has no accessibility violations", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();
  });

  it("has no other accessibility violations on its first page", async () => {
    // `aria-prohibited-attr` is off here alone: the part keeps `aria-label` on
    // the disabled end after taking its `href`, which leaves a named `generic`,
    // and that is `@uniflowed/ui`'s to fix, ubugeeei-prod/uf#1047. Every other
    // rule still runs.
    const { container } = render(<FirstPage />);
    await expect(container).toHaveNoAxeViolations({ disabledRules: ["aria-prohibited-attr"] });
  });
});
