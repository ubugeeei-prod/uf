// @flow
//
// Two panes and the handle between them — a slider wearing a different role.
//
// The APG calls this a **window splitter**, and it defines it as a separator
// that behaves like a slider: `aria-valuenow`, `aria-valuemin` and
// `aria-valuemax` say how much of the space the primary pane has, and the
// arrow keys change it. That is why it is next to `slider.js` and shares
// `internal/range.js` with it rather than living with the layout components.
//
// Almost every resizable panel on the web is pointer-only, which is a WCAG
// 2.1.1 failure — no keyboard operation at all — and a 2.5.7 one on top of it.
// If uf ships one the keyboard is the feature, so the keyboard was written
// first and shipped on its own: a keyboard-only splitter is a working
// splitter, where a pointer-only one is not.
//
// It was not the finished control. A bar between two panes is a bar that
// approximately everybody takes hold of first, and this one could not be
// moved that way at all. Both halves are here now, and the key map is exactly
// what it was — a drag that had quietly replaced it would be the first failure
// in the other direction.
//
// # The drag, and the step it does not use
//
// It is `slider.js`'s `Slider.Track` arithmetic measured against the *group's*
// box rather than a track's. `pointerdown` captures the pointer, so a drag
// that wanders off a bar four pixels wide — which every drag does — keeps
// arriving at the handle instead of being lost to whatever it wandered over;
// `pointermove` turns the position into a percentage from
// `getBoundingClientRect`; and the percentage goes through `internal/range.js`
// like every other value here, so a drag cannot leave the pane anywhere an
// arrow key could not put it, and `aria-valuenow` is true about it while it
// moves.
//
// Pressing the handle does not move it. A press on a *track* means "put the
// value here", which is what `Slider.Track` does with one and why it is half
// of WCAG 2.5.7 there; a press on a handle means "take hold of this". A
// splitter that also jumped by the distance between the pointer and its own
// centre would move a little every time it was clicked, which is the one thing
// a person who clicked it did not ask for.
//
// `step` stays the keyboard's, and the pointer has one of its own. `step`
// defaults to 10 because ten presses of an arrow key ought to cross the pane,
// and 10 is an absurd granularity for a bar being dragged under a pointer that
// moves smoothly. The pointer's is one percent, and it is a constant rather
// than a second prop because the value is already a percentage of the group:
// one is the smallest move that means anything, and a splitter announcing
// 47.382 would be reading out its arithmetic rather than its size.
//
// # The two separators in this package are not the same thing
//
// A reader who greps for `separator` finds this and `menu.js`, and they are
// unrelated:
//
//   * `Menu.Separator` is a rule between groups of items. It is not focusable,
//     it has no value, and it exists so a reader moving through a menu is told
//     the group changed.
//   * `Resizable.Handle` is a *window splitter*. It is focusable, it carries a
//     value, and operating it changes the layout.
//
// ARIA gives both the same role because both are separators; only the second
// one is a control. The difference is `tabindex` and `aria-valuenow`, which is
// also how a screen reader tells them apart.
//
// # `aria-orientation` is the separator's, not the layout's
//
// Two panes side by side are divided by a **vertical** line, so a
// `PanelGroup` whose `orientation` is `"horizontal"` renders a handle whose
// `aria-orientation` is `"vertical"`. That inversion is easy to get backwards
// and worth stating: `aria-orientation` on a separator describes the separator,
// and ARIA's default for the role is `horizontal` — so a vertical splitter
// that says nothing is announced as a horizontal rule.
//
// The APG's own pattern page does not mention `aria-orientation` at all, which
// is why implementations differ. This follows the role's definition rather
// than the pattern's silence.
//
// # Two panes, which is the pattern and not a limitation
//
// The window splitter is defined between two panes: a primary one whose size
// is the value, and the rest. A group of five panels is a layout-constraint
// problem — every handle's range depends on every other panel's minimum — and
// it is a different component with a different core, not a bigger version of
// this one. What is here is the pattern, complete.
//
// # Drawing it
//
// Each pane carries its share as `--uf-resizable-size`, a percentage, and the
// caller's stylesheet decides whether that is a width, a height, a `flex-basis`
// or nothing at all. The group is measured rather than drawn — the drag reads
// its box and never writes to it — so a caller whose panes are flex children,
// grid tracks or absolutely positioned gets the same splitter.

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

