// @flow
//
// Keeping a closing part on the page until its exit transition has finished.
//
// Every overlay part in this package is on the page only while it is open:
// `Dialog.Body`, `Popover.Body`, `Tooltip.Body`, a menu, a listbox and a toast
// render `null` once they have closed. That is the right default for
// accessibility, because a closed dialog that stays in the document is one a
// screen reader can still wander into. Rendering `null` at the moment of
// closing, though, leaves a stylesheet nothing to animate: by the time a
// `[data-state=closed]` rule could apply, the element is gone.
//
// `usePresence` is the smallest change that gives a stylesheet an exit and
// keeps the accessibility default. A part asks it whether to render. The
// answer stays yes for a while after `open` goes false, and during that time
// the part is marked `data-state="closed"`, so the stylesheet can transition
// it out. The hook watches the part's own transitions and says no once they
// have all finished.
//
// # What it owns, and what it does not
//
// It owns **one decision: whether a part is on the page.** Everything else
// about closing stays keyed on `open`, and so it happens at the moment of
// closing, not after the transition:
//
//   * Focus goes back to the trigger when `open` goes false. A reader should
//     not wait 200ms for the keyboard to answer.
//   * The scroll lock lifts when `open` goes false, so the page can be
//     scrolled while the panel fades.
//   * A closing part is `inert`: it cannot be clicked, focused or found by a
//     screen reader while it fades. That is the caller's job, so the caller
//     reads `state` and writes `inert` itself, beside the `data-state` it
//     already writes. The hook renders nothing.
//
// # How it knows the exit has finished
//
// In the layout effect of the render that closed the part, before the browser
// paints it, the hook asks the element for `getAnimations({ subtree: true })`
// and waits for every one of them to finish. It does not use a timer, a
// duration read from the stylesheet, or `transitionend`:
//
//   * A **timer** would be a second copy of a duration the stylesheet already
//     holds, and the two drift apart the first time a theme changes one.
//   * **`transitionend`** does not fire for a transition that was never
//     started, such as under reduced motion or a property that did not
//     change. It also fires once per property, and it bubbles up from every
//     child.
//   * **`getAnimations()`** returns exactly what is running, CSS transitions
//     and keyframe animations alike. It brings style up to date first, so the
//     transitions this commit started are already in the list.
//
// An empty list means there is nothing to wait for: no stylesheet, `0s` under
// reduced motion, or a test DOM that runs no CSS. The part is then removed in
// the same commit, before anything is painted, which is exactly how it
// behaved before this hook existed. That is why this is a layout effect: a
// passive one would paint one frame of a closed part that has no exit to play,
// and a synchronous test would see a part that is gone in a browser.
//
// Two kinds of animation are not waited for, because they will not end on
// their own: one that repeats for ever (a spinner inside a dialog) and one
// that is paused. Waiting on either would keep a closed part on the page for
// as long as the page is open. An animation that is *cancelled* rather than
// finished counts as finished. Its `finished` promise rejects when the
// element's style changes under it or it is removed, and a part must never
// stay on the page because of an animation that will not end.
//
// While it waits, the part is `inert`, and an inert element is not hit by the
// pointer. A part that lingers at `opacity: 0` because something in it is
// still animating is invisible and cannot be pressed, so it costs the reader
// nothing.
//
// # Reopening part-way through
//
// Suppose the reader closes and reopens before the exit has finished. `open`
// is true again, so the part is simply open: `state` is `"open"` and the part
// was never removed. The pending wait belongs to an effect that has been
// cleaned up, so when that exit's animations settle, nothing happens. In CSS
// the transition reverses from wherever it had got to, which is what a
// transition does when its target changes.
//
// # The Rules of React
//
// * **Nothing is read from the DOM while rendering.** `ref.current` is read
//   only in the effect. Rendering depends on `open` and on the hook's own
//   state, so this is safe under concurrent rendering and the React Compiler.
// * **The previous `open` is state, not a ref.** When `open` changes, the
//   hook updates its state during render. React documents this pattern
//   ("storing information from previous renders"): it re-renders immediately
//   without committing the stale answer. So the render that closes a part
//   already says `present: true`, and the part never disappears for a frame.
// * **State is set in the layout effect only when there is nothing to wait
//   for.** That is the measure-then-render pattern React documents for layout
//   effects: the answer depends on the DOM, and it has to be known before the
//   browser paints. Otherwise the effect only subscribes to promises and sets
//   state in their callbacks.
// * **StrictMode's double effect is harmless.** The first run is cleaned up
//   before its promise settles. The second run waits on the same animations
//   and alone removes the part.
// * **On the server** there are no effects. An open part renders as open, a
//   closed one renders nothing, and there is nothing to wait for.

import { useLayoutEffect, useState } from "@uniflowed/react";

