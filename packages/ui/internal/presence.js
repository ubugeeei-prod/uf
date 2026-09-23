// @flow
//
// Keeping a closing part on the page until its exit transition has finished.
//
// Every overlay part in this package is on the page only while it is open:
// `Dialog.Body`, `Popover.Body`, `Tooltip.Body`, a menu, a listbox and a toast
// all render `null` once they close. That is the right default for
// accessibility, because a closed dialog that stays in the document is one a
// screen reader can still wander into. It is also why a stylesheet cannot
// animate anything leaving. By the time a `[data-state=closed]` rule could
// apply, the element is gone.
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
// Once the render that closed the part has been committed, the effect asks the
// element for `getAnimations({ subtree: true })` and waits for every one of
// them to finish. It does not use a timer, a duration read from the
// stylesheet, or `transitionend`:
//
//   * A **timer** would be a second copy of a duration the stylesheet already
//     holds, and the two drift apart the first time a theme changes one.
//   * **`transitionend`** does not fire for a transition that was never
//     started, such as under reduced motion or a property that did not
//     change. It also fires once per property, and it bubbles up from every
//     child.
//   * **`getAnimations()`** returns exactly what is running, CSS transitions
//     and keyframe animations alike. It brings style up to date first, so the
//     transitions this commit started are already in the list. An empty list
//     means there is nothing to wait for: no stylesheet, `0s` under reduced
//     motion, or a test DOM without the method. The part is then removed on
//     the next microtask, which is how it behaved before this hook existed.
//
// An animation that is *cancelled* rather than finished counts as finished.
// Its `finished` promise rejects when the element's style changes under it or
// it is removed, and a part must never stay on the page because of an
// animation that will not end.
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
// * **No state is set synchronously in an effect.** The effect only
//   subscribes to promises, and it sets state in their callbacks.
// * **StrictMode's double effect is harmless.** The first run is cleaned up
//   before its promise settles. The second run waits on the same animations
//   and alone removes the part.
// * **On the server** there are no effects. An open part renders as open, a
//   closed one renders nothing, and there is nothing to wait for.

import { useEffect, useState } from "@uniflowed/react";

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
   * What the part should write as `data-state`, and whether it should be
   * `inert`. `"closed"` only while `present` is true and `open` is false,
   * which is while the exit plays.
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
 * part that closes with no running animations is removed on the next
 * microtask.
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

  useEffect(() => {
    if (!exiting) {
      return;
    }
    let settled = false;
    const done = () => {
      if (!settled) {
        setExiting(false);
      }
    };
    const element: $FlowFixMe = ref.current;
    const running: $ReadOnlyArray<{ readonly finished: Promise<mixed>, ... }> =
      element != null && typeof element.getAnimations === "function"
        ? element.getAnimations({ subtree: true })
        : [];
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
