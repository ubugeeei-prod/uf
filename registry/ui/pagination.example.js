// @flow
import * as React from "@uniflowed/react";

import * as Pagination from "./pagination.js";

/** The second of five pages, with a page before it and a page after it. */
export component Example() {
  return (
    <Pagination.Root page={2} pageCount={5}>
      <Pagination.Content>
        <Pagination.Previous href="?page=1" />
        <Pagination.Item href="?page=1">1</Pagination.Item>
        <Pagination.Item current href="?page=2">
          2
        </Pagination.Item>
        <Pagination.Item href="?page=3">3</Pagination.Item>
        <Pagination.Item href="?page=5">5</Pagination.Item>
        <Pagination.Next href="?page=3" />
      </Pagination.Content>
    </Pagination.Root>
  );
}
