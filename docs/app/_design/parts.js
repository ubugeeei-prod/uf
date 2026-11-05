// @flow
//
// The pieces the documentation is built from.
//
// Every one of them is a rule, a line of type, or a table. There are no cards
// and no shadows on this site: if a horizontal rule and a bordered box would
// both work, it is the rule. See `seam.css` for why.

import * as React from "@uniflowed/react";
import { useSyncExternalStore } from "@uniflowed/react";
import { Link, useRoute } from "@uniflowed/router";

import { goals } from "./goals.js";
import type { Goal } from "./goals.js";
import { NavOverflow } from "./nav-overflow.js";
import {
  currentHref,
  featuredIn,
  openSectionFor,
  readinessFor,
  sections,
  sectionsAfterToggle,
  sectionsKeptOpen,
} from "./nav.js";
import type { Entry, Section } from "./nav.js";

const NAV_OPEN = "uf-docs-nav-open";
const NAV_EMPTY = "[]";

/**
 * Sections this tab left open.
 *
 * The sidebar remounts when a reader crosses from the guide to the reference,
 * so the open sections cannot live in component state: that state is what a
 * navigation threw away. `sessionStorage` outlives the remount and does not
 * outlive the tab. The snapshot is a string because `useSyncExternalStore`
 * compares with `Object.is`, and a freshly parsed array never compares equal
 * to the previous one.
 *
 * The server, and the first client render, answer `[]`. Storage is read on
 * the client after hydration, which is how the markup the server sent and the
 * markup hydration sees stay the same: only the section of the page is open
 * until then.
 */
const navListeners = new Set<() => void>();
let navMemory: string | null = null;

function subscribeNav(notify: () => void): () => void {
  navListeners.add(notify);
  return () => {
    navListeners.delete(notify);
  };
}

function notifyNav(): void {
  for (const notify of navListeners) {
    notify();
  }
}

function readStored(): string {
  if (typeof sessionStorage === "undefined") {
    return NAV_EMPTY;
  }
  try {
    return sessionStorage.getItem(NAV_OPEN) ?? NAV_EMPTY;
  } catch {
    return NAV_EMPTY;
  }
}

function readSnapshot(): string {
  if (navMemory == null) {
    navMemory = readStored();
  }
  return navMemory;
}

function serverSnapshot(): string {
  return NAV_EMPTY;
}

function parseRemembered(raw: string): $ReadOnlyArray<string> {
  try {
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.filter((item) => typeof item === "string") : [];
  } catch {
    return [];
  }
}

function writeRemembered(titles: $ReadOnlyArray<string>): void {
  const raw = JSON.stringify(titles);
  if (raw === readSnapshot()) {
    return;
  }
  navMemory = raw;
  if (typeof sessionStorage !== "undefined") {
    try {
      sessionStorage.setItem(NAV_OPEN, raw);
    } catch {
      // A private window that refuses storage still toggles for this page.
    }
  }
  notifyNav();
}

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

