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
      {/*
        The article comes first in the document and the sidebar second, with
        the grid putting the sidebar on the left at reading widths. On a phone
        there is one column, so a reader lands on the heading they followed a
        link to rather than scrolling past thirteen navigation links to reach
        it — and the contents are still there, below, where "what else is
        there" is the question being asked.
      */}
      <main className="prose seam" id="content">
        {children}
        {next != null ? <NextPage href={next.href} title={next.title} /> : null}
      </main>
      <ManualNav />
    </div>
  );
}
