"use client";
// @flow
//
// On this page.
//
// The headings of the article, listed beside it on a wide screen, with the one
// being read marked. Some pages of this manual are two thousand lines long, and
// without this the only way to see what is further down one is to scroll there.
//
// Read from the rendered article rather than handed down by the layout: the
// ids are the ones the markdown pipeline wrote, which is the only place they
// exist, and a page written as JavaScript has its outline by the same means as
// one written in MDX. The price is that the list is empty until the page has
// hydrated — the server and the first client render both produce an empty,
// hidden `<nav>`, so hydration has nothing to disagree about, and without
// JavaScript the page is exactly what it was before this existed.

import * as React from "@uniflowed/react";
import { useEffect, useState } from "@uniflowed/react";

type Heading = {| readonly id: string, readonly text: string, readonly depth: 2 | 3 |};

/** Fewer than this and a list of the page's headings says nothing the page does not. */
const MINIMUM = 3;

/**
 * `path` is the page's pathname. It is what tells this component that a
 * navigation replaced the article under it, since the component itself stays
 * mounted across one.
 */
export component Outline(path: string) {
  const [headings, setHeadings] = useState<$ReadOnlyArray<Heading>>([]);
  const [current, setCurrent] = useState<?string>(null);

  useEffect(() => {
    const article = document.getElementById("content");
    if (article == null) {
      return;
    }
    const found: Array<Heading> = [];
    for (const element of article.querySelectorAll("h2[id], h3[id]")) {
      const text = element.textContent.trim();
      if (text !== "") {
        found.push({ id: element.id, text, depth: element.tagName === "H3" ? 3 : 2 });
      }
    }
    // The outline is read from the document the server rendered, which does not
    // exist until after hydration; there is no render-time source for it.
    // uf-lint-disable-next-line react-compiler/set-state-in-effect
    setHeadings(found);
    // uf-lint-disable-next-line react-compiler/set-state-in-effect
    setCurrent(null);
  }, [path]);

  useEffect(() => {
    if (headings.length < MINIMUM) {
      return;
    }
    const elements = headings.map((heading) => document.getElementById(heading.id)).filter(Boolean);
    // The heading being read is the last one whose top has passed a line a
    // fifth of the way down the window — where the eye is, not where the
    // scrollbar is.
    let frame = 0;
    const measure = () => {
      frame = 0;
      const line = window.innerHeight / 5;
      let reading: ?string = null;
      for (const element of elements) {
        if (element.getBoundingClientRect().top <= line) {
          reading = element.id;
        } else {
          break;
        }
      }
      setCurrent(reading);
    };
    const onScroll = () => {
      if (frame === 0) {
        frame = window.requestAnimationFrame(measure);
      }
    };
    measure();
    window.addEventListener("scroll", onScroll, { passive: true });
    window.addEventListener("resize", onScroll, { passive: true });
    return () => {
      window.removeEventListener("scroll", onScroll);
      window.removeEventListener("resize", onScroll);
      if (frame !== 0) {
        window.cancelAnimationFrame(frame);
      }
    };
  }, [headings]);

  const shown = headings.length >= MINIMUM;

  return (
    <nav className="outline" aria-label="On this page" hidden={!shown}>
      {shown ? (
        <>
          <h2>On this page</h2>
          <ol>
            {headings.map((heading) => (
              <li key={heading.id} className={heading.depth === 3 ? "depth-3" : undefined}>
                <a
                  href={`#${heading.id}`}
                  aria-current={heading.id === current ? "location" : undefined}
                >
                  {heading.text}
                </a>
              </li>
            ))}
          </ol>
        </>
      ) : null}
    </nav>
  );
}
