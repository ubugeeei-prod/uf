// @flow
//
// The other pattern every part of this package keeps writing.
//
// `roving-focus.js` is the keyboard half of these components. This is the
// other half, and it is one sentence: **a button says whether a region is
// showing, and names it.** `Dialog.Trigger` and `Menu.Trigger` are that
// sentence, and so are a collapsible, an accordion header and an expandable
// entry in a site's navigation — three components that look nothing alike and
// are the same three attributes underneath.
//
// Two rules make it up, and both of them fail silently.
//
//   * **Name it only while it is there.** `aria-controls` pointing at an id
//     nothing has tells a reader there is somewhere to go and then has nowhere
//     to send them, and `aria-labelledby` pointing at a missing element makes a
//     screen reader announce *nothing at all* rather than falling back to the
//     element's own text. So a panel reports whether it is in the document, and
//     whatever names it only claims the name while the report says yes. This is
//     the same subscription `Tabs.Panel` makes to `Tabs.Tab`, for the same
//     reason.
//   * **A closed panel is hidden, not absent.** `Tabs.Panel` returns `null`
//     when it is not selected, which is right for a tab set — the panels are
//     alternatives, and a reader looking for text in one of them is looking at
//     the wrong tab. It is wrong for a disclosure: the browser's find-in-page
//     cannot find text in a section that is not in the document, so a
//     forty-section FAQ becomes forty sections a reader has to open by hand to
//     search. `hidden="until-found"` is the platform's answer — the browser
//     reveals the section, fires `beforematch`, and scrolls to the match —
//     and it works in Chrome since 102 (2022-05), Firefox since 148 (2026-02)
//     and Safari since 26.2 (2025-12, which does not yet scroll to the match);
//     checked 2026-09-06. Everywhere else it degrades to a plain `hidden`,
//     which is what the panel would have been anyway.
//
// # Why `useUntilFound` is a hook and not a prop
//
// Because React 19 cannot say `hidden="until-found"`. `hidden` is on React's
// list of boolean attributes, so `<div hidden="until-found">` renders
// `hidden=""` — the string is truthy, and truthy is all React keeps. There is
// no prop spelling that produces the attribute, which is a thing worth knowing
// before spending an afternoon looking for one.
//
// So the panel is rendered with the ordinary boolean `hidden` — which is what
// the server sends, and what keeps a closed section closed before any
// JavaScript arrives — and an effect *upgrades* the attribute afterwards. It is
// an upgrade rather than a fight: React sets `hidden=""` when it commits, this
// runs after that commit, and the next time React changes the prop it removes
// or re-adds the attribute and this upgrades it again. Nothing here writes an
// attribute React believes it owns while React believes it.
//
// # Why this is `internal/` and not a subpath
//
// The same reason `roving-focus.js` gives. These are rules about markup this
// package emits — that a trigger and its panel agree on an id, that a panel is
// the element carrying `hidden` — and they hold because the components build
// both halves. Exported, they would be advice.

import { useEffect } from "@uniflowed/react";

/**
 * Report that this part is in the document, for as long as it is.
 *
 * `register` is the setter from whatever names this part; it is called with
 * `true` on mount and `false` on unmount, and taking it as a possibly-missing
 * function lets a part be rendered outside the thing that would name it
 * without the caller having to care.
 */
export hook usePresence(register: ((present: boolean) => void) | void): void {
  useEffect(() => {
    if (register == null) {
      return;
    }
    register(true);
    return () => register(false);
  }, [register]);
}

/**
 * Keep a closed panel hidden the way the platform means it: findable.
 *
 * The element must be rendered with a plain boolean `hidden` as well — see the
 * module header. This only upgrades the attribute React has already committed,
 * so a browser that has never heard of `until-found` sees exactly the `hidden`
 * it would have seen, and one that has can reveal the section for a
 * find-in-page hit.
 */
export hook useUntilFound(ref: { current: HTMLElement | null }, open: boolean): void {
  useEffect(() => {
    const element = ref.current;
    // Nothing to do while it is open: React has removed the attribute, and
    // adding one back would hide a panel the reader just opened.
    if (element == null || open) {
      return;
    }
    element.setAttribute("hidden", "until-found");
  }, [ref, open]);
}
