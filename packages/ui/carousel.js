// @flow
//
// A carousel: content that moves, which is the one thing on a page a
// specification tells you to let people stop.
//
// # What it gives that a list does not
//
// Nothing, for a reader who can see it. A carousel is a list of things shown
// one at a time, and the plain HTML it replaces — a list — is better at every
// job except fitting in a small space. So the whole of this module is the part
// that keeps the replacement from being worse than the list:
//
//   * **It can be stopped.** WCAG 2.2.2, *Pause, Stop, Hide*: anything that
//     starts automatically, moves, and lasts more than five seconds needs a
//     mechanism to pause it. `Carousel.Pause` is that mechanism, and it must be
//     the **first** focusable thing inside the carousel — a pause button after
//     the slides is a pause button nobody reaches in time. That is enforced
//     here rather than suggested: an autoplaying carousel whose first focus
//     stop is not the pause control raises.
//   * **It says what it is.** `aria-roledescription="carousel"` on a named
//     group, and `aria-roledescription="slide"` with "3 of 7" on each slide.
//     Without them a reader is told "group, group" and has no way to know
//     where they are or how much of it there is.
//   * **It stops announcing itself while it moves.** The slide container is
//     `aria-live="off"` while it is rotating and `"polite"` while it is not.
//     A live region that reads out every slide of an auto-rotating carousel is
//     unusable, and one that never announces anything makes the Next button
//     silent.
//   * **`Tab` cannot walk into a slide nobody can see.** This is the bug that
//     survives every other fix. The slides that are scrolled out of view are
//     still in the DOM, so their links and buttons are still focus stops — and
//     a reader who tabs into one is in content the page is not showing. They
//     are `inert`, which takes them out of the tab order *and* out of the
//     accessibility tree, and which `internal/focus.js` already skips.
//   * **It respects `prefers-reduced-motion`.** A reader who asked their system
//     to stop moving things gets a carousel that does not rotate on its own.
//     `usePrefersReducedMotion` from `@uniflowed/hooks/browser`.
//
// Rotation also stops while the pointer is over it and while focus is inside
// it — a reader in the middle of reading a slide should not have it taken away
// — and once `Carousel.Pause` has been pressed it stays stopped, because that
// was a decision rather than a hover.
//
// # Why the caller counts the slides
//
// `Carousel.Root` takes `count` and `Carousel.Item` takes `index`, the same way
// `Table.Root` takes `rowCount` and `Table.Row` takes `index`. The alternative
// — counting the children — is wrong the first time a caller renders a slide
// conditionally, filters a list, or wraps one in a component of their own, and
// it is wrong silently: the label says "3 of 6" in a carousel with seven
// slides, which is exactly the sentence a reader is relying on.
//
// # It is a `group`, not a `region`
//
// A `region` is a landmark, and a landmark is a promise that this is one of the
// handful of places worth jumping to on the page. A gallery of photographs
// three screens down is not, and a page with four carousels in it would put
// four entries in a reader's landmark list. `role="group"` says the same thing
// about the relationship between the slides without making that claim; a caller
// whose carousel *is* the page can pass `role="region"` and get it.

"use client";

import * as React from "@uniflowed/react";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "@uniflowed/react";
import { usePrefersReducedMotion } from "@uniflowed/hooks/browser";

import type { Orientation } from "./internal/roving-focus.js";
import type { Rest } from "./internal/merge-props.js";
import {
  composeHandlers,
  composeRefs,
  forwarded,
  withoutComposed,
} from "./internal/merge-props.js";
import { focusable } from "./internal/focus.js";
import { useControlled } from "./internal/controlled-state.js";

export type { Orientation } from "./internal/roving-focus.js";

type CarouselState = {|
  readonly base: string,
  readonly count: number,
  readonly index: number,
  readonly setIndex: (next: number) => void,
  readonly loop: boolean,
  readonly orientation: Orientation,
  /** Whether it is rotating right now, which is what `aria-live` reads. */
  readonly rotating: boolean,
  /** Whether the reader stopped it on purpose, which nothing but they undo. */
  readonly stopped: boolean,
  readonly setStopped: (stopped: boolean) => void,
  /** Whether a rotation was ever asked for, so `Carousel.Pause` can say so. */
  readonly rotates: boolean,
  readonly registerPause: (present: boolean) => void,
|};

const CarouselContext: React.Context<CarouselState | null> = createContext(null);

/**
 * The carousel a part belongs to.
 *
 * Raising rather than returning null, for the reason `useDialog` gives: a
 * `Carousel.Item` outside a root would render a slide labelled "1 of 0".
 */
