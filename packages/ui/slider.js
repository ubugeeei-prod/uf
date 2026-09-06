// @flow
//
// A slider, and the one decision that makes it reachable at all.
//
// `role="slider"` goes on the **thumb**. Not on the track, not on the wrapper.
// That single placement is the difference between a control a keyboard reader
// can operate and a decorative div, because the element carrying the role is
// the element that carries `tabindex="0"`, and a track with the role is a
// track nobody can focus with a thumb nobody can find.
//
// It follows that a range slider is **two** sliders. Two thumbs are two
// elements with `role="slider"`, each in the tab order, each with its own
// name — "Minimum" and "Maximum" — and each with its own bounds. Announcing
// both as 0–100 while the behaviour stops them passing each other is worse
// than not shipping the range at all: the reader is told they may set the low
// thumb to 90, they try, and the control silently refuses.
//
// So each thumb's `aria-valuemin` and `aria-valuemax` are bounded by its
// neighbour's current value, and they move when the neighbour moves. That is
// what the APG means by "when the range of another slider is dependent on the
// current value of a slider, the values of `aria-valuemin` or `aria-valuemax`
// of the dependent sliders are updated when the value changes".
//
// # `aria-valuetext` is the reason to write this component
//
// `aria-valuenow="3"` is announced as "3". If the scale is Low, Medium, High,
// or a price, or a date, then 3 is not the meaning and the reader is being
// given the implementation. `aria-valuetext="Medium"` is the meaning.
//
// It is a function on the root rather than a string on the thumb, because an
// uncontrolled slider's value is the component's and a caller cannot write
// down a string for a number they have not been told. `valueText(value, index)`
// is called with both, so a range can say "from £20" and "to £60".
//
// # The keyboard
//
//   * `ArrowRight` / `ArrowUp` add a step, `ArrowLeft` / `ArrowDown` subtract
//     one. Both axes work on both orientations, because a reader on a vertical
//     slider still reaches for the horizontal keys about as often as not.
//   * `PageUp` / `PageDown` move by `largeStep`, which is what makes a slider
//     from 0 to 10,000 crossable without holding a key down for a minute.
//   * `Home` and `End` go to that thumb's own ends — which for the lower thumb
//     of a range is its neighbour, not the slider's maximum, so the two
//     announcements and the two behaviours agree.
//   * The horizontal keys mirror in a right-to-left page. `ArrowRight` means
//     "further along", and further along is to the left there;
//     `internal/range.js` reads the direction off the element the key arrived
//     on. The vertical keys and `Home`/`End` are unaffected.
//
// # WCAG 2.5.7, and why the track is a part
//
// *Dragging Movements* says a control operated by dragging needs a way that is
// not a drag. The arrow keys are one; a press on the track is the other, and
// it is the one a pointer reader expects — so `Slider.Track` moves the nearest
// thumb to wherever it was pressed, and the drag that follows is a
// convenience on top of a control that already worked without it.
//
// That is also why the track is a part of this component rather than a `div`
// the caller draws: the press has to be turned into a value, which means
// measuring the track, and a caller who did it themselves would have to
// re-derive the snapping, the clamping and the direction.
//
// # Drawing it
//
// Nothing here has a width, a colour or a position, and an uncontrolled
// slider's value is not the caller's to compute from. So each thumb and the
// range carry the fraction they are at as custom properties —
// `--uf-slider-fraction` on a thumb, `--uf-slider-start` and `--uf-slider-end`
// on the range — and the caller's stylesheet decides what to do with them. A
// caller who writes no CSS sees nothing, which is the same promise every other
// module here makes.

"use client";

import * as React from "@uniflowed/react";
import { createContext, useContext, useMemo, useRef } from "@uniflowed/react";
import { useStableCallback } from "@uniflowed/hooks/lifecycle";

import type { Rest } from "./internal/merge-props.js";
import { composeHandlers, composeRefs, withoutComposed } from "./internal/merge-props.js";
import type { Orientation } from "./internal/roving-focus.js";
import { clamp, fraction, isReversed, snap } from "./internal/range.js";
import { useControlled } from "./internal/controlled-state.js";

type SliderState = {|
  readonly values: $ReadOnlyArray<number>,
  readonly min: number,
  readonly max: number,
  readonly step: number,
  readonly largeStep: number,
  readonly orientation: Orientation,
  readonly disabled: boolean,
  readonly valueText: ((value: number, index: number) => string) | void,
  /** Move one thumb, holding it inside its neighbours. */
  readonly setAt: (index: number, value: number) => void,
  /** The thumb nearest a value, which is the one a press on the track moves. */
  readonly nearest: (value: number) => number,
  readonly trackRef: { current: HTMLElement | null },
|};

const SliderContext: React.Context<SliderState | null> = createContext(null);