/** The page's declared capability status, in words and linked to its meaning. */
export component PageReadiness(pathname: string) {
  const readiness = readinessFor(pathname);
  if (readiness == null) return null;
  return (
    <p className="page-readiness">
      <Link to="/guide/goals">Readiness</Link>: <strong>{readiness}</strong>
    </p>
  );
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
 * Every section is a disclosure. The one the open page is in starts open, and
 * a section a reader opens stays open when they follow a link: the next page
 * adds its own section rather than closing the ones already open. That set is
 * kept for the tab, so crossing from the guide into the reference does not
 * fold it back up. Without JavaScript only the current section is open, which
 * is also what the first paint is, so the marked link is visible before
 * hydration. `<details>` rather than a script for the disclosure itself: it
 * opens and closes without JavaScript, and a screen reader already knows what
 * it is. Listing every section's pages at once made a sidebar several screens
 * tall whose bottom edge looked like its end; a closed section is a heading
 * and a page count.
 *
 * The section's landing page is the first link inside it, so opening a section
 * and following a link are two different gestures rather than one heading
 * that does either depending on where it is clicked.
 *
 * `aria-current="page"` is what marks the open page — the border colour is a
 * consequence of it, not the other way round, so the state survives with CSS
 * off and is announced by a screen reader. `NavOverflow` is the part that needs
 * a script: it says how much of the list is below the fold, and scrolls the
 * open page into view.
 */
export component ManualNav() {
  const { pathname } = useRoute();
  const marked = currentHref(pathname);
  const current = openSectionFor(pathname);
  const currentTitle = current == null ? null : current.title;
  const remembered = parseRemembered(
    useSyncExternalStore(subscribeNav, readSnapshot, serverSnapshot),
  );
  // The page's own section is open even when storage has not caught up, and
  // even when a reader closed it on the previous page: the marked link has to
  // be visible. A section they opened besides that one stays open.
  const openTitles = sectionsKeptOpen(remembered, currentTitle);

  const isOpen = (title: string): boolean => openTitles.includes(title);

  return (
    <nav className="manual-nav" id="manual-nav" aria-label="Documentation">
      <h2 className="manual-nav-title">All pages</h2>
      <div className="manual-nav-list" id="manual-nav-list">
        {sections.map((section) => {
          const links = [section.landing, ...section.pages];
          return (
            <details
              className="manual-nav-section"
              key={section.title}
              open={isOpen(section.title)}
              onToggle={(event) => {
                const element = event.currentTarget;
                if (!(element instanceof HTMLDetailsElement)) {
                  return;
                }
                // The section the page is in stays open. The browser has
                // already closed it; putting it back agrees with the `open`
                // the next render sets, and does not wait on that render.
                if (!element.open && section.title === currentTitle) {
                  element.open = true;
                  return;
                }
                const base = sectionsKeptOpen(parseRemembered(readSnapshot()), currentTitle);
                writeRemembered(sectionsAfterToggle(base, section.title, element.open));
              }}
            >
              <summary>
                <h2>{section.title}</h2>
                <span className="manual-nav-count">
                  {links.length}
                  <span className="visually-hidden">{links.length === 1 ? " page" : " pages"}</span>
                </span>
              </summary>
              <ul>
                {links.map((page) => (
                  <li key={page.href}>
                    <Link to={page.href} aria-current={page.href === marked ? "page" : undefined}>
                      {page === section.landing && page.title === section.title
                        ? "Overview"
                        : page.title}
                    </Link>
                  </li>
                ))}
              </ul>
            </details>
          );
        })}
      </div>
      <NavOverflow path={pathname} />
    </nav>
  );
}

/**
 * The home page's choice of path: one row per section, the question its
 * reader arrives with, the landing page that answers it, and the pages most
 * of its readers open first.
 *
 * A definition list, because that is what this is — a question and where it
 * is answered. It is generated from `nav.js`, so the home page cannot offer a
 * path the manual does not have or leave out one it does, and a featured page
 * is always one its section lists.
 */
export component ReaderPaths() {
  return (
    <dl className="paths">
      {sections.map((section) => {
        const featured = featuredIn(section);
        return (
          <div key={section.title}>
            <dt>{section.question}</dt>
            <dd>
              <Link to={section.landing.href}>{section.title}</Link>
              <span>{section.landing.blurb}</span>
              {featured.length > 0 ? (
                <ul className="paths-featured" aria-label={`${section.title}: most read`}>
                  {featured.map((page) => (
                    <li key={page.href}>
                      <Link to={page.href}>{page.title}</Link>
                    </li>
                  ))}
                </ul>
              ) : null}
            </dd>
          </div>
        );
      })}
    </dl>
  );
}

/**
 * The top of a section's landing page: the three to five pages most of its
 * readers open first, as a grid a reader takes in at a glance, with the first
 * marked as the place to start. The full list in reading order follows it on
 * the page; this is not a second copy of that list but the part of it that
 * matters most, which a flat numbered list cannot show.
 *
 * `section` is the section's title, and a title nothing matches fails the
 * build the same way `SectionContents` does.
 */
export component SectionLead(section: string) {
  const featured = featuredIn(sectionTitled(section));

  return (
    <ol className="lead-links">
      {featured.map((page, index) => (
        <li key={page.href}>
          <Link to={page.href}>
            {index === 0 ? <span className="lead-links-first">Start here</span> : null}
            <strong>{page.title}</strong>
            <span>{page.blurb}</span>
          </Link>
        </li>
      ))}
    </ol>
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
 * The foot of a manual page: the page before it in its section, and the next
 * page in reading order. A `nav` with `rel` links, so a reader, a screen
 * reader's landmark list and a browser's own "next page" all find the pair.
 */
export component PageTurn(previous: ?Entry, next: ?Entry) {
  if (previous == null && next == null) {
    return null;
  }
  return (
    <nav className="page-turn" aria-label="Pages">
      {previous != null ? (
        <Link to={previous.href} rel="prev" className="page-turn-previous">
          <span className="page-turn-label">Previous</span>
          <span>
            <span aria-hidden="true">← </span>
            {previous.title}
          </span>
        </Link>
      ) : null}
      {next != null ? (
        <Link to={next.href} rel="next" className="page-turn-next">
          <span className="page-turn-label">Next</span>
          <span>
            {next.title}
            <span aria-hidden="true"> →</span>
          </span>
        </Link>
      ) : null}
    </nav>
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

/**
 * What a reader can do, and the pages that get them there: one row per goal
 * in `goals.js`, its status in words, and its path as numbered links.
 *
 * A list of lists rather than a drawing, because that is what it is and what a
 * screen reader should hear — a goal, whether it works, and the steps in
 * order. The arrows between steps are CSS, so they are not read out, and the
 * status is a word as well as a colour. `compact` is the home page's version:
 * only the goals marked `onHome`, without the outcome line, and a link to the
 * whole map.
 */
export component GoalMap(compact: boolean = false) {
  const shown = compact ? goals.filter((goal) => goal.onHome) : goals;

  return (
    <div className={compact ? "goals goals-compact" : "goals"}>
      <ol className="goals-list">
        {shown.map((goal) => (
          <GoalRow key={goal.title} goal={goal} compact={compact} />
        ))}
      </ol>
      {compact ? (
        <p className="goals-more">
          <Link to="/guide/goals">
            Every goal, with what each path covers <span aria-hidden="true">→</span>
          </Link>
        </p>
      ) : null}
    </div>
  );
}

component GoalRow(goal: Goal, compact: boolean) {
  return (
    <li className="goal">
      <div className="goal-head">
        <h3>{goal.title}</h3>
        <span className="goal-status" data-status={goal.status.toLowerCase()}>
          {goal.status}
        </span>
      </div>
      {compact ? null : <p className="goal-outcome">{goal.outcome}</p>}
      {goal.caveat != null && !compact ? <p className="goal-caveat">{goal.caveat}</p> : null}
      <ol className="goal-steps" aria-label={`Steps to ${goal.title.toLowerCase()}`}>
        {goal.steps.map((step, index) => (
          <li key={step.href + step.label}>
            {/* The list is already numbered for a screen reader; this is the visible number. */}
            <span className="goal-step-number" aria-hidden="true">
              {index + 1}
            </span>
            <Link to={step.href}>{step.label}</Link>
          </li>
        ))}
      </ol>
    </li>
  );
}
