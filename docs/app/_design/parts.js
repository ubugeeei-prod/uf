// @flow
//
// The pieces the documentation is built from.
//
// Every one of them is a rule, a line of type, or a table. There are no cards
// and no shadows on this site: if a horizontal rule and a bordered box would
// both work, it is the rule. See `seam.css` for why.

import * as React from "@uniflowed/react";
import { Link, useRoute } from "@uniflowed/router";

import { isCurrent, sections } from "./nav.js";
import type { Section } from "./nav.js";

/**
 * The small mono label above a heading that says what kind of page follows —
 * the running head of a manual, not decoration.
 */
export component Eyebrow(children: React.Node) {
  return <p className="eyebrow">{children}</p>;
}

/** The one paragraph under a title that says what the page is for. */
export component Lede(children: React.Node) {
  return <p className="lede">{children}</p>;
}

/**
 * A command the reader is meant to run. The `$` is drawn by CSS rather than
 * written into the text, so selecting the line copies the command alone.
 */
export component Command(children: string) {
  return (
    <div className="command">
      <code>{children}</code>
    </div>
  );
}

/**
 * The manual's sidebar.
 *
 * A section's heading is the link to its landing page, and the list under it
 * is the section's pages in reading order. A section whose landing page is its
 * only page is a heading with nothing under it.
 *
 * `aria-current="page"` is what marks the open page — the border colour is a
 * consequence of it, not the other way round, so the state survives with CSS
 * off and is announced by a screen reader.
 */
export component ManualNav() {
  const { pathname } = useRoute();

  return (
    <nav className="manual-nav" aria-label="Documentation">
      <h2 className="manual-nav-title">All pages</h2>
      {sections.map((section) => (
        <React.Fragment key={section.title}>
          <h2>
            <Link
              to={section.landing.href}
              aria-current={isCurrent(pathname, section.landing.href) ? "page" : undefined}
            >
              {section.title}
            </Link>
          </h2>
          {section.pages.length > 0 ? (
            <ul>
              {section.pages.map((page) => (
                <li key={page.href}>
                  <Link
                    to={page.href}
                    aria-current={isCurrent(pathname, page.href) ? "page" : undefined}
                  >
                    {page.title}
                  </Link>
                </li>
              ))}
            </ul>
          ) : null}
        </React.Fragment>
      ))}
    </nav>
  );
}

/**
 * The home page's choice of path: one row per section, the question its
 * reader arrives with, and the landing page that answers it.
 *
 * A definition list, because that is what this is — a question and where it
 * is answered. It is generated from `nav.js`, so the home page cannot offer a
 * path the manual does not have or leave out one it does.
 */
export component ReaderPaths() {
  return (
    <dl className="paths">
      {sections.map((section) => (
        <div key={section.title}>
          <dt>{section.question}</dt>
          <dd>
            <Link to={section.landing.href}>{section.title}</Link>
            <span>{section.landing.blurb}</span>
          </dd>
        </div>
      ))}
    </dl>
  );
}

/**
 * A section's pages, in reading order, on that section's landing page.
 *
 * Rendered from `nav.js` rather than written out on each landing page, so a
 * page added to a section is on its landing page by construction, and the
 * order a landing page recommends is the order "next page" walks.
 *
 * `section` is the section's title. A title nothing matches fails the render —
 * and with it the build — rather than printing an empty list on a page whose
 * whole job is the list.
 */
export component SectionContents(section: string) {
  const found = sectionTitled(section);

  return (
    <ol className="section-contents">
      {found.pages.map((page) => (
        <li key={page.href}>
          <Link to={page.href}>{page.title}</Link>
          <span>{page.blurb}</span>
        </li>
      ))}
    </ol>
  );
}

function sectionTitled(title: string): Section {
  for (const section of sections) {
    if (section.title === title) {
      return section;
    }
  }
  throw new Error(`docs/app/_design/nav.js has no section titled ${JSON.stringify(title)}`);
}

/**
 * The link to the next page in reading order.
 *
 * Inside a section that is the page below this one; on a section's last page
 * it is the landing page of the section its reader goes to next. Rendering
 * nothing at the end of the manual is deliberate: there is no "next" to offer,
 * and a disabled control that says so would be noise.
 */
export component NextPage(href: string, title: string) {
  return (
    <p className="next-page">
      <Link to={href}>
        Next: {title} <span aria-hidden="true">→</span>
      </Link>
    </p>
  );
}

/** Where the repository is browsed, for a link to the file a page is written in. */
const SOURCE_BASE = "https://github.com/ubugeeei-prod/uf/blob/main/";

/**
 * The foot of a manual page: the file it is written in, as a link to edit it.
 *
 * A reader who found something wrong is one click from the file, and the path
 * is printed as well as linked — a contributor with the repository open wants
 * the path, not a web page.
 */
export component PageSource(file: string) {
  return (
    <p className="page-source">
      <a href={`${SOURCE_BASE}${file}`}>Edit this page</a>
      <code>{file}</code>
    </p>
  );
}