hook useSlider(part: string): SliderState {
  const state = useContext(SliderContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside a Slider.Root`);
  }
  return state;
}

/**
 * The slider.
 *
 * `value` is always an array, with one entry per thumb, because a range slider
 * is not a different component from a single one — it is the same component
 * with two thumbs, and a `number | [number, number]` prop would make every
 * caller narrow a union to read their own value back.
 */
export component SliderRoot(
  children: React.Node,
  value?: $ReadOnlyArray<number>,
  defaultValue?: $ReadOnlyArray<number> = [0],
  onValueChange?: (value: $ReadOnlyArray<number>) => void,
  min?: number = 0,
  max?: number = 100,
  step?: number = 1,
  largeStep?: number = 10,
  orientation?: Orientation = "horizontal",
  disabled?: boolean = false,
  valueText?: (value: number, index: number) => string,
  ...rest: Rest
) {
  const [values, setValues] = useControlled(value, defaultValue, onValueChange);
  const trackRef = useRef<HTMLElement | null>(null);

  const setAt = useStableCallback((index: number, next: number) => {
    if (disabled) {
      return;
    }
    const current = values[index];
    if (current === undefined) {
      return;
    }
    // Bounded by the neighbours rather than by the slider, which is what stops
    // the thumbs being dragged through each other — and it is the same pair of
    // numbers the thumb announces as its own min and max, so what a reader is
    // told and what the control does cannot drift apart.
    const [lower, upper] = boundsOf(values, index, min, max);
    const settled = snap(next, lower, upper, step);
    if (settled === current) {
      return;
    }
    const changed = values.slice();
    changed[index] = settled;
    setValues(changed);
  });

  const nearest = useStableCallback((target: number): number => {
    let at = 0;
    let best = Infinity;
    for (let index = 0; index < values.length; index += 1) {
      const distance = Math.abs((values[index] ?? 0) - target);
      // Strictly nearer, so a press exactly between two thumbs takes the first
      // one rather than the last — arbitrary either way, and consistent is
      // what stops it feeling random.
      if (distance < best) {
        best = distance;
        at = index;
      }
    }
    return at;
  });

  const state = useMemo(
    () => ({
      values,
      min,
      max,
      step,
      largeStep,
      orientation,
      disabled,
      valueText,
      setAt,
      nearest,
      trackRef,
    }),
    [values, min, max, step, largeStep, orientation, disabled, valueText, setAt, nearest],
  );

  return (
    <SliderContext.Provider value={state}>
      <div {...rest}>{children}</div>
    </SliderContext.Provider>
  );
}

/**
 * The bar the thumbs sit on, and the half of WCAG 2.5.7 that is not a key.
 *
 * A press anywhere on it moves the nearest thumb there, and holding the
 * pointer down drags that thumb. The pointer is captured, so a drag that
 * wanders off the track — which every drag does — keeps arriving here instead
 * of being lost to whatever it wandered over.
 */
export component SliderTrack(children: React.Node, ...rest: Rest) {
  const slider = useSlider("Slider.Track");
  const passed = withoutComposed(rest, ["onPointerDown", "onPointerMove", "onPointerUp", "ref"]);
  const dragging = useRef<number | null>(null);

  /** The value the pointer is over, from the track's own box. */
  const valueAt = (event: $FlowFixMe): number | null => {
    const track = slider.trackRef.current;
    if (track == null) {
      return null;
    }
    const box = track.getBoundingClientRect();
    const vertical = slider.orientation === "vertical";
    const size = vertical ? box.height : box.width;
    if (size <= 0) {
      return null;
    }
    const along = vertical ? box.bottom - event.clientY : event.clientX - box.left;
    // A vertical slider's minimum is at the *bottom*, which is why the reading
    // above is taken from `bottom` rather than `top`: a slider that grows
    // downwards is the one arrangement no reader expects.
    const part = clamp(along / size, 0, 1);
    const forward = isReversed(track, slider.orientation) ? 1 - part : part;
    return slider.min + forward * (slider.max - slider.min);
  };

  const moveTo = (event: $FlowFixMe, index: number | null) => {
    const target = valueAt(event);
    if (target == null) {
      return;
    }
    const at = index ?? slider.nearest(target);
    dragging.current = at;
    slider.setAt(at, target);
  };

  return (
    <div
      {...passed}
      onPointerDown={composeHandlers(rest.onPointerDown, (event: $FlowFixMe) => {
        if (slider.disabled) {
          return;
        }
        // Otherwise the press selects the page's text on the way past, which
        // makes a drag paint everything blue.
        event.preventDefault();
        event.currentTarget?.setPointerCapture?.(event.pointerId);
        moveTo(event, null);
      })}
      onPointerMove={composeHandlers(rest.onPointerMove, (event: $FlowFixMe) => {
        if (dragging.current != null) {
          moveTo(event, dragging.current);
        }
      })}
      onPointerUp={composeHandlers(rest.onPointerUp, (event: $FlowFixMe) => {
        dragging.current = null;
        event.currentTarget?.releasePointerCapture?.(event.pointerId);
      })}
      ref={composeRefs(rest.ref, (element) => {
        slider.trackRef.current = element;
      })}
    >
      {children}
    </div>
  );
}

/**
 * The filled part of the track.
 *
 * From the lowest thumb to the highest, which for a single thumb is from the
 * slider's minimum to that thumb — the difference between a volume control and
 * a price range, expressed by how many thumbs there are rather than by a prop.
 *
 * Presentational: it is inside the track and carries no role, because a reader
 * is told the value by the thumb and telling them again here would be telling
 * them twice.
 */
export component SliderRange(...rest: Rest) {
  const slider = useSlider("Slider.Range");
  const passed = withoutComposed(rest, ["style"]);
  const ends = [...slider.values].sort((first, second) => first - second);
  const start = slider.values.length > 1 ? (ends[0] ?? slider.min) : slider.min;
  const end = ends[ends.length - 1] ?? slider.min;

  return (
    <div
      {...passed}
      aria-hidden="true"
      style={{
        ...(rest.style as $FlowFixMe),
        "--uf-slider-start": fraction(start, slider.min, slider.max),
        "--uf-slider-end": fraction(end, slider.min, slider.max),
      }}
    />
  );
}

/**
 * One thumb, which is the slider as far as a screen reader is concerned.
 *
 * `index` is which of the root's values this thumb owns, and it defaults to
 * zero so a one-thumb slider never mentions it. It is a prop rather than
 * something counted from the document, because the tab order of a range slider
 * has to stay put while the thumbs move — the APG is explicit that a thumb
 * passing another does not reorder them — and a position counted from the page
 * is a position that changes when the page does.
 *
 * A name is the caller's, and for a range it is two names: "Minimum" and
 * "Maximum" told apart is the whole difference between a control a reader can
 * operate and two identical "slider"s.
 */
export component SliderThumb(index?: number = 0, ...rest: Rest) {
  const slider = useSlider("Slider.Thumb");
  const passed = withoutComposed(rest, ["onKeyDown", "style"]);
  const value = slider.values[index] ?? slider.min;
  const [lower, upper] = boundsOf(slider.values, index, slider.min, slider.max);

  return (
    <span
      {...passed}
      aria-disabled={slider.disabled ? "true" : undefined}
      aria-orientation={slider.orientation}
      // The neighbour's value, not the slider's end. A reader told they may
      // set this thumb to 90 while the control refuses at 60 has been told
      // something the control disagrees with.
      aria-valuemax={upper}
      aria-valuemin={lower}
      aria-valuenow={value}
      aria-valuetext={slider.valueText?.(value, index)}
      onKeyDown={composeHandlers(rest.onKeyDown, (event: $FlowFixMe) => {
        if (slider.disabled) {
          return;
        }
        const reversed = isReversed(event.currentTarget, slider.orientation);
        const move = stepFor(event.key, slider.step, slider.largeStep, reversed);
        if (move != null) {
          // Before moving: the arrow keys scroll the page, and a slider that
          // moves the page under the reader as it moves the value is a control
          // they cannot watch.
          event.preventDefault();
          slider.setAt(index, value + move);
          return;
        }
        if (event.key === "Home" || event.key === "End") {
          event.preventDefault();
          // This thumb's own ends, which for the lower thumb of a range is its
          // neighbour rather than the slider's maximum.
          slider.setAt(index, event.key === "Home" ? lower : upper);
        }
      })}
      role="slider"
      style={{
        ...(rest.style as $FlowFixMe),
        "--uf-slider-fraction": fraction(value, slider.min, slider.max),
      }}
      // In the tab sequence, and out of it while disabled — the browser does
      // this for a real control and there is no real control here to do it.
      tabIndex={slider.disabled ? -1 : 0}
    />
  );
}

/**
 * How far a key moves the value, or nothing when the key is not ours.
 *
 * `PageUp` and `PageDown` are never mirrored: they mean "a lot more" and "a
 * lot less", which is not a direction on the page. Neither are `ArrowUp` and
 * `ArrowDown`, since writing direction does not flip the vertical axis.
 */
function stepFor(key: string, step: number, largeStep: number, reversed: boolean): number | null {
  const forward = reversed ? -1 : 1;
  const move = step <= 0 ? 1 : step;
  return match (key) {
    "ArrowRight" => move * forward,
    "ArrowLeft" => -move * forward,
    "ArrowUp" => move,
    "ArrowDown" => -move,
    "PageUp" => largeStep <= 0 ? move : largeStep,
    "PageDown" => -(largeStep <= 0 ? move : largeStep),
    _ => null,
  };
}

/**
 * What a thumb may be set to: its neighbours' values, or the slider's ends.
 *
 * One function, called by the thumb to announce its bounds and by the root to
 * enforce them, so the two cannot disagree.
 */
function boundsOf(
  values: $ReadOnlyArray<number>,
  index: number,
  min: number,
  max: number,
): [number, number] {
  const below = index > 0 ? values[index - 1] : undefined;
  const above = index < values.length - 1 ? values[index + 1] : undefined;
  return [below ?? min, above ?? max];
}
