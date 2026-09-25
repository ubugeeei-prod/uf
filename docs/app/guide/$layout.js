// @flow
//
// The manual.
//
// The sidebar, the seam, the prose column and the next-page link. `/guide` and
// `/reference` both use it; `/reference/$layout.js` re-exports this one
// rather than copying it, so the two halves of the manual cannot drift apart.

import * as React from "@uniflowed/react";
import { useRoute } from "@uniflowed/router";

import { CodeCopy } from "../_design/code-copy.js";
import { Outline } from "../_design/outline.js";
import { ManualNav, PageReadiness, PageSource, PageTurn } from "../_design/parts.js";
import { nextAfter, previousBefore, sourceFor } from "../_design/nav.js";

export component Layout(children: React.Node) {
  const { pathname } = useRoute();
  const next = nextAfter(pathname);
  const previous = previousBefore(pathname);
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
        {/*
          On a phone the sidebar is below the article, which is right for a
          reader who followed a link here and wrong for one who wants to go
          somewhere else. This is the way down to it, shown only there.
        */}
        <a className="to-contents" href="#manual-nav">
          All pages <span aria-hidden="true">↓</span>
        </a>
        <PageReadiness pathname={pathname} />
        {children}
        <PageTurn previous={previous} next={next} />
        {source != null ? <PageSource file={source} /> : null}
      </main>
      <ManualNav />
      {/*
        Third in the document, so a phone reaches the article and then the
        manual's contents before this page's; on a wide screen it is the right
        rail, and below that it is not shown at all.
      */}
      <Outline path={pathname} />
      <CodeCopy path={pathname} />
    </div>
  );
}
