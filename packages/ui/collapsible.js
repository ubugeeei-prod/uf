// @flow
//
// A button and the region it shows: the disclosure pattern, on its own.
//
// This is the smallest component in the package and it is here because the
// three attributes it gets right are the three everybody leaves out.
// `<button onClick={() => setOpen(!open)}>` with a `{open && <div>…</div>}`
// after it looks finished and tells a screen reader nothing: not that the
// button controls anything, not whether the thing is showing, and not which
// region it is. A reader hears "Details, button" and has no way to know that
// pressing it changed the page.
//
// So: `aria-expanded` on the trigger, `aria-controls` naming the content —
// and only while there is content to name, because an `aria-controls` pointing
// at an id nothing has is a promise the component cannot keep.
//
// # The closed content stays in the document
//
// `Tabs.Panel` returns `null` when it is not selected and that is right for a
// tab set. Here it is wrong, and the reason is the browser's find-in-page: text
// in a section that is not in the document cannot be found, so a page of
// collapsed sections is a page a reader has to open by hand to search.
// `internal/disclosure.js` explains what is done instead, and why React needs a
// hook to say it.
//
// # The height, for a stylesheet that animates it
//
// `measure` puts the height the content *would* have on
// `Collapsible.Content` as `--uf-collapsible-height`, correct while the panel
// is still closed — which is the only moment it is any use, because
// `height: 0 → var(--uf-collapsible-height)` is a transition that has to know
// its destination before it starts. `internal/disclosure.js` holds the
// measuring pass and says why the obvious ways of asking all answer zero, and
// why the prop is opt-in rather than always on.
//
//   [data-collapsible-content] {
//     overflow: hidden;
//     transition: height 150ms;
//     height: 0;
//   }
//   [data-collapsible-content]:not([hidden]) {
//     height: var(--uf-collapsible-height);
//   }
//
// The selector is the caller's — a class, a `data-*` of their own, whatever
// they already style with. This package emits the number and no styles at all,
// which is the same division `internal/anchor.js` keeps for a popover: a
// stylesheet can say *how* to move, and only the component can say how far.

"use client";

import * as React from "@uniflowed/react";
import { createContext, useContext, useId, useMemo, useRef, useState } from "@uniflowed/react";

import type { RenderProp, Rest } from "./internal/merge-props.js";
import {
  composeHandlers,
  composeRefs,
  withProps,
  withoutComposed,
} from "./internal/merge-props.js";
import { useMeasuredHeight, usePresence, useUntilFound } from "./internal/disclosure.js";
import { useControlled } from "./internal/controlled-state.js";

type CollapsibleState = {|
  readonly contentId: string,
  readonly open: boolean,
  readonly setOpen: (open: boolean) => void,
  /** Whether a `Collapsible.Content` is rendered, so the trigger names one that exists. */
  readonly present: boolean,
  readonly registerContent: (present: boolean) => void,
  /** Whether the content carries its measured height; see the module header. */
  readonly measure: boolean,
|};

const CollapsibleContext: React.Context<CollapsibleState | null> = createContext(null);

hook useCollapsible(part: string): CollapsibleState {
  const state = useContext(CollapsibleContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside a Collapsible.Root`);
  }
  return state;
}

/**
 * The pair, and the state they agree about.
 *
 * Renders no element of its own: a trigger and its content are siblings in
 * whatever layout the caller wrote, and a wrapper would put a `<div>` between
 * them that the caller then has to style around. `Menu.Root` makes the same
 * choice for the same reason.
 */
export component CollapsibleRoot(
  children: React.Node,
  defaultOpen?: boolean = false,
  open?: boolean,
  onOpenChange?: (open: boolean) => void,
  measure?: boolean = false,
) {
  const contentId = `${useId()}-content`;
  const [isOpen, setOpen] = useControlled(open, defaultOpen, onOpenChange);
  const [present, setPresent] = useState(false);

  const state = useMemo(
    () => ({ contentId, open: isOpen, setOpen, present, registerContent: setPresent, measure }),
    [contentId, isOpen, setOpen, present, measure],
  );

  return <CollapsibleContext.Provider value={state}>{children}</CollapsibleContext.Provider>;
}

/** The control that shows and hides the content. */
export component CollapsibleTrigger(
  children: React.Node,
  disabled?: boolean = false,
  render?: RenderProp,
  ...rest: Rest
) {
  const collapsible = useCollapsible("Collapsible.Trigger");
  const props = withProps(withoutComposed(rest, ["onClick"]), {
    // Named only while the content is in the document. A caller who renders
    // the content conditionally — or not at all until data arrives — would
    // otherwise have this trigger pointing at nothing.
    "aria-controls": collapsible.present ? collapsible.contentId : undefined,
    "aria-expanded": collapsible.open ? "true" : "false",
    children,
    disabled,
    onClick: composeHandlers(rest.onClick, () => {
      if (!disabled) {
        collapsible.setOpen(!collapsible.open);
      }
    }),
  });

  if (render != null) {
    return render(props);
  }
  return <button {...props} type="button" />;
}

/**
 * The region the trigger shows.
 *
 * It is always rendered and `hidden` while closed, rather than removed — see
 * the module header, and `internal/disclosure.js` for what `hidden` is upgraded
 * to and why that takes an effect.
 */
export component CollapsibleContent(children: React.Node, render?: RenderProp, ...rest: Rest) {
  const collapsible = useCollapsible("Collapsible.Content");
  const contentRef = useRef<HTMLElement | null>(null);
  usePresence(collapsible.registerContent);
  useUntilFound(contentRef, collapsible.open);
  useMeasuredHeight(contentRef, collapsible.measure);

  const props = withProps(withoutComposed(rest, ["ref"]), {
    children,
    hidden: !collapsible.open,
    id: collapsible.contentId,
    ref: composeRefs(rest.ref, (element: HTMLElement | null) => {
      contentRef.current = element;
    }),
  });

  if (render != null) {
    return render(props);
  }
  return <div {...props} />;
}
