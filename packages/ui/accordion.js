// @flow
//
// An accordion: a stack of disclosures that know about each other.
//
// The disclosure itself is `collapsible.js` and the argument for the closed
// panel staying in the document is `internal/disclosure.js`. What this adds is
// everything that follows from the sections being a *stack*, and every one of
// them is a thing a hand-written accordion leaves out.
//
// # The heading level is the caller's
//
// The trigger is a button inside a heading, and which heading depends entirely
// on where the accordion sits: inside a section titled by an `<h2>` it must be
// an `<h3>`, and at the top of a page it might be an `<h2>`. A component that
// hard-codes one produces a document outline nobody can navigate — and skimming
// by heading is how a screen-reader user reads a long page, so the outline is
// not decoration. `Dialog.Title` hard-codes `<h2>`, which is defensible for a
// dialog, because a dialog is a document of its own with one title in it. It is
// not defensible here.
//
// # The panel is a named region
//
// `role="region"` with `aria-labelledby` naming the trigger that opens it, so
// the panel turns up in a screen reader's list of landmarks under the name the
// reader just pressed. An unnamed region is a landmark that says "region" and
// nothing else, which is why the naming is wired here rather than left as
// advice — and why the trigger's id and the panel's are made in one place.
//
// # `single`, `multiple`, and the section that cannot be closed
//
// A `single` accordion keeps one section open; `multiple` lets any number be.
// `collapsible` asks whether the open section of a `single` accordion may be
// closed again, leaving nothing open. When it may not, the open section's
// trigger says `aria-disabled="true"` rather than `disabled`, so a reader is
// told "pressing this does nothing" instead of finding that a header they can
// see has vanished from the accessibility tree — the same distinction
// `menu.js` and `tabs.js` make, for the same reason.
//
// It has one consequence worth stating, because it is the opposite of every
// other set in this package: the arrow keys **land on** a disabled header here
// rather than stepping over it. `moveTo`'s `skipDisabled` is where that lives.
// An accordion's headers are ordinary buttons in the page's tab order — `Tab`
// reaches every one of them — so arrows that skipped one would disagree with
// `Tab` about which headers exist.
//
// # The headers are not a roving tab stop
//
// This is the difference between an accordion and a tab list, and it is easy to
// get backwards. A tab list is *one* control, so it takes one stop in the tab
// order and the arrows move inside it. An accordion is a stack of ordinary
// buttons that happen to be near each other: `Tab` reaches every header,
// because each one is a real control a reader might want to press. The arrow
// keys between headers are a convenience the practices call optional, and they
// are here because a long FAQ is nicer with them — but nothing about them takes
// a header out of the tab order.
//
// # The height a stylesheet animates to
//
// `measure` on the root puts each panel's would-be height on it as
// `--uf-collapsible-height` — one property name across both disclosure
// components, because it is one measurement and a second name would be a second
// rule to keep in step. `collapsible.js`'s header shows the stylesheet, and
// `internal/disclosure.js` holds the measuring pass and the argument for the
// opt-in.
//
// It is on `Accordion.Root` rather than on each `Accordion.Content` because a
// forty-section FAQ is one decision, made once, rather than forty props that
// have to agree.
//
// # How the sections are found
//
// By a `data-*` attribute of this package's own rather than by role, which is
// the exception to what `roving-focus.js`'s other sets do. There is no ARIA
// role for an accordion, nor for one of its headers — the pattern is built out
// of headings and buttons — so there is no role to ask for, and an accordion
// nested inside another accordion's panel still has to keep its own arrow keys
// to itself. That needs a name for the container and a name for the header, and
// this package has to be the one to give them.

"use client";

import * as React from "@uniflowed/react";
import {
  createContext,
  useCallback,
  useContext,
  useId,
  useMemo,
  useRef,
  useState,
} from "@uniflowed/react";

import type { PartEvent, RenderProp, Rest } from "./internal/merge-props.js";
import {
  composeHandlers,
  composeRefs,
  withProps,
  withoutComposed,
} from "./internal/merge-props.js";
import { moveOnKey } from "./internal/roving-focus.js";
import type { RovingSet } from "./internal/roving-focus.js";
import { useMeasuredHeight, usePresence, useUntilFound } from "./internal/disclosure.js";
import { useControlled } from "./internal/controlled-state.js";

