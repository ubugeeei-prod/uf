"use client";
// @flow
//
// What the sidebar cannot say without a script: that it goes on.
//
// On a wide screen the sidebar is a scrolling box of its own, and a list cut
// off by the bottom of a box looks exactly like a list that ended there. A fade
// would say it, and this site has no gradients or shadows, so it is said in
// words and a rule instead: a hairline across the edge where the list
// continues, and under it a count of the entries below — which is also a
// button that scrolls to them. At the top, once the list has scrolled, the
// same hairline marks the edge it went under.
//
// It also scrolls the open page into view in the box when a page loads, so a
// reader deep in a long section sees where they are rather than the top of the
// list. The box is scrolled rather than the element asked to scroll itself
// into view, because that would scroll the whole page on a phone, where the
// sidebar is below the article and not a box at all.
//
// On a phone, and on any screen where the list fits, there is nothing below
// the fold, so this renders nothing. Without JavaScript the sidebar is the
// same disclosures, with the open section open and no count under them.

import { useEffect, useState } from "@uniflowed/react";

const LIST = "manual-nav-list";

/** `path` is what says a navigation replaced the page; the component stays mounted. */
export component NavOverflow(path: string) {
  const [below, setBelow] = useState(0);

  useEffect(() => {
    const list = document.getElementById(LIST);
    if (list == null) {
      return;
    }
    revealCurrent(list);
    const measure = () => {
      setBelow(countBelow(list));
      list.toggleAttribute("data-scrolled", list.scrollTop > 0);
    };
    measure();
    list.addEventListener("scroll", measure, { passive: true });
    // `toggle` does not bubble, so it is caught on the way down instead.
    list.addEventListener("toggle", measure, true);
    window.addEventListener("resize", measure);
    return () => {
      list.removeEventListener("scroll", measure);
      list.removeEventListener("toggle", measure, true);
      window.removeEventListener("resize", measure);
    };
  }, [path]);

  if (below === 0) {
    return null;
  }
  return (
    <button
      type="button"
      className="manual-nav-more"
      onClick={() => {
        const list = document.getElementById(LIST);
        if (list != null) {
          list.scrollBy({ top: list.clientHeight * 0.8, behavior: scrollBehavior() });
        }
      }}
    >
      {below} more below<span aria-hidden="true"> ↓</span>
    </button>
  );
}

/**
 * How many entries — section headings and the links of open sections — start
 * below the bottom edge of the box. A closed section's links are not counted:
 * they are not below, they are inside a heading that is.
 */
function countBelow(list: HTMLElement): number {
  if (list.scrollHeight <= list.clientHeight + 1) {
    return 0;
  }
  const edge = list.getBoundingClientRect().bottom;
  let count = 0;
  for (const entry of list.querySelectorAll("summary, details[open] a")) {
    if (entry.getBoundingClientRect().top >= edge - 4) {
      count += 1;
    }
  }
  return count;
}

/** Scroll the box, not the page, so the marked link sits in its upper third. */
function revealCurrent(list: HTMLElement): void {
  if (list.scrollHeight <= list.clientHeight + 1) {
    return;
  }
  const current = list.querySelector('a[aria-current="page"]');
  if (current == null) {
    return;
  }
  const box = list.getBoundingClientRect();
  const link = current.getBoundingClientRect();
  if (link.top >= box.top + 32 && link.bottom <= box.bottom - 48) {
    return;
  }
  list.scrollTop += link.top - box.top - box.height / 3;
}

function scrollBehavior(): "auto" | "smooth" {
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth";
}