import type { Rest } from "./internal/merge-props.js";
import { composeHandlers, composeRefs, withoutComposed } from "./internal/merge-props.js";
import type { Orientation } from "./internal/roving-focus.js";
import { clamp, isReversed, snap } from "./internal/range.js";
import { useControlled } from "./internal/controlled-state.js";

/**
 * How finely a drag may move the splitter, in percentage points.
 *
 * Not `step`, which is the keyboard's and defaults to ten; the module header
 * says why the two cannot be the same number. One is a constant rather than a
 * prop because the value is already a percentage of the group, so one is the
 * smallest move that means anything.
 */
const POINTER_STEP = 1;

type ResizableState = {|
  readonly base: string,
  /** The primary pane's share of the group, as a percentage. */
  readonly value: number,
  readonly setValue: (value: number) => void,
  readonly min: number,
  readonly max: number,
  readonly step: number,
  /** How the panes are laid out; the handle's own orientation is the other one. */
  readonly orientation: Orientation,
  readonly disabled: boolean,
  readonly hasPrimary: boolean,
  readonly registerPrimary: (present: boolean) => void,
  /**
   * The element a drag is measured against.
   *
   * The group rather than the handle, because the value is the primary pane's
   * share *of the group* — the handle is a few pixels wide and has no idea how
   * much space there is to divide.
   */
  readonly groupRef: { current: HTMLElement | null },
|};

const ResizableContext: React.Context<ResizableState | null> = createContext(null);

