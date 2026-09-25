// @flow
//
// A Server Component handing a Client Component a function that is not a
// server action. Only a `"use server"` export is registered as a server
// reference, so Flight refuses this one while it renders, and the route
// answers with its error boundary (ubugeeei-prod/uf#1359).
import * as React from "@uniflowed/react";

import { DeleteNote } from "../../notes/_components/DeleteNote.js";

// Rendered per request, so `uf build` does not try to prerender a page that
// is meant to fail.
export const dynamic = "force-dynamic";

async function notAnAction(): Promise<void> {}

export component Page() {
  return <DeleteNote remove={notAnAction} />;
}
