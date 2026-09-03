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

/** A line of terminal output, and how it should read. */
export type OutputLine = {|
  +text: string,
  +tone?: "plain" | "ok" | "bad" | "muted",
|};

/**
 * Terminal output, reproduced.
 *
 * What appears here is what the command actually printed — these blocks are
 * pasted from a real run, never composed to look good. A page that shows
 * invented output is a page that will be wrong the first time someone checks.
 */
export component Terminal(lines: $ReadOnlyArray<OutputLine>, label?: string) {
  return (
    <figure className="terminal" role="group" aria-label={label ?? "Terminal output"}>
      <pre>
        <code>
          {lines.map((line, index) => (
            <span className={toneClass(line.tone)} key={index}>
              {line.text}
              {"\n"}
            </span>
          ))}
        </code>
      </pre>
    </figure>
  );
}

function toneClass(tone: ?("plain" | "ok" | "bad" | "muted")): string {
  return match (tone) {
    "ok" => "ok",
    "bad" => "bad",
    "muted" => "muted",
    _ => "",
  };
}

/**
 * The manual's sidebar.
 *
 * `aria-current="page"` is what marks the open page — the border colour is a
 * consequence of it, not the other way round, so the state survives with CSS
 * off and is announced by a screen reader.
 */
export component ManualNav() {
  const { pathname } = useRoute();

  return (
    <nav className="manual-nav" aria-label="Documentation">
      {sections.map((section) => (
        <React.Fragment key={section.title}>
          <h2>{section.title}</h2>
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
        </React.Fragment>
      ))}
    </nav>
  );
}

/** A claim the project makes, and the page that backs it up. */
export type Claim = {|
  +title: string,
  +body: React.Node,
|};

/**
 * The home page's claims.
 *
 * A definition list, because that is what this is: a term and its
 * elaboration. Not a grid of icons — every row here has to earn its space by
 * linking to the page that proves it.
 */
export component Claims(items: $ReadOnlyArray<Claim>) {
  return (
    <dl className="claims">
      {items.map((item) => (
        <div key={item.title}>
          <dt>{item.title}</dt>
          <dd>{item.body}</dd>
        </div>
      ))}
    </dl>
  );
}

/**
 * The link to the next page in reading order.
 *
 * A manual is read front to back at least once. Rendering nothing at the end
 * is deliberate: there is no "next" to offer, and a disabled control that
 * says so would be noise.
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
