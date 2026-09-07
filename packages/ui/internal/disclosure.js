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
// # The height a closed panel would have
//
// The third rule, and the one that took a piece of work rather than a line.
// `height: 0 → var(--uf-collapsible-height)` is the whole of animating a
// disclosure, and the number in that property is the one thing a stylesheet
// cannot compute: it is the height the content *would* have, wanted at the
// moment the panel is still closed, because a transition has to know its
// destination before it starts.
//
// Every obvious way of getting it answers zero. A closed panel is `hidden`, so
// it has no box: `ResizeObserver` reports `0`, `getBoundingClientRect()` is
// empty, `scrollHeight` is `0`, and `useElementSize` from
// `@uniflowed/hooks/dom` measures a hidden element and reports zero — which is
// exactly the moment the number is wanted. Measuring after the panel opens
// gives the right number one frame late, which is the jank this removes.
//
// So `useMeasuredHeight` lays the panel out without painting it: inline
// `display`, `position: absolute` and `visibility: hidden`, read, restore. Four
// things about that are not obvious:
//
//   * It overrides `display` inline rather than removing `hidden`. The
//     attribute is `useUntilFound`'s and React's; an effect that took it away
//     and put it back would be fighting both of them for one frame, and would
//     lose whichever ran last. An inline declaration beats the user-agent
//     stylesheet's `[hidden] { display: none }` and touches nothing anybody
//     else believes they own.
//   * It sets `content-visibility: visible` in the same pass, because
//     `hidden="until-found"` is `content-visibility: hidden`, which does not lay
//     its subtree out either. Defeating one of the two and not the other
//     measures zero on exactly the panels this package ships.
//   * It sets `height: auto` in the same pass, for the same reason and against
//     the very rule this property exists for. A closed panel is `height: 0` —
//     that is the half of `height: 0 → var(--uf-collapsible-height)` that is
//     always on — so a panel laid out with the stylesheet still applying
//     measures zero and writes `0px` back into the property it was asked to
//     fill. Defeating `display` and not `height` is the same mistake as
//     defeating `display` and not `content-visibility`, one declaration along.
//   * And it pins the width, because taking a box out of flow makes it
//     shrink-to-fit against its containing block — the nearest positioned
//     ancestor, which on most pages is the viewport. Text that wraps to four
//     lines where the panel lives measures one line there. The width it would
//     have in flow is its parent's content box; when that cannot be read the
//     pass leaves the width alone rather than inventing one.
//
// It is opt-in, and that is the honest answer to "it should cost nothing on a
// page that never animates". Whether a stylesheet reads the property is not
// something the component can ask — `getComputedStyle` on a `display: none`
// element answers about the declaration, not about whether anybody transitions
// on it — so the alternative to a prop is a forced layout per panel per render
// on every page that has a collapsible, animated or not. A forty-section FAQ is
// forty synchronous layouts nobody asked for.
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

/** The custom property a stylesheet transitions a disclosure's height to. */
export const HEIGHT_PROPERTY: string = "--uf-collapsible-height";

/**
 * The width `element` would have in flow, as a CSS length, or nothing.
 *
 * Taking the panel out of flow to measure it costs shrink-to-fit: an absolutely
 * positioned box with `width: auto` is as wide as its *content* wants to be,
 * bounded by its containing block — which is the nearest positioned ancestor
 * and, on most pages, is the viewport rather than the panel's parent. A
 * paragraph that wraps to four lines in a sidebar measures one line there, and
 * the number written into the property is then a height the panel never has.
 *
 * The width it would have in flow is its parent's content box, which is what
 * `getComputedStyle` reports for `width` on a laid-out element. Nothing is
 * returned when the answer is not a length — a parent that is itself
 * `display: none`, or a DOM with no layout to report — and the pass then does
 * what it did before rather than pinning a width it had to guess.
 */
function widthInFlow(element: HTMLElement): string | null {
  const parent = element.parentElement;
  const view: $FlowFixMe = element.ownerDocument?.defaultView;
  if (parent == null || view == null || typeof view.getComputedStyle !== "function") {
    return null;
  }
  const width: mixed = view.getComputedStyle(parent).width;
  return typeof width === "string" && width.endsWith("px") ? width : null;
}

