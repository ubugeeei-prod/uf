// @flow
//
// The manual.
//
// The sidebar, the seam, the prose column and the next-page link. `/guide` and
// `/reference` both use it; `/reference/$layout.js` re-exports this one
// rather than copying it, so the two halves of the manual cannot drift apart.

import * as React from "@uniflowed/react";
import { useRoute } from "@uniflowed/router";

import { Outline } from "../_design/outline.js";
import { ManualNav, NextPage, PageSource } from "../_design/parts.js";
import { nextAfter, sourceFor } from "../_design/nav.js";

export component Layout(children: React.Node) {
  const { pathname } = useRoute();
  const next = nextAfter(pathname);
  const source = sourceFor(pathname);

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
        {source != null ? <PageSource file={source} /> : null}
      </main>
      <ManualNav />
      {/*
        Third in the document, so a phone reaches the article and then the
        manual's contents before this page's; on a wide screen it is the right
        rail, and below that it is not shown at all.
      */}
      <Outline path={pathname} />
    </div>
  );
}
