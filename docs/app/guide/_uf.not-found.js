// @flow
//
// The manual's own 404.
//
// A page under `/guide` that does not exist is still a question about the
// manual, so the answer belongs inside the manual: this renders in
// `app/guide/_uf.layout.js`, with the sidebar and the prose column, and the
// reader keeps the table of contents they were navigating. The site's root
// `_uf.not-found.js` answers everything else, and `/reference` has none of its
// own — so it falls back to the root one, which is the rule working rather
// than a gap.
//
// It lists the manual in reading order rather than apologising, for the same
// reason the root one lists the site: someone who followed a stale link wants
// the page that replaced it.

import * as React from "@uniflowed/react";
import { Link } from "@uniflowed/router";

import { Eyebrow, Lede } from "../_design/parts.js";
import { sections } from "../_design/nav.js";

export const metadata: {| readonly title: string |} = { title: "No such page · uf" };

export default component GuideNotFound() {
  return (
    <>
      <Eyebrow>Not found</Eyebrow>
      <h1>There is no such page in the manual.</h1>
      <Lede>
        uf is pre-release and pages still move, so a link from an older version of the site can land
        here. The manual, in reading order:
      </Lede>
      {sections.map((section) => (
        <section key={section.title}>
          <h2>{section.title}</h2>
          <ul>
            {section.pages.map((page) => (
              <li key={page.href}>
                <Link to={page.href}>{page.title}</Link> — {page.blurb}
              </li>
            ))}
          </ul>
        </section>
      ))}
    </>
  );
}
