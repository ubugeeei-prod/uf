// @flow
//
// A tooltip: the component most likely to make a page *worse* than the plain
// HTML it replaced.
//
// The failure is specified rather than a matter of taste. WCAG 2.1 SC 1.4.13,
// *Content on Hover or Focus*, requires content shown that way to be
// dismissible with `Escape`, hoverable — the pointer can travel onto it —  and
// persistent until one of those things ends it. A `title` attribute fails all
// three: it is on the browser's clock, it cannot be hovered, and `Escape` does
// nothing. A `div` that appears on `:hover` fails the first two. Those are the
// two things a project without this component writes.
//
// `internal/hover-intent.js` holds all three clauses, because they are one
// mechanism and separating them is how a component ends up with two of them.
//
// # What a tooltip is not
//
// It is **not focusable**, and nothing in it may be. A tooltip that takes focus
// puts a stop in the tab order the reader did not ask for and cannot leave in
// the direction they expect; the APG says so, and it is why anything with a
// link or a button in it is a `HoverCard` and not this. It carries
// `role="tooltip"` and describes its trigger with `aria-describedby` — and only
// while it is in the document, because a reference to an element that is not
// there makes a screen reader announce nothing at all.
//
// It has **no `aria-haspopup`**. That attribute promises an interactive popup,
// and a reader who is told a button has one and then finds nothing to interact
// with has been sent somewhere that does not exist.
//
// # The delay, and the one that must not be there
//
// A pointer resting on a trigger opens the tooltip after a delay; a pointer
// crossing a toolbar on its way elsewhere passes six triggers and must open
// none of them. **Focus opens it with no delay at all** — a reader who tabbed
// to a control has already said what they want, and a wait exists to filter out
// intent nobody expressed.
//
// `Tooltip.Provider` shares one clock between a group of tooltips, so the
// second icon in a toolbar answers immediately once the reader has waited out
// the first. A tooltip outside a provider is a complete tooltip and keeps its
// own delay.
//
// # Touch, which is a decision rather than an accident
//
// **This does not open on touch.** There is no hover on a touch screen, so the
// only gesture left is the tap — and a tooltip that opens on tap either
// swallows the tap the control needed or shows content the next tap dismisses
// before it has been read. Both are worse than nothing.
//
// What follows from that is a rule for the caller rather than for this module:
// a tooltip may not be the only place something is said. If the trigger is an
// icon button, the tooltip's text is what a reader on a phone needs *as the
// button's name* — so give the button an `aria-label` and let the tooltip
// repeat it. `pointerenter` is ignored for a touch pointer here, which is what
// stops the tap being eaten.
//
// # Why the pointer is watched natively
//
// `pointerenter` and `pointerleave` are the platform's way of saying the
// pointer arrived at *this* element; React's `onPointerEnter` is its own
// reconstruction of them from `pointerover` and `pointerout`. The reconstruction
// is faithful and it is not the same event — and the property this component
// has to read to know a tap from a hover, `pointerType`, belongs to the pointer
// event the platform sent. So the listeners go on the element, through
// `@uniflowed/hooks`' `useEventListener`, which is also what stops a caller's
// `render` function from dropping a handler by forgetting to spread it.

"use client";

import * as React from "@uniflowed/react";
import { createContext, useContext, useEffect, useId, useMemo, useRef } from "@uniflowed/react";
import { useEventListener } from "@uniflowed/hooks/dom";
import { useStableCallback } from "@uniflowed/hooks/lifecycle";

import type { Align, Side } from "./internal/anchor.js";
import type { DelayGroup, HoverIntent } from "./internal/hover-intent.js";
import type { Rest } from "./internal/merge-props.js";
import { composeRefs, withProps, withoutComposed } from "./internal/merge-props.js";
import {
  DEFAULT_CLOSE_DELAY,
  DEFAULT_OPEN_DELAY,
  DEFAULT_SKIP_DELAY,
  useDelayGroup,
  useDismissOnEscape,
  useFocusableTrigger,
  useHoverIntent,
} from "./internal/hover-intent.js";
import { useAnchor } from "./internal/anchor.js";
import { useControlled } from "./internal/controlled-state.js";

export type { Align, Side } from "./internal/anchor.js";

/** What a `Tooltip.Provider` shares with the tooltips inside it. */
type TooltipScope = {|
  /** The default `openDelay` for every tooltip in the group. */
  readonly delayDuration: number,
  readonly group: DelayGroup,
|};

const TooltipScopeContext: React.Context<TooltipScope | null> = createContext(null);