/** Whether one section is open at a time, or any number of them. */
export type AccordionType = "single" | "multiple";

/** Nothing open, as one array rather than a new one per render. */
const NOTHING: $ReadOnlyArray<string> = [];

/**
 * The headers the arrow keys run across, and what owns them.
 *
 * `skipDisabled` is false, which is this set's one departure from every other
 * one here; the module header says why.
 */
const HEADERS: RovingSet = {
  item: "[data-accordion-trigger]",
  owner: "[data-accordion]",
  orientation: "vertical",
  wrap: true,
  skipDisabled: false,
};

type AccordionState = {|
  readonly open: $ReadOnlyArray<string>,
  readonly toggle: (value: string) => void,
  /** Whether closing the last open section is allowed; only meaningful for `single`. */
  readonly closable: boolean,
  readonly type: AccordionType,
  /** Whether each panel carries its measured height; see the module header. */
  readonly measure: boolean,
|};

const AccordionContext: React.Context<AccordionState | null> = createContext(null);

type AccordionItemState = {|
  readonly triggerId: string,
  readonly contentId: string,
  readonly open: boolean,
  readonly toggle: () => void,
  /** True when this section is open and the accordion will not let it close. */
  readonly locked: boolean,
  readonly disabled: boolean,
  /** Whether an `Accordion.Content` is rendered, so the trigger names one that exists. */
  readonly present: boolean,
  readonly registerContent: (present: boolean) => void,
|};

const AccordionItemContext: React.Context<AccordionItemState | null> = createContext(null);

hook useAccordion(part: string): AccordionState {
  const state = useContext(AccordionContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside an Accordion.Root`);
  }
  return state;
}

hook useAccordionItem(part: string): AccordionItemState {
  const state = useContext(AccordionItemContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside an Accordion.Item`);
  }
  return state;
}

/**
 * The stack, and the one place the arrow keys are handled.
 *
 * `value` is the list of open sections in both modes, for the reason
 * `toggle-group.js` gives at greater length: in `single` mode it holds at most
 * one, and keeping that invariant is the component's job rather than the
 * caller's to remember.
 */
export component AccordionRoot(
  children: renders* AccordionItem,
  type?: AccordionType = "single",
  collapsible?: boolean = true,
  defaultValue?: $ReadOnlyArray<string> = NOTHING,
  value?: $ReadOnlyArray<string>,
  onValueChange?: (value: $ReadOnlyArray<string>) => void,
  measure?: boolean = false,
  render?: RenderProp,
  ...rest: Rest
) {
  const [open, setOpen] = useControlled<$ReadOnlyArray<string>>(value, defaultValue, onValueChange);

  const toggle = useCallback(
    (item: string) => {
      const isOpen = open.includes(item);
      if (type === "multiple") {
        setOpen(isOpen ? open.filter((each) => each !== item) : [...open, item]);
        return;
      }
      // Opening one closes the other, which is the whole of `single`. Closing
      // the open one is a separate question, and `collapsible` answers it.
      if (isOpen && !collapsible) {
        return;
      }
      setOpen(isOpen ? NOTHING : [item]);
    },
    [open, setOpen, type, collapsible],
  );

  const state = useMemo(
    // `collapsible` only ever narrows a `single` accordion: in `multiple` mode
    // every section closes on its own, and there is no last one to protect.
    () => ({ open, toggle, closable: type === "multiple" || collapsible, type, measure }),
    [open, toggle, type, collapsible, measure],
  );
  const props = withProps(withoutComposed(rest, ["onKeyDown"]), {
    children,
    // The name the arrow keys use to tell this accordion's headers from those
    // of an accordion nested inside one of its panels.
    "data-accordion": "",
    onKeyDown: composeHandlers(rest.onKeyDown, (event: PartEvent) => {
      const stack: $FlowFixMe = event.currentTarget;
      moveOnKey(event, stack, HEADERS);
    }),
  });

  return (
    <AccordionContext.Provider value={state}>
      {render == null ? <div {...props} /> : render(props)}
    </AccordionContext.Provider>
  );
}

