"use client";
"use flow";
//
// Which sections of the sidebar stay open.
//
// The sidebar remounts when a reader crosses from the guide to the reference,
// so the open sections cannot live in component state: that state is what a
// navigation threw away. `sessionStorage` outlives the remount and does not
// outlive the tab. It is also a browser API, and so is the subscription that
// reads it, which is why this list is its own client module rather than part
// of `parts.js`. A server module that named either would be rejected at build
// time, and the rest of that file is markup the server renders.
//
// This module still renders on the server. The server, and the first client
// render, answer an empty list, and the section of the open page is added on
// top, so the markup the server sent and the markup hydration sees both open
// only that section. The tab's remembered sections arrive on the render after
// hydration.
// The snapshot is a string because `useSyncExternalStore` compares with
// `Object.is`, and a freshly parsed array never compares equal to the previous
// one.

import * as React from "@uniflowed/react";
import { useSyncExternalStore } from "@uniflowed/react";
import { Link } from "@uniflowed/router";

import { sections, sectionsAfterToggle, sectionsKeptOpen } from "./nav.js";

const NAV_OPEN = "uf-docs-nav-open";
const NAV_EMPTY = "[]";

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
  // `typeof` still evaluates the getter. A browser that has storage blocked
  // throws there, before the method call, and that throw happens during
  // render. The catch is the answer for that and for a window with no
  // `sessionStorage` at all.
  try {
    if (typeof sessionStorage === "undefined") {
      return NAV_EMPTY;
    }
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
  try {
    if (typeof sessionStorage !== "undefined") {
      sessionStorage.setItem(NAV_OPEN, raw);
    }
  } catch {
    // A private window, or one that has storage blocked, still toggles for
    // this page. The property access is what throws, so it stays inside the
    // catch.
  }
  notifyNav();
}

/**
 * The disclosures. `currentTitle` is the section the open page is in, and
 * `marked` is that page's href. Either is null on a page the manual does not
 * list. The page's own section is open even when storage has not caught up,
 * and even when a reader closed it on the previous page: the marked link has
 * to be visible. A section they opened besides that one stays open.
 */
export component ManualNavSections(currentTitle: string | null, marked: string | null) {
  const remembered = parseRemembered(
    useSyncExternalStore(subscribeNav, readSnapshot, serverSnapshot),
  );
  const openTitles = sectionsKeptOpen(remembered, currentTitle);
  const isOpen = (title: string): boolean => openTitles.includes(title);

  return sections.map((section) => {
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
          // The section the page is in stays open. The browser has already
          // closed it; putting it back agrees with the `open` the next render
          // sets, and does not wait on that render.
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
                {page === section.landing && page.title === section.title ? "Overview" : page.title}
              </Link>
            </li>
          ))}
        </ul>
      </details>
    );
  });
}