type TooltipState = {|
  readonly base: string,
  readonly open: boolean,
  readonly setOpen: (open: boolean) => void,
  readonly triggerRef: { current: HTMLElement | null },
  readonly intent: HoverIntent,
  /**
   * How long a pointer must rest before this tooltip opens, asked at the moment
   * it arrives rather than read from a render.
   *
   * A function because the answer is the group's and changes as the reader
   * moves: the same tooltip waits the full delay when it is the first one
   * touched and opens at once when it is the second.
   */
  readonly openDelay: () => number,
  readonly closeDelay: number,
  /**
   * Whether `Escape` has dismissed it while the reader is still here.
   *
   * A ref rather than state because nothing renders it, and because what it
   * guards happens inside an event handler: without it, a dismissal is undone
   * by the very next thing the pointer or focus does — which for a hover card
   * is the focus it hands back to its own trigger, and is a component that
   * cannot be closed. It is cleared when the reader leaves the trigger, so
   * coming back opens it again.
   */
  readonly dismissed: { current: boolean },
|};

const TooltipContext: React.Context<TooltipState | null> = createContext(null);

hook useTooltip(part: string): TooltipState {
  const state = useContext(TooltipContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside a Tooltip.Root`);
  }
  return state;
}

/**
 * One clock for a group of tooltips.
 *
 * `delayDuration` is the delay each tooltip inside uses unless it sets its own,
 * so a toolbar says it once. `skipDelayDuration` is the window after one closes
 * during which the next opens immediately — the interaction that makes a row of
 * icon buttons readable rather than tedious.
 *
 * Renders no element: it is a context and a clock, and a `<div>` around a
 * toolbar is the caller's business.
 */
export component TooltipProvider(
  children: React.Node,
  delayDuration?: number = DEFAULT_OPEN_DELAY,
  skipDelayDuration?: number = DEFAULT_SKIP_DELAY,
) {
  const group = useDelayGroup(skipDelayDuration);
  const scope = useMemo(() => ({ delayDuration, group }), [delayDuration, group]);

  return <TooltipScopeContext.Provider value={scope}>{children}</TooltipScopeContext.Provider>;
}

/**
 * One tooltip and its trigger.
 *
 * `openDelay` falls back to the enclosing `Tooltip.Provider`'s
 * `delayDuration`, and to 700ms when there is no provider — a tooltip on its
 * own is a complete tooltip and needs nothing around it.
 */
export component TooltipRoot(
  children: React.Node,
  closeDelay?: number = DEFAULT_CLOSE_DELAY,
  defaultOpen?: boolean = false,
  onOpenChange?: (open: boolean) => void,
  open?: boolean,
  openDelay?: number,
) {
  const base = useId();
  const scope = useContext(TooltipScopeContext);
  const [isOpen, setOpen] = useControlled(open, defaultOpen, onOpenChange);
  const triggerRef = useRef<HTMLElement | null>(null);
  const dismissed = useRef(false);
  const intent = useHoverIntent(setOpen);
  const own = openDelay ?? scope?.delayDuration ?? DEFAULT_OPEN_DELAY;
  const group = scope?.group;

  // What this tooltip last told the group, so that mounting closed says
  // nothing — a page of six tooltips would otherwise open six skip windows
  // before the reader had touched anything.
  const told = useRef(false);
  useEffect(() => {
    if (group == null || told.current === isOpen) {
      return;
    }
    told.current = isOpen;
    if (isOpen) {
      group.opened();
    } else {
      group.closed();
    }
  }, [group, isOpen]);

  // A tooltip taken away while it was showing has to say so, or the group is
  // left believing one is open and answers every later hover instantly.
  useEffect(
    () => () => {
      if (told.current) {
        told.current = false;
        group?.closed();
      }
    },
    [group],
  );

  const state = useMemo(
    () => ({
      base,
      closeDelay,
      dismissed,
      intent,
      open: isOpen,
      openDelay: () => group?.delayFor(own) ?? own,
      setOpen,
      triggerRef,
    }),
    [base, closeDelay, group, intent, isOpen, own, setOpen],
  );

  return <TooltipContext.Provider value={state}>{children}</TooltipContext.Provider>;
}

/**
 * What the tooltip describes.
 *
 * A `<button>` by default, and whatever the caller renders when they pass
 * `render` — a link, a menu item, an icon button of their own. Either way it
 * has to be something the keyboard can reach, and `useFocusableTrigger` refuses
 * anything else: a tooltip on a `<span>` is a tooltip only a mouse can find,
 * and it looks perfect in the markup.
 *
 * Every handler is attached to the element rather than passed as a prop, so a
 * `render` function that forgets to spread something still gets a working
 * tooltip; see the module header.
 */
export component TooltipTrigger(
  children?: React.Node,
  render?: (props: Rest) => React.Node,
  ...rest: Rest
) {
  const tooltip = useTooltip("Tooltip.Trigger");
  const { closeDelay, dismissed, intent, openDelay, setOpen, triggerRef } = tooltip;
  useFocusableTrigger(triggerRef, "Tooltip.Trigger");

  // Whether the pointer put focus here. A click focuses the button, and
  // reopening the tooltip the click just dismissed would put it back over the
  // thing the reader pressed — so a focus that arrived with a press opens
  // nothing, and the next one, which is the keyboard's, does.
  const pressed = useRef(false);

  useEventListener(triggerRef, "pointerenter", (event: $FlowFixMe) => {
    // A tap is not a hover. See the module header: opening here is what eats
    // the tap the control was there to receive.
    if (event.pointerType === "touch" || dismissed.current) {
      return;
    }
    intent.openAfter(openDelay());
  });
  useEventListener(triggerRef, "pointerleave", () => {
    // Leaving is what makes a dismissal stop applying: coming back is a fresh
    // gesture and deserves a fresh answer.
    dismissed.current = false;
    intent.closeAfter(closeDelay);
  });
  useEventListener(triggerRef, "pointerdown", () => {
    pressed.current = true;
    intent.cancel();
    setOpen(false);
  });
  useEventListener(triggerRef, "focusin", () => {
    if (pressed.current) {
      pressed.current = false;
      return;
    }
    if (dismissed.current) {
      return;
    }
    // No delay: the reader has already said what they want by arriving here.
    intent.openAfter(0);
  });
  useEventListener(triggerRef, "focusout", () => {
    pressed.current = false;
    dismissed.current = false;
    intent.closeAfter(0);
  });

  // Annotated because this one is not written inside a `ref={...}`, and there
  // is nothing else here for Flow to infer the element's type from.
  const attach = composeRefs(rest.ref, (element: HTMLElement | null) => {
    triggerRef.current = element;
  });
  // Only while it is there. `aria-describedby` pointing at an element that has
  // been removed is the dangling reference this package keeps coming back to.
  const ours = {
    "aria-describedby": tooltip.open ? `${tooltip.base}-body` : undefined,
    ref: attach,
  };

  if (render != null) {
    return render(withProps(withoutComposed(rest, ["ref"]), ours));
  }

  return (
    <button
      {...withoutComposed(rest, ["ref"])}
      aria-describedby={ours["aria-describedby"]}
      ref={attach}
      type="button"
    >
      {children}
    </button>
  );
}

/**
 * The tooltip itself.
 *
 * Hoverable, which is the clause every hand-written tooltip fails: the pointer
 * arriving here calls off the close the trigger scheduled when the pointer
 * left it, so the trip across the gap does not take the content away. It never
 * takes focus, holds no tab stop, and answers `Escape` from wherever focus
 * happens to be.
 */
export component TooltipBody(
  children: React.Node,
  align?: Align = "center",
  alignOffset?: number = 0,
  avoidCollisions?: boolean = true,
  collisionPadding?: number = 0,
  side?: Side = "top",
  sideOffset?: number = 0,
  ...rest: Rest
) {
  const tooltip = useTooltip("Tooltip.Body");
  const { closeDelay, intent, open, triggerRef } = tooltip;
  const bodyRef = useRef<HTMLElement | null>(null);
  const close = useStableCallback(() => {
    // `Escape` dismisses it *and* keeps it dismissed while the reader is still
    // on the trigger. Without the flag the pointer that is still resting there
    // — or, for a hover card, the focus it hands back — reopens it at once,
    // and the key does nothing a reader can see.
    tooltip.dismissed.current = true;
    intent.cancel();
    tooltip.setOpen(false);
  });

  const anchored = useAnchor({
    align,
    alignOffset,
    anchorRef: triggerRef,
    avoidCollisions,
    collisionPadding,
    open,
    overlayRef: bodyRef,
    side,
    sideOffset,
  });

  useDismissOnEscape(open, bodyRef, close);

  // Keyed on `open` rather than written with `useEventListener`, and the
  // difference is load-bearing: that hook reads its target once, when it
  // attaches, and this element does not exist until the tooltip opens. It is
  // the same trap `select.js` documents about its outside-press listener.
  useEffect(() => {
    const body = bodyRef.current;
    if (!open || body == null) {
      return;
    }
    const stay = () => intent.cancel();
    const go = () => intent.closeAfter(closeDelay);
    body.addEventListener("pointerenter", stay);
    body.addEventListener("pointerleave", go);
    return () => {
      body.removeEventListener("pointerenter", stay);
      body.removeEventListener("pointerleave", go);
    };
  }, [open, intent, closeDelay]);

  if (!open) {
    return null;
  }

  return (
    <div
      {...withoutComposed(rest, ["ref"])}
      data-align={anchored.align}
      data-side={anchored.side}
      data-state="open"
      id={`${tooltip.base}-body`}
      ref={composeRefs(rest.ref, (element) => {
        bodyRef.current = element;
      })}
      role="tooltip"
      // No `tabIndex`. A tooltip the keyboard can land in is a stop the reader
      // did not ask for and cannot leave the way they expect.
    >
      {children}
    </div>
  );
}
