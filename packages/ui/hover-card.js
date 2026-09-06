// @flow
//
// A hover card: the preview a name expands into, taken seriously.
//
// It is a tooltip's sibling and it is not a tooltip, and the difference is what
// is inside. A tooltip holds a phrase and must never be focusable; a hover card
// holds an avatar, a paragraph and two links, and every one of those has to be
// reachable — so the pointer has to be able to get there and so does `Tab`.
//
// That makes SC 1.4.13's hoverable clause the whole component rather than a
// detail of it: the gap between a name and the card that describes it takes a
// moment to cross with a mouse, longer with a trackpad, and much longer for a
// reader magnifying the screen. Closing on `pointerleave` snatches it away
// mid-journey. `internal/hover-intent.js` is the delay that stops that, shared
// with `tooltip.js` so the two cannot drift.
//
// # What it is not
//
// **Not a dialog.** It carries no `role="dialog"` and no `aria-modal`: nothing
// about it is modal, focus is not moved into it when it opens, and announcing a
// dialog a reader never asked for is worse than announcing nothing. Its content
// is in the document immediately after its trigger, so the reading order
// carries it and `Tab` reaches it — which is the whole of what a keyboard
// reader needs from it.
//
// **Not described by `aria-describedby`.** A card of links flattened into one
// description string is a sentence nobody can act on: the links stop being
// links. `tooltip.js` describes its trigger because a tooltip is a phrase; this
// does not, because this is not.
//
// # Touch
//
// It does not open on touch, for the reason `tooltip.js` gives at more length:
// there is no hover on a touch screen, and the only gesture left is the tap the
// trigger itself needs. A hover card is therefore an *enrichment* — the link
// under it must go somewhere useful on its own, because a reader on a phone
// will only ever get the link.

"use client";

import * as React from "@uniflowed/react";
import { createContext, useContext, useEffect, useId, useMemo, useRef } from "@uniflowed/react";
import { useEventListener } from "@uniflowed/hooks/dom";
import { useStableCallback } from "@uniflowed/hooks/lifecycle";

import type { Align, Side } from "./internal/anchor.js";
import type { HoverIntent } from "./internal/hover-intent.js";
import type { Rest } from "./internal/merge-props.js";
import { composeRefs, withProps, withoutComposed } from "./internal/merge-props.js";
import {
  DEFAULT_CLOSE_DELAY,
  DEFAULT_OPEN_DELAY,
  useDismissOnEscape,
  useFocusableTrigger,
  useHoverIntent,
} from "./internal/hover-intent.js";
import { useAnchor } from "./internal/anchor.js";
import { useControlled } from "./internal/controlled-state.js";

export type { Align, Side } from "./internal/anchor.js";

type HoverCardState = {|
  readonly base: string,
  readonly open: boolean,
  readonly setOpen: (open: boolean) => void,
  readonly triggerRef: { current: HTMLElement | null },
  readonly intent: HoverIntent,
  readonly openDelay: number,
  readonly closeDelay: number,
  /** Whether `Escape` has dismissed it; see `tooltip.js`, which shares the rule. */
  readonly dismissed: { current: boolean },
|};

const HoverCardContext: React.Context<HoverCardState | null> = createContext(null);

