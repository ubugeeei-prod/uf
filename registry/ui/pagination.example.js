// @flow
import * as React from "@uniflowed/react";

import {
  Pagination,
  PaginationContent,
  PaginationItem,
  PaginationNext,
  PaginationPrevious,
} from "./pagination.js";

/** The second of five pages, with a page before it and a page after it. */
export component Example() {
  return (
    <Pagination page={2} pageCount={5}>
      <PaginationContent>
        <PaginationPrevious href="?page=1" />
        <PaginationItem href="?page=1">1</PaginationItem>
        <PaginationItem current href="?page=2">
          2
        </PaginationItem>
        <PaginationItem href="?page=3">3</PaginationItem>
        <PaginationItem href="?page=5">5</PaginationItem>
        <PaginationNext href="?page=3" />
      </PaginationContent>
    </Pagination>
  );
}