/**
 * One section: a header and the panel it shows.
 *
 * Renders a `<div>` because a section needs an element to be styled as one, and
 * because the heading and the panel have to be siblings rather than nested —
 * a panel inside its own heading would be part of the heading's accessible
 * name.
 */
export component AccordionItem(
  value: string,
  children: renders* (AccordionHeader | AccordionContent),
  disabled?: boolean = false,
  render?: RenderProp,
  ...rest: Rest
) {
  const accordion = useAccordion("Accordion.Item");
  const base = useId();
  const [present, setPresent] = useState(false);
  const open = accordion.open.includes(value);
  const toggle = accordion.toggle;

  const state = useMemo(
    () => ({
      triggerId: `${base}-trigger`,
      contentId: `${base}-content`,
      open,
      toggle: () => toggle(value),
      locked: open && !accordion.closable,
      disabled,
      present,
      registerContent: setPresent,
    }),
    [base, open, toggle, value, accordion.closable, disabled, present],
  );

  const props = withProps(rest, { children });

  return (
    <AccordionItemContext.Provider value={state}>
      {render == null ? <div {...props} /> : render(props)}
    </AccordionItemContext.Provider>
  );
}

/**
 * The heading the trigger lives in.
 *
 * `level` is the caller's and has no sensible default beyond a guess, so the
 * guess is stated: `3`, which is right for an accordion inside a section that
 * has a title of its own. HTML has six levels and `<h7>` is not an element, so
 * a level outside that range is clamped rather than rendered — a tag nobody
 * recognises is announced as nothing at all, which loses the heading entirely.
 */
export component AccordionHeader(
  children: renders AccordionTrigger,
  level?: number = 3,
  ...rest: Rest
) {
  const clamped = Math.min(6, Math.max(1, Math.trunc(level)));
  const Heading = `h${String(clamped)}`;

  return <Heading {...rest}>{children}</Heading>;
}

/**
 * The button that opens and closes the section.
 *
 * It carries no `tabIndex` of its own on purpose: every header stays in the
 * page's tab order, which is what makes this an accordion and not a tab list.
 */
export component AccordionTrigger(children: React.Node, render?: RenderProp, ...rest: Rest) {
  const item = useAccordionItem("Accordion.Trigger");
  // Locked and disabled are two different sentences a reader hears the same
  // way, and both are `aria-disabled` rather than `disabled` so the header
  // stays where they can find it: "this section will not close" and "this
  // section is unavailable".
  const inert = item.locked || item.disabled;

  const props = withProps(withoutComposed(rest, ["onClick"]), {
    "aria-controls": item.present ? item.contentId : undefined,
    "aria-disabled": inert ? "true" : undefined,
    "aria-expanded": item.open ? "true" : "false",
    children,
    // What the arrow keys look for. Not a role, because the accordion pattern
    // has none to look for; see the module header.
    "data-accordion-trigger": "",
    id: item.triggerId,
    onClick: composeHandlers(rest.onClick, () => {
      if (!inert) {
        item.toggle();
      }
    }),
  });

  if (render != null) {
    return render(props);
  }
  return <button {...props} type="button" />;
}

/**
 * The panel, which is a named region and stays in the document while closed.
 *
 * `internal/disclosure.js` explains what "stays in the document" is worth and
 * what `hidden` is upgraded to for it.
 */
export component AccordionContent(children: React.Node, render?: RenderProp, ...rest: Rest) {
  const accordion = useAccordion("Accordion.Content");
  const item = useAccordionItem("Accordion.Content");
  const contentRef = useRef<HTMLElement | null>(null);
  usePresence(item.registerContent);
  useUntilFound(contentRef, item.open);
  useMeasuredHeight(contentRef, accordion.measure);

  const props = withProps(withoutComposed(rest, ["ref"]), {
    // The name a reader hears for this landmark is the header they pressed.
    "aria-labelledby": item.triggerId,
    children,
    hidden: !item.open,
    id: item.contentId,
    ref: composeRefs(rest.ref, (element: HTMLElement | null) => {
      contentRef.current = element;
    }),
    role: "region",
  });

  if (render != null) {
    return render(props);
  }
  return <div {...props} />;
}
