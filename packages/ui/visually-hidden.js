// @flow
"use client";
//
// Text for assistive technology and nobody else, and the announcer built on it.
//
// # `VisuallyHidden`
//
// `display: none` and `visibility: hidden` take an element out of the
// accessibility tree as well as off the screen, which is the opposite of what a
// visually hidden label is for. The style in `internal/visually-hidden-style.js`
// is the one that survives every engine's accessibility mapping: a one-pixel
// box, clipped twice (`clip` for the engines that predate `clip-path`), and
// `white-space: nowrap` so a screen reader's virtual cursor does not read a
// clipped paragraph one word per line.
//
// `focusable` is for the skip link: hidden until a keyboard lands on it, then
// on screen while focus is anywhere inside, because a focus ring around a
// one-pixel box is a focus ring nobody can see.
//
// # `announce`
//
// A live region only announces a *change*, and only when the region was in the
// document before the change — so a region rendered together with its message
// says nothing, and a component that renders its own region next to itself has
// to exist, silent, before the thing it wants to say happens. Every component
// that did that also put an element in the caller's layout, and three of them
// (the collections, the range calendar and the segmented fields) left theirs
// visible, so "3 selected" was printed on the page under the list.
//
// `announce()` is one pair of regions for the whole document, created on the
// first call and kept. It follows React Aria's LiveAnnouncer, and not by
// accident:
//
//   * one `role="log"` region per politeness, since a region's politeness is
//     read when the region is first seen and cannot be changed on a live one;
//   * each message is a new child node rather than a new text value, so the
//     same message twice is announced twice (`aria-relevant="additions"`);
//   * each message is removed after `timeout`, so a reader who arrives at the
//     end of the document later does not find a transcript there;
//   * the first message waits 100 ms after the regions are created, because a
//     region and its first content inserted together are, again, silent.
//
// On the server there is no document and `announce` does nothing; there is
// nothing to hydrate either, because the regions are never rendered by React.

import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import { composeHandlers, withProps, withoutComposed } from "./internal/merge-props.js";
import type { RenderProp, Rest } from "./internal/merge-props.js";
import { visuallyHiddenStyle } from "./internal/visually-hidden-style.js";

type FocusEvent = {
  readonly currentTarget: mixed,
  readonly relatedTarget: mixed,
  readonly defaultPrevented: boolean,
  ...
};

/**
 * Content a screen reader reads and a sighted reader does not see.
 *
 *     <button><Icon name="trash" /><VisuallyHidden>Delete draft</VisuallyHidden></button>
 *     <VisuallyHidden focusable render={(props) => <a href="#main" {...props} />}>
 *       Skip to content
 *     </VisuallyHidden>
 *
 * A caller's `style` is kept, underneath the hiding, so it applies again the
 * moment a `focusable` one is shown.
 */
export component VisuallyHidden(
  children?: React.Node,
  /** Show the content while focus is inside it: the skip-link pattern. */
  focusable: boolean = false,
  render?: RenderProp,
  ...rest: Rest
) {
  const [focused, setFocused] = useState(false);
  const style = rest.style;
  const own = typeof style === "object" && style != null ? style : {};
  const hidden = !(focusable && focused);
  const props = withProps(withoutComposed(rest, ["onFocus", "onBlur"]), {
    children,
    style: hidden ? { ...own, ...visuallyHiddenStyle } : style,
    onFocus: composeHandlers(rest.onFocus, () => {
      if (focusable) setFocused(true);
    }),
    onBlur: composeHandlers(rest.onBlur, (event: FocusEvent) => {
      // Moving between two links inside one skip block is not leaving it.
      const { currentTarget, relatedTarget } = event;
      if (
        currentTarget instanceof Node &&
        relatedTarget instanceof Node &&
        currentTarget.contains(relatedTarget)
      )
        return;
      setFocused(false);
    }),
  });
  return render != null ? render(props) : <span {...props} />;
}

export type Politeness = "polite" | "assertive";

export type AnnounceOptions = {|
  /**
   * `"polite"` (the default) waits for the reader to finish; `"assertive"`
   * interrupts, and is for what cannot wait — an error that stopped a save.
   */
  readonly politeness?: Politeness,
  /** Milliseconds before the message leaves the document. 7000 by default. */
  readonly timeout?: number,
|};

type Announcer = {|
  readonly root: HTMLElement,
  readonly polite: HTMLElement,
  readonly assertive: HTMLElement,
  /** When the regions went into the document; the first message waits for them. */
  readonly created: number,
|};

let announcer: Announcer | null = null;

/** Wait this long after creating the regions, so their first content is a change. */
const SETTLE_MS = 100;

function region(document: Document, politeness: Politeness): HTMLElement {
  const element = document.createElement("div");
  element.setAttribute("role", "log");
  element.setAttribute("aria-live", politeness);
  element.setAttribute("aria-relevant", "additions");
  return element;
}

function announcerIn(document: Document): Announcer | null {
  const body = document.body;
  if (body == null) return null;
  // A test's cleanup, or an app that replaces `<body>`, can take the regions
  // out; a region that is not in the document announces nothing.
  if (announcer != null && announcer.root.isConnected && announcer.root.ownerDocument === document)
    return announcer;
  const root = document.createElement("div");
  root.setAttribute("data-uf-live-announcer", "");
  root.style.cssText =
    "position:absolute;width:1px;height:1px;margin:-1px;padding:0;border:0;" +
    "overflow:hidden;clip:rect(0, 0, 0, 0);clip-path:inset(50%);white-space:nowrap";
  const assertive = region(document, "assertive");
  const polite = region(document, "polite");
  root.append(assertive, polite);
  // A direct child of `<body>`, which is the level a modal's conceal walk
  // reaches last; `dialog.js` skips it by the attribute above.
  body.prepend(root);
  announcer = { root, polite, assertive, created: Date.now() };
  return announcer;
}

/**
 * Say something to a screen reader, from anywhere.
 *
 *     announce(`${count} results`);
 *     announce("Could not save the draft", { politeness: "assertive" });
 *
 * A function rather than a hook, because what needs announcing comes from
 * event handlers, effects and `catch` blocks — `toast()` makes the same choice.
 * An empty message is ignored rather than announced as silence.
 */
export function announce(message: string, options?: AnnounceOptions): void {
  if (message.trim() === "" || typeof document === "undefined") return;
  const current = announcerIn(document);
  if (current == null) return;
  const politeness = options?.politeness ?? "polite";
  const timeout = options?.timeout ?? 7000;
  const target = politeness === "assertive" ? current.assertive : current.polite;
  const insert = () => {
    const node = document.createElement("div");
    node.textContent = message;
    target.append(node);
    if (timeout > 0 && Number.isFinite(timeout)) setTimeout(() => node.remove(), timeout);
  };
  const wait = current.created + SETTLE_MS - Date.now();
  if (wait > 0) setTimeout(insert, wait);
  else insert();
}

/** Take every message still in the regions out, for one politeness or both. */
export function clearAnnouncements(politeness?: Politeness): void {
  const current = announcer;
  if (current == null) return;
  if (politeness !== "polite") current.assertive.replaceChildren();
  if (politeness !== "assertive") current.polite.replaceChildren();
}
