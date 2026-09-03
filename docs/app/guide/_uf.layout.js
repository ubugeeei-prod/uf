// @flow
//
// The manual.
//
// The sidebar, the seam, the prose column and the next-page link. `/guide` and
// `/reference` both use it; `/reference/_uf.layout.js` re-exports this one
// rather than copying it, so the two halves of the manual cannot drift apart.

import * as React from "@uniflowed/react";
import { useRoute } from "@uniflowed/router";

import { ManualNav, NextPage } from "../_design/parts.js";
import { nextAfter } from "../_design/nav.js";

export component Layout(children: React.Node) {
  const { pathname } = useRoute();
  const next = nextAfter(pathname);

  return (
    <div className="manual">
      <ManualNav />
      <main className="prose seam" id="content">
        {children}
        {next != null ? <NextPage href={next.href} title={next.title} /> : null}
      </main>
    </div>
  );
}
