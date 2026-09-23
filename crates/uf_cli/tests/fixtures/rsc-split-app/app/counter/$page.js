// @flow
//
// The interactive route: a Server Component that renders a Client Component.
//
// The boundary is the import below. Everything on this side of it is still
// shipped to the browser — uf's client re-renders the matched tree from the
// same modules the server used, so the page above the boundary is a module
// React needs in order to reach the boundary at all. Dropping *that* needs a
// Flight-shaped payload; see ubugeeei-prod/uf#252.
//
// It also hands `submitNote` to `Stamp` as a prop, which is the one thing a
// Server Component can give a Client Component that is a function: a server
// reference (ubugeeei-prod/uf#1359).

import * as React from "@uniflowed/react";

import { submitNote } from "./_actions/tally.js";
import Counter from "./_components/Counter.js";
import Stamp from "./_components/Stamp.js";

export default component CounterPage() {
  return (
    <section>
      <h1>rsc-split-app counter</h1>
      <Counter />
      {/* A server function handed to a client component as a prop. */}
      <Stamp note={submitNote} />
    </section>
  );
}