hook useHoverCard(part: string): HoverCardState {
  const state = useContext(HoverCardContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside a HoverCard.Root`);
  }
  return state;
}

/**
 * A hover card and the thing it previews.
 *
 * `closeDelay` is longer than a tooltip's would need to be on purpose: it is
 * the time the reader has to reach the card, and a card holding links is a card
 * they are reaching for.
 */
export component HoverCardRoot(
  children: React.Node,
  closeDelay?: number = DEFAULT_CLOSE_DELAY,
  defaultOpen?: boolean = false,
  onOpenChange?: (open: boolean) => void,
  open?: boolean,
  openDelay?: number = DEFAULT_OPEN_DELAY,
) {
  const base = useId();
  const [isOpen, setOpen] = useControlled(open, defaultOpen, onOpenChange);
  const triggerRef = useRef<HTMLElement | null>(null);
  const dismissed = useRef(false);
  const intent = useHoverIntent(setOpen);

  const state = useMemo(
    () => ({
      base,
      closeDelay,
      dismissed,
      intent,
      open: isOpen,
      openDelay,
      setOpen,
      triggerRef,
    }),
    [base, closeDelay, intent, isOpen, openDelay, setOpen],
  );

  return <HoverCardContext.Provider value={state}>{children}</HoverCardContext.Provider>;
}

/**
 * What the card is about, which is usually a link.
 *
 * `render` is how it becomes one: `<HoverCard.Trigger render={(props) => <a
 * href={profile} {...props}>@ada</a>} />`. Whatever it ends up as has to be
 * reachable by keyboard, and `useFocusableTrigger` refuses anything else —
 * a hover card on a `<span>` is one a keyboard reader can never see.
 */
export component HoverCardTrigger(
  children?: React.Node,
  render?: (props: Rest) => React.Node,
  ...rest: Rest
) {
  const card = useHoverCard("HoverCard.Trigger");
  const { closeDelay, dismissed, intent, openDelay, triggerRef } = card;
  useFocusableTrigger(triggerRef, "HoverCard.Trigger");

  // A press focuses the trigger, and a card opening under the reader's own
  // click would cover what they just went to. See `tooltip.js`.
  const pressed = useRef(false);

  useEventListener(triggerRef, "pointerenter", (event: $FlowFixMe) => {
    if (event.pointerType === "touch" || dismissed.current) {
      return;
    }
    intent.openAfter(openDelay);
  });
  useEventListener(triggerRef, "pointerleave", () => {
    dismissed.current = false;
    intent.closeAfter(closeDelay);
  });
  useEventListener(triggerRef, "pointerdown", () => {
    pressed.current = true;
    intent.cancel();
  });
  useEventListener(triggerRef, "focusin", () => {
    if (pressed.current) {
      pressed.current = false;
      return;
    }
    // The focus a dismissed card hands *back* to this trigger must not reopen
    // it, which is the whole reason the flag exists: without it `Escape` closes
    // the card, focus returns here, and the card comes straight back.
    if (dismissed.current) {
      return;
    }
    // A reader who tabbed here has said what they want; only the pointer is
    // guessed at, so only the pointer waits.
    intent.openAfter(0);
  });
  useEventListener(triggerRef, "focusout", () => {
    pressed.current = false;
    dismissed.current = false;
    // `closeAfter` rather than a close, and this is where the delay earns its
    // keep a second time: `Tab` from the trigger *into* the card is a leave
    // followed immediately by an arrival, and the card's own `focusin` calls
    // this off before it runs. A `closeDelay` of nought would close the card
    // in the instant the reader reached it, which is why the default is not
    // nought and why a caller who sets one should not set that.
    intent.closeAfter(closeDelay);
  });

  // Annotated because this one is not written inside a `ref={...}`, and there
  // is nothing else here for Flow to infer the element's type from.
  const attach = composeRefs(rest.ref, (element: HTMLElement | null) => {
    triggerRef.current = element;
  });

  if (render != null) {
    return render(withProps(withoutComposed(rest, ["ref"]), { ref: attach }));
  }

  return (
    <button {...withoutComposed(rest, ["ref"])} ref={attach} type="button">
      {children}
    </button>
  );
}

/**
 * The card.
 *
 * Stays while the pointer is over it and while focus is inside it, which are
 * the same rule applied to the two ways a reader can be in it. `Escape` closes
 * it from either — and when focus was inside, focus goes back to the trigger,
 * because a card that took its own links away and left focus on `<body>` would
 * send the reader back to the top of the page.
 */
export component HoverCardBody(
  children: React.Node,
  align?: Align = "center",
  alignOffset?: number = 0,
  avoidCollisions?: boolean = true,
  collisionPadding?: number = 0,
  side?: Side = "bottom",
  sideOffset?: number = 0,
  ...rest: Rest
) {
  const card = useHoverCard("HoverCard.Body");
  const { closeDelay, intent, open, triggerRef } = card;
  const bodyRef = useRef<HTMLElement | null>(null);
  // Whether the reader is *in* the card, as opposed to over it. It decides one
  // thing and it cannot be asked afterwards: a card closed while it held focus
  // has to hand focus back, and by the time the effect below is cleaned up the
  // element is gone from the document and `activeElement` has already fallen to
  // `<body>` — so the answer is kept while it is still true.
  const held = useRef(false);
  const close = useStableCallback(() => {
    card.dismissed.current = true;
    intent.cancel();
    card.setOpen(false);
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

  // Keyed on `open`, because the element does not exist until then; see
  // `tooltip.js` for why `useEventListener` cannot be used here.
  useEffect(() => {
    const body = bodyRef.current;
    if (!open || body == null) {
      return;
    }
    const stay = () => intent.cancel();
    const go = () => intent.closeAfter(closeDelay);
    const arrived = () => {
      held.current = true;
      stay();
    };
    const gone = () => {
      held.current = false;
      go();
    };
    body.addEventListener("pointerenter", stay);
    body.addEventListener("pointerleave", go);
    // `focusin` and `focusout` rather than `focus` and `blur`: the pair that
    // bubbles is the one that hears a reader moving between two links *inside*
    // the card, where the leave is immediately followed by an arrival and the
    // scheduled close is called off before it runs.
    body.addEventListener("focusin", arrived);
    body.addEventListener("focusout", gone);

    return () => {
      body.removeEventListener("pointerenter", stay);
      body.removeEventListener("pointerleave", go);
      body.removeEventListener("focusin", arrived);
      body.removeEventListener("focusout", gone);
      // Only when the card is being taken away from under the reader's focus,
      // which is what `Escape` does: focus was on a link that no longer exists,
      // and leaving it on `<body>` sends the reader back to the top of the
      // page. A card that closed because the pointer left, with focus
      // somewhere else entirely, has no business moving it.
      if (held.current) {
        held.current = false;
        triggerRef.current?.focus?.();
      }
    };
  }, [open, intent, closeDelay, triggerRef]);

  if (!open) {
    return null;
  }

  return (
    <div
      {...withoutComposed(rest, ["ref"])}
      data-align={anchored.align}
      data-side={anchored.side}
      data-state="open"
      id={`${card.base}-body`}
      ref={composeRefs(rest.ref, (element) => {
        bodyRef.current = element;
      })}
    >
      {children}
    </div>
  );
}
