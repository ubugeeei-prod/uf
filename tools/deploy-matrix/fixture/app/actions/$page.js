// @flow
//
// Server actions: the page is prerendered, and everything it does after that
// is a `POST` back to the target.

import * as React from "@uniflowed/react";

import Actions from "./_client/Actions.js";

export default component ActionsPage() {
  return (
    <article>
      <h1>matrix actions</h1>
      <Actions />
    </article>
  );
}
