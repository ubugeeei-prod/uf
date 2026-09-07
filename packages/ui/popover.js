// @flow
//
// A popover: a dialog that is not modal, which is the whole of the difference.
//
// `dialog.js` is modal and only modal, on purpose — every line of it is a
// promise that the rest of the page is unavailable. A popover makes the
// opposite promise, and it has to make it in every one of the same places:
//
//   * No `aria-modal`, because the page behind is still there.
//   * Nothing is made `inert` and nothing is `aria-hidden`, because a reader
//     may still reach it.
//   * The page is not scroll-locked, because a wheel over a popover scrolling
//     the page behind is what a reader expects from something that did not
//     take the page over.
//   * **`Tab` leaves.** This is the load-bearing one. A trap is what makes a
//     modal dialog safe and what makes a popover a hole a reader falls into:
//     they tabbed in, they tab out, and a component that wraps them back to
//     the first control has taken the page away without ever saying so.
//
// So a popover is not a `Dialog` with a flag. A flag would mean every one of
// the behaviours above reading it, and the failure of the one that forgot would
// be a dialog that is not modal while announcing that it is — silent, and
// wrong in the direction that traps people.
//
// What it *does* share with a dialog is the part a reader notices when it is
// missing: focus moves into it when it opens, `Escape` closes it, and focus
// goes back to the trigger — unless the reader dismissed it by pressing or
// tabbing somewhere else, in which case it stays where they put it.
//
// # Where it goes
//
// `internal/anchor.js`, the same as every other overlay here: `side`, `align`,
// `sideOffset` and `alignOffset` place it against the trigger, it flips and
// slides to stay on the screen, and it reports where it ended up as `data-side`
// and `data-align` so a stylesheet can point an arrow without measuring
// anything itself.
//
// # Its name
//
// `role="dialog"` needs an accessible name, and a popover has no `Title` part
// to take one from — shadcn's does not either, and adding one would make the
// common case (a form, a colour picker, a date picker) carry a heading nobody
// asked for. So the body is named after the button that opened it, which is
// true and is what a reader would say the popover is, and a caller who passes
// `aria-label` or `aria-labelledby` of their own keeps it.

"use client";

