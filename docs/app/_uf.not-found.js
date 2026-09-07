// @flow
//
// The page for a path that is not a page.
//
// It lists the manual rather than apologising: someone who arrives here from a
// stale link wants the table of contents, not a large "404".

import * as React from "@uniflowed/react";
import { Link } from "@uniflowed/router";
import type { Metadata } from "@uniflowed/router";

import { Eyebrow, Lede } from "./_design/parts.js";
import { pages } from "./_design/nav.js";

/**
 * `noindex` because this page is prerendered.
 *
 * `uf build` writes it to `dist/docs/404.html` so a static host has something
 * to serve, which makes it a document a crawler can be handed like any other —
 * and a "there is no page here" in a search result is a search result for a
 * page that does not exist. The sitemap already leaves it out; this is the
 * half that covers a crawler which found it another way.
 */
export const metadata: Metadata = {
  title: "Not found · uf",
  robots: { index: false, follow: false },
};

export default component NotFound() {
  return (
    <section className="not-found seam" id="content">
      <Eyebrow>Not found</Eyebrow>
      <h1>There is no page here.</h1>
      <Lede>
        The link may be from an older version of the site — uf is pre-release and pages still move.
        Everything that does exist is below.
      </Lede>
      <ul>
        {pages.map((page) => (
          <li key={page.href}>
            <Link to={page.href}>{page.title}</Link> — {page.blurb}
          </li>
        ))}
      </ul>
    </section>
  );
}
