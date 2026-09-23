"use client";
// @flow
//
// Search.
//
// A button in the masthead, and `/` or ⌘K / Ctrl+K from anywhere, open a modal
// `<dialog>` with one field and a list of results. The dialog is the browser's
// own, so focus is held inside it, Escape closes it and the page behind it is
// inert without any of that being written here.
//
// The field is a combobox in the ARIA sense: focus stays in it while the arrow
// keys move a highlighted result, announced through `aria-activedescendant`,
// and Enter follows the highlighted one. Each result is still an ordinary link,
// so a pointer, a middle click and "open in new tab" all do what they do
// everywhere else.
//
// What it searches is two things, in this order of arrival. The page titles
// and one-line blurbs from `nav.js` are in this module, so a reader who opens
// the dialog before anything has loaded can already find a page by name. The
// full index — every heading of every page and the text under it — is fetched
// the first time the dialog opens, from the file `tools/docs/search-index.js`
// writes into the built site. `uf dev` does not write it; there the titles are
// all there is, and the dialog says so.

import * as React from "@uniflowed/react";
import { useEffect, useId, useRef, useState } from "@uniflowed/react";

import { pages, sectionFor } from "./nav.js";
import { INDEX_PATH, readIndex, search, segments } from "./search.js";
import type { SearchEntry, SearchResult } from "./search.js";

type IndexState = "idle" | "loading" | "ready" | "missing";

/** What is searchable before the index arrives: each page's title and blurb. */
const TITLES: $ReadOnlyArray<SearchEntry> = pages.map((page) => ({
  href: page.href,
  page: page.title,
  section: sectionFor(page.href)?.title ?? "",
  heading: null,
  text: page.blurb,
}));

export component SearchDialog() {
  const dialog = useRef<?HTMLDialogElement>(null);
  const field = useRef<?HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const [entries, setEntries] = useState<$ReadOnlyArray<SearchEntry>>(TITLES);
  const [indexState, setIndexState] = useState<IndexState>("idle");
  const id = useId();

  const results: $ReadOnlyArray<SearchResult> = search(entries, query, 12);

  const open = () => {
    const element = dialog.current;
    if (element == null || element.open) {
      return;
    }
    element.showModal();
    field.current?.select();
    if (indexState === "idle") {
      setIndexState("loading");
      fetch(INDEX_PATH)
        .then((response) => (response.ok ? response.json() : null))
        .then((value) => {
          const index = readIndex(value);
          if (index == null) {
            setIndexState("missing");
          } else {
            setEntries(index.entries);
            setIndexState("ready");
          }
        })
        .catch(() => setIndexState("missing"));
    }
  };

  const close = () => {
    dialog.current?.close();
  };

  // A document load, not `router.push`. Every page of the manual is a server
  // page, which the router hands to the browser as a document load anyway, and
  // doing it here keeps the `#heading` the result is for: the browser scrolls
  // to it once the page is there, which is the thing a result has to do.
  const follow = (href: string) => {
    close();
    window.location.assign(href);
  };

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const shortcut =
        (event.key === "k" && (event.metaKey || event.ctrlKey)) ||
        (event.key === "/" && !event.metaKey && !event.ctrlKey && !event.altKey && !typing(event));
      if (shortcut) {
        event.preventDefault();
        open();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  });

  const optionId = (index: number) => `${id}-result-${index}`;
  const listId = `${id}-results`;
  const trimmed = query.trim();

  return (
    <>
      <button className="search-button" type="button" onClick={open} aria-haspopup="dialog">
        <span>Search</span>
        <kbd aria-hidden="true">/</kbd>
      </button>
      <dialog
        ref={dialog}
        className="search"
        aria-label="Search the manual"
        // A click outside the panel closes it, where the browser supports
        // saying so; everywhere else Escape and the button in the field do.
        closedby="any"
      >
        <div className="search-panel">
          <div className="search-field">
            <input
              ref={field}
              type="search"
              role="combobox"
              aria-expanded={results.length > 0}
              aria-controls={listId}
              aria-autocomplete="list"
              aria-activedescendant={results.length > 0 ? optionId(active) : undefined}
              aria-label="Search the manual"
              placeholder="Search commands, options, guides…"
              autoComplete="off"
              spellCheck={false}
              value={query}
              onChange={(event) => {
                setQuery(event.currentTarget.value);
                setActive(0);
              }}
              onKeyDown={(event) => {
                if (event.key === "ArrowDown" || event.key === "ArrowUp") {
                  event.preventDefault();
                  if (results.length > 0) {
                    const step = event.key === "ArrowDown" ? 1 : -1;
                    setActive((active + step + results.length) % results.length);
                  }
                } else if (event.key === "Enter") {
                  const chosen = results[active];
                  if (chosen != null) {
                    event.preventDefault();
                    follow(chosen.entry.href);
                  }
                }
              }}
            />
            <button className="search-close" type="button" onClick={close}>
              Esc
            </button>
          </div>

          <div className="search-results" id={listId} role="listbox" aria-label="Results">
            {results.map((result, index) => (
              <div
                key={result.entry.href}
                id={optionId(index)}
                role="option"
                aria-selected={index === active}
              >
                <a
                  href={result.entry.href}
                  tabIndex={-1}
                  onMouseMove={() => {
                    if (active !== index) {
                      setActive(index);
                    }
                  }}
                  // The link does the navigating; this only takes the dialog
                  // down, which a link to a heading on this same page would
                  // otherwise leave open over the place it scrolled to.
                  onClick={close}
                >
                  <span className="search-where">
                    {result.entry.section === "" ? null : `${result.entry.section} · `}
                    {result.entry.heading == null ? "Page" : result.entry.page}
                  </span>
                  <span className="search-title">
                    <Marked value={result.entry.heading ?? result.entry.page} query={query} />
                  </span>
                  {result.excerpt === "" ? null : (
                    <span className="search-excerpt">
                      <Marked value={result.excerpt} query={query} />
                    </span>
                  )}
                </a>
              </div>
            ))}
          </div>

          <p className="search-status" role="status">
            {status(trimmed, results.length, indexState)}
          </p>
        </div>
      </dialog>
    </>
  );
}

component Marked(value: string, query: string) {
  return (
    <>
      {segments(value, query).map((segment) =>
        segment.match ? (
          <mark key={segment.start}>{segment.text}</mark>
        ) : (
          <React.Fragment key={segment.start}>{segment.text}</React.Fragment>
        ),
      )}
    </>
  );
}

/** The line under the results, which a screen reader hears as they change. */
function status(query: string, count: number, index: IndexState): string {
  const scope =
    index === "ready"
      ? ""
      : index === "missing"
        ? " Page titles only: the full index is written by `uf run docs:build`."
        : " Page titles only, while the full index loads.";
  if (query === "") {
    return `Type to search every page and heading.${scope}`;
  }
  if (count === 0) {
    return `Nothing matches “${query}”.${scope}`;
  }
  return `${count === 12 ? "The first 12" : count} ${count === 1 ? "result" : "results"}. ↑ ↓ to move, Enter to open.${scope}`;
}

/** Whether a key press is text being typed into something on the page. */
function typing(event: KeyboardEvent): boolean {
  const target = event.target;
  if (!(target instanceof HTMLElement)) {
    return false;
  }
  return (
    target.isContentEditable ||
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement
  );
}
