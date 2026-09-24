// @flow
import * as React from "@uniflowed/react";

import * as AlertDialog from "./alert-dialog.js";

/** A deletion, asked about before it happens. */
export component Example() {
  return (
    <AlertDialog.Root>
      <AlertDialog.Trigger tone="danger">Delete project</AlertDialog.Trigger>
      <AlertDialog.Content>
        <AlertDialog.Header>
          <AlertDialog.Title>Delete this project?</AlertDialog.Title>
          <AlertDialog.Description>
            Its pages, its history and its settings go with it, and this cannot be undone.
          </AlertDialog.Description>
        </AlertDialog.Header>
        <AlertDialog.Footer>
          <AlertDialog.Cancel>Keep it</AlertDialog.Cancel>
          <AlertDialog.Action tone="danger">Delete</AlertDialog.Action>
        </AlertDialog.Footer>
      </AlertDialog.Content>
    </AlertDialog.Root>
  );
}