/** Where a part is in its life: showing, or on its way out. */
export type PresenceState = "open" | "closed";

/** What `usePresence` answers with. */
export type Presence = {
  /**
   * Whether the part should be rendered at all. True while `open`, and for as
   * long after it as the part's exit animations run.
   */
  readonly present: boolean,
  /**
   * What the part should write as `data-state`: `"closed"` whenever `open` is
   * false. For a part that renders nothing once it is not `present`, that is
   * exactly while its exit plays. `presenceProps` adds `inert` for that time.
   */
  readonly state: PresenceState,
};

/**
 * Whether a part that shows while `open` should still be on the page, and
 * the state to mark it with.
 *
 * `open` is the part's own open state, such as `dialog.open`. `ref` is the
 * element whose animations decide when a closing part may go. It must be the
 * element the part renders, and it only needs to be set while `present` is
 * true. A part that mounts closed is not present and costs nothing, and a
 * part that closes with no running animations is removed in the same commit,
 * before the browser paints.
 *
 * The caller renders `null` when `present` is false. Otherwise it writes
 * `data-state={state}`, and adds `inert` when `state` is `"closed"`. The
 * module header explains why focus and the scroll lock stay keyed on `open`
 * and not on this.
 */
export hook usePresence(open: boolean, ref: { readonly current: HTMLElement | null }): Presence {
  const [shown, setShown] = useState<boolean>(open);
  const [exiting, setExiting] = useState<boolean>(false);

  // `open` changed since the last render: update the state now. Closing starts
  // the exit and opening cancels it. React re-renders at once with the new
  // state, so no frame is committed with the part missing (see the header).
  if (open !== shown) {
    setShown(open);
    setExiting(!open);
  }

  useLayoutEffect(() => {
    if (!exiting) {
      return;
    }
    const running = exitAnimations(ref.current);
    if (running.length === 0) {
      // Nothing to wait for, so the part goes in this commit, before paint.
      setExiting(false);
      return;
    }
    let settled = false;
    const done = () => {
      if (!settled) {
        setExiting(false);
      }
    };
    // A cancelled animation rejects `finished`. It is treated as finished too:
    // a part must not stay on the page waiting for an animation that ended
    // some other way.
    Promise.all(running.map((animation) => animation.finished.catch(() => undefined))).then(
      done,
      done,
    );
    return () => {
      settled = true;
    };
  }, [exiting, ref]);

  return { present: open || exiting, state: open ? "open" : "closed" };
}

/** The part of an `Animation` this module reads. */
type Settling = {
  readonly finished: Promise<mixed>,
  readonly playState?: string,
  readonly effect?: ?{
    readonly getComputedTiming?: () => { readonly endTime?: number, ... },
    ...
  },
  ...
};

/**
 * The animations on `element` and inside it that a closing part should wait
 * for: everything running that will end by itself.
 *
 * An animation that repeats for ever has an `endTime` of `Infinity`, and a
 * paused one does not advance, so neither is in the answer; the module header
 * says why. A DOM without `getAnimations` answers nothing, which is a part
 * with nothing to wait for.
 */
function exitAnimations(element: HTMLElement | null): $ReadOnlyArray<Settling> {
  // Read through a structural type: `getAnimations` is recent enough that a
  // DOM library may not declare it, and a test DOM may not have it.
  const host: ?{
    readonly getAnimations?: (options: { subtree: boolean }) => $ReadOnlyArray<Settling>,
    ...
  } = element as $FlowFixMe;
  if (host == null || typeof host.getAnimations !== "function") {
    return [];
  }
  return host.getAnimations({ subtree: true }).filter((animation) => {
    if (animation.playState === "paused") {
      return false;
    }
    const timing = animation.effect?.getComputedTiming?.();
    return timing?.endTime == null || Number.isFinite(timing.endTime);
  });
}

/**
 * Whether anything on `element` or inside it is animating towards an end.
 *
 * For a part that measures itself: a measurement taken while a transition runs
 * reads a frame of the transition, and one that briefly changes the element's
 * own style to measure it would cancel the transition outright.
 */
export function isAnimating(element: HTMLElement | null): boolean {
  return exitAnimations(element).length > 0;
}

/**
 * The attributes a part writes: `data-state`, for the stylesheet, and `inert`
 * while it is closing, so a part on its way out cannot be pressed, focused or
 * found by a screen reader.
 *
 * `inert` is `undefined` otherwise, which React renders as no attribute at
 * all. That includes a part that has finished closing and is still in the
 * document, like a disclosure's `hidden` panel: find-in-page does not look
 * inside an inert element, and `hidden="until-found"` exists to be found.
 */
export function presenceProps(presence: Presence): {|
  "data-state": PresenceState,
  inert: true | void,
|} {
  return {
    "data-state": presence.state,
    inert: presence.present && presence.state === "closed" ? true : undefined,
  };
}
