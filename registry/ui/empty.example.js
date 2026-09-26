// @flow
import * as React from "@uniflowed/react";

import { Button } from "./button.js";
import * as Empty from "./empty.js";

/** An invoice list with nothing in it yet, and the two ways to fill it. */
export component Example() {
  return (
    <Empty.Root>
      <Empty.Media>
        <svg
          fill="none"
          focusable="false"
          height="20"
          stroke="currentColor"
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth="2"
          viewBox="0 0 24 24"
          width="20"
        >
          <path d="M6 3h9l3 3v15H6z" />
          <path d="M9 12h6M9 16h4" />
        </svg>
      </Empty.Media>
      <Empty.Title>No invoices yet</Empty.Title>
      <Empty.Description>
        Invoices you create or import appear here, newest first.
      </Empty.Description>
      <Empty.Content>
        <Button tone="primary">Create an invoice</Button>
        <Button>Import from CSV</Button>
      </Empty.Content>
    </Empty.Root>
  );
}