import * as React from "@uniflowed/react";
import {
  createContext,
  useContext,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "@uniflowed/react";
import { useStableCallback } from "@uniflowed/hooks/lifecycle";

import type { Align, LogicalSide } from "./internal/anchor.js";
import type { Rest } from "./internal/merge-props.js";
import { composeHandlers, composeRefs, withoutComposed } from "./internal/merge-props.js";
import { focusable } from "./internal/focus.js";
import { useAnchor } from "./internal/anchor.js";
import { useControlled } from "./internal/controlled-state.js";
import { usePresence } from "./internal/disclosure.js";

export type { Align, LogicalSide, Side } from "./internal/anchor.js";

type PopoverState = {|
  readonly base: string,
  readonly open: boolean,
  readonly setOpen: (open: boolean) => void,
  /** What opened it, where it is anchored, and where focus goes back to. */
  readonly triggerRef: { current: HTMLElement | null },
  /**
   * Whether a trigger is rendered, so the body only names one that exists.
   *
   * A popover opened by `defaultOpen` in a page with no trigger is a real
   * arrangement — a first-run hint pointing at something — and an
   * `aria-labelledby` naming the id that trigger *would* have had makes a
   * screen reader announce nothing at all.
   */
  readonly triggered: boolean,
  readonly registerTrigger: (present: boolean) => void,
|};

const PopoverContext: React.Context<PopoverState | null> = createContext(null);

/**
 * The popover a part belongs to.
 *
 * Raising rather than returning null, for the reason `useDialog` gives: a
 * `Popover.Body` outside a root would render a `role="dialog"` that nothing
 * opens, closes or names, and it would look correct.
 */
hook usePopover(part: string): PopoverState {
  const state = useContext(PopoverContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside a Popover.Root`);
  }
  return state;
}

/**
 * The popover, open or closed. Uncontrolled unless `open` is given.
 *
 * Renders no element of its own: the trigger and the body are siblings in
 * whatever layout the caller wrote, and a wrapper would put a `<div>` between
 * them for the caller to style around.
 */
export component PopoverRoot(
  children: React.Node,
  defaultOpen?: boolean = false,
  open?: boolean,
  onOpenChange?: (open: boolean) => void,
) {
  const base = useId();
  const [isOpen, setOpen] = useControlled(open, defaultOpen, onOpenChange);
  const triggerRef = useRef<HTMLElement | null>(null);
  const [triggered, setTriggered] = useState(false);

  const state = useMemo(
    () => ({
      base,
      open: isOpen,
      registerTrigger: setTriggered,
      setOpen,
      triggerRef,
      triggered,
    }),
    [base, isOpen, setOpen, triggered],
  );

  return <PopoverContext.Provider value={state}>{children}</PopoverContext.Provider>;
}

/**
 * The button that opens the popover, and what it is anchored to.
 *
 * A toggle rather than an opener: pressing the button of an open popover closes
 * it, which is what every disclosure does and what a reader who pressed it by
 * accident expects. The outside-press handler in `Popover.Body` knows the
 * trigger is not "outside" for exactly this reason — closing there and letting
 * this click reopen made the press a no-op that flickered.
 */
export component PopoverTrigger(children: React.Node, ...rest: Rest) {
  const popover = usePopover("Popover.Trigger");
  const passed = withoutComposed(rest, ["onClick", "ref"]);
  usePresence(popover.registerTrigger);

  return (
    <button
      {...passed}
      // Only while it is open: an `aria-controls` naming an element that is not
      // in the document tells a reader there is somewhere to go and then has
      // nowhere to send them.
      aria-controls={popover.open ? `${popover.base}-body` : undefined}
      aria-expanded={popover.open ? "true" : "false"}
      aria-haspopup="dialog"
      id={`${popover.base}-trigger`}
      onClick={composeHandlers(rest.onClick, () => popover.setOpen(!popover.open))}
      ref={composeRefs(rest.ref, (element) => {
        popover.triggerRef.current = element;
      })}
      type="button"
    >
      {children}
    </button>
  );
}

/**
 * The popover itself: positioned, focused, dismissible, and not a trap.
 *
 * The three ways out are the three a reader tries. `Escape` closes it and gives
 * focus back to the trigger, because the reader is still where they were. A
 * press outside closes it and leaves focus alone, because they have already
 * moved on. `Tab` past the last control inside closes it for the same reason
 * and leaves focus where the browser put it — which is the behaviour that
 * separates this from a dialog, and the one a copy of `dialog.js` with the
 * `aria-modal` deleted would get wrong.
 */
export component PopoverBody(
  children: React.Node,
  align?: Align = "center",
  alignOffset?: number = 0,
  avoidCollisions?: boolean = true,
  collisionPadding?: number = 0,
  /**
   * Where focus lands when it opens, when the first focus stop is the wrong
   * answer. The same prop `Dialog.Body` takes, deliberately spelled the same
   * way: `DatePicker.Calendar` fills it with the day that holds the grid's tab
   * stop, because a reader who opened a date picker is looking for the date and
   * not for the button that steps back a month.
   */
  initialFocus?: { current: HTMLElement | null },
  side?: LogicalSide = "bottom",
  sideOffset?: number = 0,
  ...rest: Rest
) {
  const popover = usePopover("Popover.Body");
  const bodyRef = useRef<HTMLElement | null>(null);
  // Stable, so the effect below depends on `open` and on nothing else. Keyed on
  // `setOpen` it re-ran whenever the caller passed a fresh `onOpenChange`
  // closure — which is every render — and re-running it took focus back from
  // wherever the reader had moved it inside the popover.
  const close = useStableCallback(() => popover.setOpen(false));
  // Set when the reader left rather than closed: a press outside, or a Tab that
  // carried them out. Focus is theirs from then on, and dragging it back to the
  // trigger would undo the thing they just did.
  const left = useRef(false);
  const triggerRef = popover.triggerRef;

  const anchored = useAnchor({
    align,
    alignOffset,
    anchorRef: triggerRef,
    avoidCollisions,
    collisionPadding,
    open: popover.open,
    overlayRef: bodyRef,
    side,
    sideOffset,
  });

  useEffect(() => {
    const body = bodyRef.current;
    if (!popover.open || body == null) {
      return;
    }
    const document = body.ownerDocument;
    const trigger = triggerRef.current;
    // Whatever had focus, which is the trigger for a popover that was opened
    // and the previously focused element for one that opened itself.
    const opener = trigger ?? (document.activeElement as $FlowFixMe);

    const outside = (target: EventTarget | null): boolean => {
      const node: $FlowFixMe = target;
      // The trigger is outside the body and is not "outside" for this purpose.
      return node != null && !body.contains(node) && !(trigger?.contains(node) ?? false);
    };

    const onOutsidePress = (event: Event) => {
      if (!outside(event.target)) {
        return;
      }
      left.current = true;
      close();
    };
    // Capture, so a press is seen even where something below it stops the
    // event — a menu inside the popover, for instance.
    document.addEventListener("pointerdown", onOutsidePress, true);

    // Tab out is a dismissal, not an escape hatch that leaves a popover open
    // behind the reader: a non-modal overlay whose reader has gone is one they
    // can no longer press Escape at, because Escape is handled where focus is.
    const onFocusMoved = (event: Event) => {
      if (!outside(event.target)) {
        return;
      }
      left.current = true;
      close();
    };
    document.addEventListener("focusin", onFocusMoved, true);

    // Where the caller said, then the first thing worth acting on, then the
    // popover itself when it holds nothing focusable - so focus is inside it
    // whichever of the three answers, and Escape reaches the handler below.
    //
    // The named element has to still be *in* this popover, for the reason
    // `dialog.js` gives at the same line: a ref left from a previous opening
    // would move focus somewhere the reader did not open.
    const named = initialFocus?.current ?? null;
    ((named != null && body.contains(named) ? named : focusable(body)[0]) ?? body).focus();

    return () => {
      document.removeEventListener("pointerdown", onOutsidePress, true);
      document.removeEventListener("focusin", onFocusMoved, true);
      if (left.current) {
        left.current = false;
        return;
      }
      // Only when focus would otherwise be lost to `<body>`, the same
      // condition `menu.js` restores under: a popover closed by a control
      // inside it that moved focus somewhere deliberate must not have that
      // undone.
      const active = document.activeElement;
      if (active == null || active === document.body || body.contains(active)) {
        opener?.focus?.();
      }
    };
  }, [popover.open, triggerRef, close, initialFocus]);

  if (!popover.open) {
    return null;
  }

  const passed = withoutComposed(rest, ["onKeyDown", "ref"]);
  // Named by its trigger unless the caller said otherwise. Setting it anyway
  // would override an `aria-label` they passed — `aria-labelledby` wins — and
  // leave the popover announced as its button rather than as itself.
  const named = rest["aria-label"] != null || rest["aria-labelledby"] != null;

  return (
    <div
      // `passed` first, then this component's semantics; see
      // `internal/merge-props.js` for the three bugs that rule is made of.
      {...passed}
      aria-labelledby={named || !popover.triggered ? undefined : `${popover.base}-trigger`}
      data-align={anchored.align}
      data-side={anchored.side}
      data-state="open"
      id={`${popover.base}-body`}
      onKeyDown={composeHandlers(rest.onKeyDown, (event) => {
        if (event.key !== "Escape") {
          return;
        }
        event.preventDefault();
        // This popover, not the dialog around it. Two overlays nest in the
        // DOM, so without this one Escape closed both.
        event.stopPropagation();
        close();
      })}
      ref={composeRefs(rest.ref, (element) => {
        bodyRef.current = element;
      })}
      // No `aria-modal`. The page behind a popover is still available, and
      // saying otherwise is the one lie a screen reader cannot see through.
      role="dialog"
      // So the popover can hold focus itself when it contains nothing
      // focusable, and so Escape has somewhere to be heard.
      tabIndex={-1}
    >
      {children}
    </div>
  );
}