/**
 * The height `element` has, or would have if it were not hidden.
 *
 * The measuring pass, and the reason this module has a header section about it.
 * An open panel is measured where it stands; a closed one is briefly laid out
 * and not painted. Either way this reads layout, which is a synchronous reflow
 * — the caller is the one that decides it is worth paying.
 *
 * # The two things a closed panel is not, and has to be made
 *
 * Laying it out is necessary and is not sufficient, because the stylesheet this
 * property exists for is `height: 0` on the closed panel and
 * `height: var(--uf-collapsible-height)` on the open one. A panel laid out with
 * that rule still applying measures **zero**, the property is written back as
 * `0px`, and the transition has a destination of nothing — the bug the whole
 * hook was written to avoid, arriving through the rule it was written for. So
 * `height: auto` is set inline for the pass, exactly as `display` is: not
 * because the panel wants an inline height but because the author declaration
 * has to be defeated for one synchronous read and put back.
 *
 * `width` is the same argument for the other axis and is `widthInFlow`'s. Both
 * are restored with everything else; a stylesheet is never left fighting an
 * inline declaration this wrote.
 */
function heightOf(element: HTMLElement): number {
  if (!element.hasAttribute("hidden")) {
    return element.getBoundingClientRect().height;
  }
  const style = element.style;
  const before = {
    boxSizing: style.boxSizing,
    contentVisibility: style.getPropertyValue("content-visibility"),
    display: style.display,
    height: style.height,
    position: style.position,
    visibility: style.visibility,
    width: style.width,
  };
  // Out of flow and unpainted, so nothing below the panel moves and no frame
  // shows it. The order is not significant; this is all one style
  // recalculation, paid for by the single read below.
  style.display = "block";
  style.setProperty("content-visibility", "visible");
  style.position = "absolute";
  style.visibility = "hidden";
  style.height = "auto";
  const width = widthInFlow(element);
  if (width != null) {
    // `border-box`, because what fills the parent's content width in flow is
    // the panel's margin box rather than its content box: measuring a padded
    // panel content-box wide would make it wider than it will ever be and its
    // text shorter than it will ever wrap to.
    style.boxSizing = "border-box";
    style.width = width;
  }
  const height = element.getBoundingClientRect().height;
  style.boxSizing = before.boxSizing;
  style.display = before.display;
  style.setProperty("content-visibility", before.contentVisibility);
  style.height = before.height;
  style.position = before.position;
  style.visibility = before.visibility;
  style.width = before.width;
  return height;
}

/**
 * Keep `HEIGHT_PROPERTY` on a disclosure's panel equal to its content's height.
 *
 * `enabled` is the caller's opt-in; see the module header for why there is one.
 * When it is off this writes nothing and measures nothing, which is what makes
 * a page with no animation pay nothing.
 *
 * Two effects rather than one, because they answer different questions. The
 * first measures on every render, with no dependency array on purpose: what the
 * panel would be worth changes when the *caller* renders different children
 * into it, and a dependency list here would be a claim about when that happens
 * that only the caller could keep — `useFirstItem` in `roving-focus.js` makes
 * the same argument for the same reason. The second watches for a size change
 * no render caused: an image that finished loading, a font that swapped. It
 * only ever fires while the panel is open, because a `display: none` element
 * has no box for a `ResizeObserver` to report on — which is the whole problem
 * this hook exists for, arriving one more time.
 */
export hook useMeasuredHeight(ref: { current: HTMLElement | null }, enabled: boolean): void {
  useEffect(() => {
    const element = ref.current;
    if (!enabled || element == null) {
      return;
    }
    const measured = `${String(heightOf(element))}px`;
    // Compared before writing, so a render that changed nothing does not dirty
    // the element's style and invite another style recalculation.
    if (element.style.getPropertyValue(HEIGHT_PROPERTY) !== measured) {
      element.style.setProperty(HEIGHT_PROPERTY, measured);
    }
  });

  useEffect(() => {
    const element = ref.current;
    const view = element?.ownerDocument?.defaultView;
    if (!enabled || element == null || view == null) {
      return;
    }
    // Read off the window rather than through a local, for the reason
    // `internal/anchor.js` gives where it does the same: a capitalised name
    // holding a constructor is read as a React component by `uf lint`, and the
    // window's own property is the thing being asked about anyway.
    const host: $FlowFixMe = view;
    if (typeof host.ResizeObserver !== "function") {
      return;
    }
    const sizes = new host.ResizeObserver(() => {
      element.style.setProperty(HEIGHT_PROPERTY, `${String(heightOf(element))}px`);
    });
    sizes.observe(element);
    return () => sizes.disconnect();
  }, [ref, enabled]);
}
