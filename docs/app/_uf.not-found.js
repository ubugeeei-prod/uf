// @flow
//
// The page for a path that is not a page.
//
// It lists the manual rather than apologising: someone who arrives here from a
// stale link wants the table of contents, not a large "404".

import * as React from "@uniflowed/react";
import { Link } from "@uniflowed/router";

import { Eyebrow, Lede } from "./_design/parts.js";
import { pages } from "./_design/nav.js";

export const metadata: {| +title: string |} = { title: "Not found · uf" };

export default component NotFound() {
  return (
    <section className="not-found seam" id="content">
      <Eyebrow>Not found</Eyebrow>
      <h1>There is no page here.</h1>
      <Lede>
        The link may be from an older version of the site — uf is pre-release
        and pages still move. Everything that does exist is below.
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
