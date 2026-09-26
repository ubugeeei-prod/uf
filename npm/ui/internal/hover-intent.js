// @flow
//
// What WCAG requires of anything that appears because a pointer or focus
// arrived, which is a tooltip and a hover card and nothing else in this
// package.
//
// SC 1.4.13, *Content on Hover or Focus*, is three clauses, and a hand-written
// tooltip fails all three. They are not a matter of taste and they are not
// separable, so they live in one module:
//
//   * **Dismissible** — `Escape` removes it without moving the pointer. The
//     content never holds focus, so the key never reaches it: the listener has
//     to be on the document, which is `useDismissOnEscape` below.
//   * **Hoverable** — the pointer can travel from the trigger onto the content
//     without it vanishing on the way. That is why leaving schedules a close
//     rather than performing one, and why arriving anywhere cancels it. A
//     component that closes on `pointerleave` snatches the content away from a
//     reader who was moving towards it — including every reader who magnifies
//     the screen, for whom the trip is long.
//   * **Persistent** — it stays until it is dismissed or the pointer and focus
//     have both left. A timer that closes it on its own is out.
//
// The opening delay is not one of the three clauses; it is what makes the
// component bearable. A pointer crossing a toolbar enters six triggers on its
// way somewhere else, and a tooltip that opened on each would be six
// interruptions. **A delay belongs to the pointer and not to focus**: a reader
// who tabbed to a control has already said what they want, and making them wait
// for it is a delay with nothing to prevent.
//
// # The clock is a ref, and `useTimeout` is deliberately not used
//
// `@uniflowed/hooks/timing` has the hook this looks like it wants, and its
// contract is not this one: `useTimeout(body, millis)` sets its timer in an
// effect keyed on `millis`, so asking again for the *same* delay does not
// restart it. Every interesting sequence here asks twice — enter, leave,
// enter — and with an open delay equal to the close delay the second request
// would inherit the first request's deadline and fire early. What is wanted is
// "restart the clock", which is a command rather than a state, so it is written
// as one.
//
// # Why this is `internal/` and not a subpath
//
// It is not a `useHoverIntent` for anybody to build a tooltip with; it is the
// half of `tooltip.js` and `hover-card.js` that has to be the same in both. A
// consumer given a copy could build the component that closes on
// `pointerleave`, which is the failure this exists to prevent.

import { useEffect, useMemo, useRef } from "@uniflowed/react";
import { useStableCallback } from "@uniflowed/hooks/lifecycle";

import { FOCUS_STOPS } from "./focus.js";

/** How long a pointer must rest on a trigger before its tooltip opens. */
export const DEFAULT_OPEN_DELAY: number = 700;

/**
 * How long the content stays after the pointer leaves.
 *
 * This is the hoverable clause's whole implementation: the gap between a
 * trigger and its overlay takes a moment to cross, and a reader who is crossing
 * it has not left.
 */
export const DEFAULT_CLOSE_DELAY: number = 300;

/**
 * How long after one tooltip closes the next one opens with no delay.
 *
 * A reader who has waited out the delay once has established that they are
 * reading tooltips; the second icon in a toolbar should answer immediately.
 */
export const DEFAULT_SKIP_DELAY: number = 300;

/** Opening and closing, on a clock that can be restarted or called off. */
export type HoverIntent = {|
  /** Open after `millis`, or in this tick when that is nought. */
  readonly openAfter: (millis: number) => void,
  /** Close after `millis`, or in this tick when that is nought. */
  readonly closeAfter: (millis: number) => void,
  /** Forget whatever was scheduled. Arriving anywhere calls this first. */
  readonly cancel: () => void,
|};

/**
 * A group of tooltips that share one clock.
 *
 * `Tooltip.Provider` holds it; `Tooltip.Root` asks it what delay to use. A
 * tooltip outside a provider never sees one and uses its own delay, which is
 * the behaviour a tooltip on its own has always had.
 */
export type DelayGroup = {|
  /** `own`, or nought while the group is inside its skip window. */
  readonly delayFor: (own: number) => number,
  /** Told that a tooltip in the group has opened. */
  readonly opened: () => void,
  /** Told that one has closed, which is what starts the window. */
  readonly closed: () => void,
|};

/**
 * A pending open or close, restartable, and cancelled when the component goes.
 *
 * `setOpen` is called with the answer rather than with a toggle, so a schedule
 * that is overtaken by a second one does not leave the component holding the
 * first one's opinion.
 */