hook useCarousel(part: string): CarouselState {
  const state = useContext(CarouselContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside a Carousel.Root`);
  }
  return state;
}

/**
 * The carousel: a named group of slides, one of them showing.
 *
 * `autoplay` is how many milliseconds each slide is shown for, or `null` for a
 * carousel that only moves when it is asked to. A carousel that rotates must
 * hold a `Carousel.Pause`, and that one must be the first thing `Tab` reaches
 * inside it; both are checked.
 */
export component CarouselRoot(
  children: React.Node,
  autoplay?: number | null = null,
  count: number,
  defaultIndex?: number = 0,
  index?: number,
  label: string,
  loop?: boolean = true,
  onIndexChange?: (index: number) => void,
  orientation?: Orientation = "horizontal",
  ...rest: Rest
) {
  const base = useId();
  const [current, setIndex] = useControlled(index, defaultIndex, onIndexChange);
  const rootRef = useRef<HTMLElement | null>(null);
  const paused = useRef(0);
  const [stopped, setStopped] = useState(false);
  // Whether the pointer or focus is resting on it. State rather than a ref,
  // because the timer below is an effect and has to be torn down when it
  // changes.
  const [held, setHeld] = useState(false);
  const reducedMotion = usePrefersReducedMotion();
  const passed = withoutComposed(rest, [
    "onBlur",
    "onFocus",
    "onPointerEnter",
    "onPointerLeave",
    "ref",
  ]);
  // Stable, so `Carousel.Pause`'s registration effect runs once rather than
  // once per render of the root — which would decrement and re-increment the
  // count, and leave it at zero for exactly as long as it takes the check
  // below to read it.
  const registerPause = useCallback((present: boolean) => {
    paused.current += present ? 1 : -1;
  }, []);

  // A reader who asked their system to stop moving things has answered this
  // question already, and the answer is not "rotate anyway and offer a button".
  const rotates = autoplay != null && !reducedMotion;
  const rotating = rotates && !stopped && !held;

  const state = useMemo(
    () => ({
      base,
      count,
      index: current,
      loop,
      orientation,
      registerPause,
      rotates,
      rotating,
      setIndex,
      setStopped,
      stopped,
    }),
    [base, count, current, loop, orientation, registerPause, rotates, rotating, setIndex, stopped],
  );

  useEffect(() => {
    if (!rotating || count <= 1) {
      return;
    }
    // The global timer rather than the document's, the same as
    // `internal/hover-intent.js`: one clock for the package, and the one a
    // caller's fake timers replace.
    const timer = setTimeout(() => {
      setIndex(current === count - 1 ? 0 : current + 1);
    }, autoplay ?? 0);
    return () => {
      clearTimeout(timer);
    };
    // `current` is named, so each slide's turn is timed from the moment it
    // arrived rather than from a repeating interval that keeps running while
    // the reader presses Next.
  }, [autoplay, count, current, rotating, setIndex]);

  useEffect(() => {
    const root = rootRef.current;
    if (!rotates || root == null) {
      return;
    }
    if (paused.current === 0) {
      throw new Error(
        "A Carousel.Root with autoplay must hold a Carousel.Pause: WCAG 2.2.2 " +
          "requires a mechanism to stop anything that moves by itself for more " +
          "than five seconds.",
      );
    }
    const first = focusable(root)[0];
    if (first == null || first.getAttribute("data-uf-carousel-pause") == null) {
      throw new Error(
        "Carousel.Pause must be the first focusable element inside " +
          "Carousel.Root: a pause control the reader reaches after the slides " +
          "is one they reach after the thing they wanted to stop.",
      );
    }
  }, [rotates]);

  return (
    <CarouselContext.Provider value={state}>
      <div
        {...passed}
        aria-label={label}
        // What the reader is told instead of "group". Everything else here is
        // arrangement; this is the sentence.
        aria-roledescription="carousel"
        data-orientation={orientation}
        onBlur={composeHandlers(rest.onBlur, (event: $FlowFixMe) => {
          if (!event.currentTarget?.contains?.(event.relatedTarget)) {
            setHeld(false);
          }
        })}
        // Rotation stops while a reader is in it and starts again when they
        // leave — unless they stopped it deliberately, which `stopped` keeps.
        onFocus={composeHandlers(rest.onFocus, () => setHeld(true))}
        onPointerEnter={composeHandlers(rest.onPointerEnter, () => setHeld(true))}
        onPointerLeave={composeHandlers(rest.onPointerLeave, () => setHeld(false))}
        ref={composeRefs(rest.ref, (element: HTMLElement | null) => {
          rootRef.current = element;
        })}
        role="group"
      >
        {children}
      </div>
    </CarouselContext.Provider>
  );
}

/**
 * The slides, and the live region that says which one is showing.
 *
 * `aria-live="off"` while it rotates: a live region reading out a slide every
 * four seconds is a page a screen reader cannot be used on. `"polite"` the rest
 * of the time, so pressing Next says something.
 */
export component CarouselContent(children: React.Node, ...rest: Rest) {
  const carousel = useCarousel("Carousel.Content");

  return (
    <div
      {...rest}
      aria-live={carousel.rotating ? "off" : "polite"}
      data-orientation={carousel.orientation}
      id={`${carousel.base}-content`}
    >
      {children}
    </div>
  );
}

/**
 * One slide, which says where in the set it is and gets out of the way when it
 * is not the one showing.
 *
 * `inert` rather than a class: the slides that are not showing are still in the
 * document, and without it `Tab` walks into a link nobody can see. It also
 * takes the subtree out of the accessibility tree, which is what stops a reader
 * being read six slides in a row.
 */
export component CarouselItem(children: React.Node, index: number, ...rest: Rest) {
  const carousel = useCarousel("Carousel.Item");
  const current = index === carousel.index;

  return (
    <div
      {...rest}
      // "3 of 7", which is the only way a reader knows where they are. A caller
      // who has a better name for the slide keeps it.
      aria-label={
        rest["aria-label"] == null && rest["aria-labelledby"] == null
          ? `${String(index + 1)} of ${String(carousel.count)}`
          : undefined
      }
      aria-roledescription="slide"
      data-state={current ? "active" : "inactive"}
      // React renders `inert` from a boolean, and `undefined` removes it.
      inert={current ? undefined : true}
      role="group"
    >
      {children}
    </div>
  );
}

/**
 * The control WCAG 2.2.2 is about, and the first thing `Tab` reaches.
 *
 * It says which state pressing it produces, which is what a toggle button is
 * for: `aria-pressed` on a pause button is the announcement "pause, pressed",
 * and a reader who has stopped a carousel wants to be told it is stopped.
 */
export component CarouselPause(
  children?: React.Node,
  pauseLabel?: string = "Stop the carousel",
  playLabel?: string = "Start the carousel",
  ...rest: Rest
) {
  const carousel = useCarousel("Carousel.Pause");
  const register = carousel.registerPause;
  const passed = withoutComposed(rest, ["onClick"]);
  const named = rest["aria-label"] != null || rest["aria-labelledby"] != null;

  useEffect(() => {
    register(true);
    return () => register(false);
  }, [register]);

  return (
    <button
      {...passed}
      aria-controls={`${carousel.base}-content`}
      aria-label={named ? undefined : carousel.stopped ? playLabel : pauseLabel}
      aria-pressed={carousel.stopped ? "true" : "false"}
      // How `Carousel.Root` recognises this button as the pause control without
      // reaching into React's tree, which it has no way to do from an effect.
      data-uf-carousel-pause=""
      onClick={composeHandlers(rest.onClick, () => carousel.setStopped(!carousel.stopped))}
      type="button"
    >
      {children}
    </button>
  );
}

/** The button that goes back one slide. */
export component CarouselPrevious(
  children?: React.Node,
  label?: string = "Previous slide",
  ...rest: Rest
) {
  return (
    <CarouselStep {...forwarded(rest)} label={label} step={-1}>
      {children}
    </CarouselStep>
  );
}

/** The button that goes forward one slide. */
export component CarouselNext(children?: React.Node, label?: string = "Next slide", ...rest: Rest) {
  return (
    <CarouselStep {...forwarded(rest)} label={label} step={1}>
      {children}
    </CarouselStep>
  );
}

/**
 * Both of the stepping buttons.
 *
 * One component because the difference is a sign and a name, and two copies of
 * the wrapping arithmetic is how a carousel comes to loop in one direction and
 * stop in the other.
 */
component CarouselStep(children?: React.Node, label: string, step: number, ...rest: Rest) {
  const carousel = useCarousel(step < 0 ? "Carousel.Previous" : "Carousel.Next");
  const passed = withoutComposed(rest, ["onClick"]);
  const last = carousel.count - 1;
  const at = step < 0 ? 0 : last;
  const wrapped = step < 0 ? last : 0;
  const ends = carousel.index === at;

  return (
    <button
      {...passed}
      aria-controls={`${carousel.base}-content`}
      aria-label={rest["aria-label"] == null ? label : undefined}
      // Disabled at the end of a carousel that does not loop, because a button
      // that does nothing is a button a reader presses twice before believing
      // it.
      disabled={!carousel.loop && ends}
      onClick={composeHandlers(rest.onClick, () => {
        carousel.setIndex(ends ? wrapped : carousel.index + step);
      })}
      type="button"
    >
      {children}
    </button>
  );
}