hook useResizable(part: string): ResizableState {
  const state = useContext(ResizableContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside a Resizable.PanelGroup`);
  }
  return state;
}

/**
 * The two panes and their handle.
 *
 * `value` is the primary pane's percentage of the group, which is what the
 * handle announces — the APG's "a decimal value representing the current
 * position of the separator", where 0 is collapsed and 100 is as large as it
 * is allowed to be.
 *
 * `min` is the size the primary pane collapses to. It is 0 by default, so
 * `Enter` collapses the pane entirely; a group whose primary pane should never
 * disappear gives it a floor.
 */
export component ResizablePanelGroup(
  children: React.Node,
  value?: number,
  defaultValue?: number = 50,
  onValueChange?: (value: number) => void,
  min?: number = 0,
  max?: number = 100,
  step?: number = 10,
  orientation?: Orientation = "horizontal",
  disabled?: boolean = false,
  ...rest: Rest
) {
  const base = useId();
  const [share, setShare] = useControlled(value, defaultValue, onValueChange);
  const [hasPrimary, setHasPrimary] = useState(false);
  const groupRef = useRef<HTMLElement | null>(null);

  const state = useMemo(
    () => ({
      base,
      value: clamp(share, min, max),
      setValue: setShare,
      min,
      max,
      step,
      orientation,
      disabled,
      hasPrimary,
      registerPrimary: setHasPrimary,
      groupRef,
    }),
    [base, share, setShare, min, max, step, orientation, disabled, hasPrimary],
  );

  const passed = withoutComposed(rest, ["ref"]);

  return (
    <ResizableContext.Provider value={state}>
      <div
        {...passed}
        ref={composeRefs(rest.ref, (element) => {
          groupRef.current = element;
        })}
      >
        {children}
      </div>
    </ResizableContext.Provider>
  );
}

/**
 * One pane.
 *
 * `primary` marks the one whose size is the value, and the one the handle
 * names with `aria-controls`. Exactly one pane in a group is primary; the
 * other takes what is left. It is a prop rather than "the first one", because
 * the first one in the document is not the first one to mount the moment a
 * caller renders a pane conditionally, and a handle pointing at the wrong pane
 * is a handle that announces someone else's size.
 */
export component ResizablePanel(children: React.Node, primary?: boolean = false, ...rest: Rest) {
  const group = useResizable("Resizable.Panel");
  const passed = withoutComposed(rest, ["style"]);
  const register = group.registerPrimary;

  useEffect(() => {
    if (!primary) {
      return;
    }
    register(true);
    return () => register(false);
  }, [primary, register]);

  return (
    <div
      {...passed}
      id={primary ? `${group.base}-primary` : undefined}
      style={{
        ...(rest.style as $FlowFixMe),
        "--uf-resizable-size": `${String(primary ? group.value : 100 - group.value)}%`,
      }}
    >
      {children}
    </div>
  );
}

/**
 * The splitter: a separator that behaves like a slider.
 *
 * `label` is its accessible name and has a default, because a splitter is a
 * bare bar with no text in it every time — and a focusable separator with no
 * name is announced as "separator", which tells a reader there is a control
 * here and nothing about what it does.
 */
export component ResizableHandle(label?: string = "Resize", ...rest: Rest) {
  const group = useResizable("Resizable.Handle");
  const passed = withoutComposed(rest, [
    "onKeyDown",
    "onPointerCancel",
    "onPointerDown",
    "onPointerMove",
    "onPointerUp",
  ]);
  // Where the pane was before `Enter` collapsed it. A ref because nothing
  // renders it: it is a fact about the last keystroke, not about the layout.
  const restoreTo = useRef<number | null>(null);
  // Whether the pointer is down on this handle. Also a ref, and for the same
  // reason: it changes between renders and no render depends on it.
  const dragging = useRef(false);

  const moveBy = (amount: number) => {
    restoreTo.current = null;
    group.setValue(clamp(group.value + amount, group.min, group.max));
  };

  /** The primary pane's share at the pointer, or null with no box to read. */
  const shareAt = (event: $FlowFixMe): number | null => {
    const element = group.groupRef.current;
    if (element == null) {
      return null;
    }
    const box = element.getBoundingClientRect();
    const vertical = group.orientation === "vertical";
    const size = vertical ? box.height : box.width;
    if (size <= 0) {
      // A group with no box has no percentages in it, and dividing by its
      // width would put `Infinity` into `aria-valuenow`.
      return null;
    }
    // From the top for stacked panes, where `Slider.Track` reads from the
    // bottom: a slider's minimum is at the bottom of its track, and a group's
    // primary pane is the one *before* the handle, which is the top one.
    const along = vertical ? event.clientY - box.top : event.clientX - box.left;
    const part = clamp(along / size, 0, 1);
    // In a right-to-left page the pane before the handle is the one on the
    // right, so the reading runs the other way. `isReversed` mirrors nothing on
    // a stacked group, because writing direction does not flip the vertical
    // axis.
    const forward = isReversed(element, group.orientation) ? 1 - part : part;
    // The value *is* the percentage, so the position becomes one directly
    // rather than being mapped across `min`–`max` the way a slider's is: those
    // two are bounds on how far the pane may be dragged, not the ends of a
    // scale. Mapping them would put the handle somewhere the pointer is not.
    return snap(forward * 100, group.min, group.max, POINTER_STEP);
  };

  const endDrag = (event: $FlowFixMe) => {
    dragging.current = false;
    event.currentTarget?.releasePointerCapture?.(event.pointerId);
  };

  return (
    <div
      {...passed}
      aria-label={label}
      // Only while a primary pane is in the document, for the reason every
      // part of this package repeats: an `aria-controls` naming an id nothing
      // has is worse than saying nothing at all.
      aria-controls={group.hasPrimary ? `${group.base}-primary` : undefined}
      aria-disabled={group.disabled ? "true" : undefined}
      // The separator's own orientation, which is the other axis from the one
      // the panes are laid out along. See the module header.
      aria-orientation={group.orientation === "horizontal" ? "vertical" : "horizontal"}
      aria-valuemax={group.max}
      aria-valuemin={group.min}
      aria-valuenow={group.value}
      onKeyDown={composeHandlers(rest.onKeyDown, (event: $FlowFixMe) => {
        if (group.disabled) {
          return;
        }
        if (event.key === "Enter") {
          event.preventDefault();
          // Collapse, or put it back where it was. The APG gives `Enter` both
          // jobs, and a collapse with no way back is a pane a keyboard reader
          // has thrown away.
          const previous = restoreTo.current;
          if (previous != null) {
            restoreTo.current = null;
            group.setValue(previous);
            return;
          }
          if (group.value === group.min) {
            return;
          }
          restoreTo.current = group.value;
          group.setValue(group.min);
          return;
        }

        if (event.key === "Home" || event.key === "End") {
          event.preventDefault();
          restoreTo.current = null;
          group.setValue(event.key === "Home" ? group.min : group.max);
          return;
        }

        const amount = stepFor(
          event.key,
          group.step,
          group.orientation,
          isReversed(event.currentTarget, group.orientation),
        );
        if (amount != null) {
          event.preventDefault();
          moveBy(amount);
        }
      })}
      onPointerCancel={composeHandlers(rest.onPointerCancel, endDrag)}
      onPointerDown={composeHandlers(rest.onPointerDown, (event: $FlowFixMe) => {
        if (group.disabled) {
          return;
        }
        // Otherwise the press selects the text in the panes either side on the
        // way past, so a drag paints half the page blue.
        event.preventDefault();
        // Which also means the browser will not focus this element, and a
        // reader who has just dragged the splitter is the reader most likely to
        // want an arrow key next.
        event.currentTarget?.focus?.();
        event.currentTarget?.setPointerCapture?.(event.pointerId);
        dragging.current = true;
        // A drag is a move, so the pane `Enter` would put back is no longer
        // where it was. Leaving it would make the next `Enter` restore a size
        // from before the drag.
        restoreTo.current = null;
        // Deliberately no value change: taking hold of the handle is not asking
        // it to move. The module header says what a press does on a track
        // instead, and why the two are not the same gesture.
      })}
      onPointerMove={composeHandlers(rest.onPointerMove, (event: $FlowFixMe) => {
        if (!dragging.current) {
          return;
        }
        const share = shareAt(event);
        if (share != null) {
          group.setValue(share);
        }
      })}
      onPointerUp={composeHandlers(rest.onPointerUp, endDrag)}
      role="separator"
      // A separator that is not in the tab sequence is the WCAG 2.1.1 failure
      // this module exists to avoid.
      tabIndex={group.disabled ? -1 : 0}
    />
  );
}

/**
 * How far a key moves the splitter, or nothing when the key is not ours.
 *
 * Only the keys along the axis the panes are laid out on: `ArrowUp` in a group
 * of side-by-side panes is the page's, and swallowing it takes a scroll key
 * away from every reader who uses one.
 *
 * The primary pane is the one before the handle, so moving the handle towards
 * the end of the axis makes it larger — and in a right-to-left page the end of
 * a horizontal axis is on the left, so the arrows mirror.
 */
function stepFor(
  key: string,
  step: number,
  orientation: Orientation,
  reversed: boolean,
): number | null {
  const move = step <= 0 ? 1 : step;
  if (orientation === "vertical") {
    return match (key) {
      "ArrowDown" => move,
      "ArrowUp" => -move,
      _ => null,
    };
  }
  const forward = reversed ? -1 : 1;
  return match (key) {
    "ArrowRight" => move * forward,
    "ArrowLeft" => -move * forward,
    _ => null,
  };
}