export hook useHoverIntent(setOpen: (open: boolean) => void): HoverIntent {
  const timer = useRef<TimeoutID | null>(null);
  const change = useStableCallback(setOpen);

  const cancel = useStableCallback(() => {
    if (timer.current != null) {
      clearTimeout(timer.current);
      timer.current = null;
    }
  });

  const schedule = useStableCallback((open: boolean, millis: number) => {
    cancel();
    if (millis <= 0) {
      // In this tick, not in a zero-millisecond timeout. A tooltip that opens
      // on focus, and the second tooltip in a toolbar, both have to be open by
      // the time the event handler returns — a test that has to advance a clock
      // to see them is describing a wait the reader would also have had.
      change(open);
      return;
    }
    timer.current = setTimeout(() => {
      timer.current = null;
      change(open);
    }, millis);
  });

  // The component can be taken away while a tooltip is waiting to open, and a
  // timer that outlives it sets state on something that is gone.
  useEffect(() => cancel, [cancel]);

  return useMemo(
    () => ({
      cancel,
      closeAfter: (millis: number) => schedule(false, millis),
      openAfter: (millis: number) => schedule(true, millis),
    }),
    [cancel, schedule],
  );
}

/**
 * The shared clock behind `Tooltip.Provider`.
 *
 * Refs rather than state, and that is the whole design: nothing here is
 * rendered. A group that held "are we skipping" in state would re-render every
 * tooltip in a toolbar twice for each one the pointer passed over, to change
 * nothing anybody can see.
 *
 * The window is open while a tooltip in the group is showing — moving along a
 * toolbar with one already open is the case that must not stutter — and for
 * `skipDelay` after the last one closes.
 */
export hook useDelayGroup(skipDelay: number): DelayGroup {
  const skipping = useRef(false);
  const timer = useRef<TimeoutID | null>(null);

  const stop = useStableCallback(() => {
    if (timer.current != null) {
      clearTimeout(timer.current);
      timer.current = null;
    }
  });

  useEffect(() => stop, [stop]);

  return useMemo(
    () => ({
      closed: () => {
        stop();
        if (skipDelay <= 0) {
          skipping.current = false;
          return;
        }
        skipping.current = true;
        timer.current = setTimeout(() => {
          timer.current = null;
          skipping.current = false;
        }, skipDelay);
      },
      delayFor: (own: number) => (skipping.current ? 0 : own),
      opened: () => {
        // While one is open the group is answering instantly, and the window
        // does not start counting down until it closes.
        stop();
        skipping.current = true;
      },
    }),
    [skipDelay, stop],
  );
}

/**
 * Close on `Escape`, from wherever focus happens to be.
 *
 * On the document, because the content shown on hover holds no focus and a
 * handler on it would never be reached — which is exactly why the hand-written
 * version fails the dismissible clause rather than implementing it wrongly.
 *
 * Capture, and `stopPropagation`, so one `Escape` is one dismissal: a tooltip
 * inside a dialog answers the key itself rather than leaving the reader with a
 * dialog that closed because a tooltip was showing.
 */
export hook useDismissOnEscape(
  open: boolean,
  ref: { current: HTMLElement | null },
  onDismiss: () => void,
): void {
  const dismiss = useStableCallback(onDismiss);

  useEffect(() => {
    const document = ref.current?.ownerDocument;
    if (!open || document == null) {
      return;
    }
    const onKeyDown = (event: $FlowFixMe) => {
      if (event.key !== "Escape") {
        return;
      }
      event.stopPropagation();
      dismiss();
    };
    document.addEventListener("keydown", onKeyDown, true);
    return () => document.removeEventListener("keydown", onKeyDown, true);
  }, [open, ref, dismiss]);
}

/**
 * Refuse a trigger the keyboard cannot reach.
 *
 * A tooltip on a `<span>` is a tooltip only a mouse can find, and it looks
 * perfect: the markup is right, the styles are right, and a reader who never
 * touches a mouse is told nothing at all. It is the failure this package exists
 * to make loud, so it is an error rather than a warning — the same answer
 * `useDialog` gives to a part outside its root.
 *
 * Checked in an effect because it is a question about an element, and the
 * element does not exist until one has been committed.
 */
export hook useFocusableTrigger(ref: { current: HTMLElement | null }, part: string): void {
  useEffect(() => {
    const element = ref.current;
    if (element == null || element.matches(FOCUS_STOPS)) {
      return;
    }
    throw new Error(
      `${part} must be something the keyboard can reach: it was rendered onto ` +
        `<${element.tagName.toLowerCase()}>, which is not focusable, so the ` +
        `content would only ever appear for a pointer. Render a button or a ` +
        `link, or give the element a tabindex of 0. A disabled control is not ` +
        `focusable either: use aria-disabled and keep it in the tab order.`,
    );
  }, [ref, part]);
}
