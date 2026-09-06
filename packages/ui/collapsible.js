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
// # No height, yet
//
// A collapsible that animates open needs the height its content *would* have,
// which a stylesheet cannot compute — the obvious `useElementSize` from
// `@uniflowed/hooks/dom` measures the element while it is hidden and reports
// zero, which is exactly the moment the number is wanted. Getting it right
// means a measuring pass with the panel briefly laid out and not painted, and
// that is a piece of work of its own rather than a line to be added here — #330.
// This component ships without it rather than with a custom property that reads
// `0px`.

"use client";

import * as React from "@uniflowed/react";
import { createContext, useContext, useId, useMemo, useRef, useState } from "@uniflowed/react";

import type { Rest } from "./internal/merge-props.js";
import { composeHandlers, composeRefs, withoutComposed } from "./internal/merge-props.js";
import { usePresence, useUntilFound } from "./internal/disclosure.js";
import { useControlled } from "./internal/controlled-state.js";

type CollapsibleState = {|
  readonly contentId: string,
  readonly open: boolean,
  readonly setOpen: (open: boolean) => void,
  /** Whether a `Collapsible.Content` is rendered, so the trigger names one that exists. */
  readonly present: boolean,
  readonly registerContent: (present: boolean) => void,
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
) {
  const contentId = `${useId()}-content`;
  const [isOpen, setOpen] = useControlled(open, defaultOpen, onOpenChange);
  const [present, setPresent] = useState(false);

  const state = useMemo(
    () => ({ contentId, open: isOpen, setOpen, present, registerContent: setPresent }),
    [contentId, isOpen, setOpen, present],
  );

  return <CollapsibleContext.Provider value={state}>{children}</CollapsibleContext.Provider>;
}

/** The button that shows and hides the content. */
export component CollapsibleTrigger(
  children: React.Node,
  disabled?: boolean = false,
  ...rest: Rest
) {
  const collapsible = useCollapsible("Collapsible.Trigger");
  const passed = withoutComposed(rest, ["onClick"]);

  return (
    <button
      {...passed}
      // Named only while the content is in the document. A caller who renders
      // the content conditionally — or not at all until data arrives — would
      // otherwise have this button pointing at nothing.
      aria-controls={collapsible.present ? collapsible.contentId : undefined}
      aria-expanded={collapsible.open ? "true" : "false"}
      disabled={disabled}
      onClick={composeHandlers(rest.onClick, () => {
        if (!disabled) {
          collapsible.setOpen(!collapsible.open);
        }
      })}
      type="button"
    >
      {children}
    </button>
  );
}

/**
 * The region the trigger shows.
 *
 * It is always rendered and `hidden` while closed, rather than removed — see
 * the module header, and `internal/disclosure.js` for what `hidden` is upgraded
 * to and why that takes an effect.
 */
export component CollapsibleContent(children: React.Node, ...rest: Rest) {
  const collapsible = useCollapsible("Collapsible.Content");
  const contentRef = useRef<HTMLElement | null>(null);
  usePresence(collapsible.registerContent);
  useUntilFound(contentRef, collapsible.open);

  return (
    <div
      {...withoutComposed(rest, ["ref"])}
      hidden={!collapsible.open}
      id={collapsible.contentId}
      ref={composeRefs(rest.ref, (element) => {
        contentRef.current = element;
      })}
    >
      {children}
    </div>
  );
}
