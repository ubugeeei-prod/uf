"use flow";
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
//
// # A form, a wheel, and the end of a gesture
//
// A thumb is a `span`. A form never sees it, so `name` renders the hidden
// input `internal/form-value.js` argues for, and the value it carries is the
// thumbs in order, joined by a comma — one number when there is one thumb.
// No `name`, no input.
//
// `onValueChange` fires while the value is moving. `onValueChangeEnd` fires
// when the reader lets go: the pointer comes up, the key comes up, or a wheel
// notch lands. A parent that writes on every move and commits when the gesture
// ends needs both, and the second one is the values the gesture left.
//
// The wheel is the focused thumb's. A wheel over a slider nobody is editing
// still scrolls the page; taking it would trap every reader who merely passed
// the control. Wheel up, and a wheel to the right when there is no vertical
// delta, move toward the maximum. Writing direction does not flip that: a
// wheel is not an arrow.

"use client";

import * as React from "@uniflowed/react";
import { createContext, useContext, useEffect, useRef } from "@uniflowed/react";
import { useEventListener } from "@uniflowed/hooks/dom";
import { useStableCallback } from "@uniflowed/hooks/lifecycle";

import type { RenderProp, Rest } from "./internal/merge-props.js";
import {
  composeHandlers,
  composeRefs,
  withProps,
  withoutComposed,
} from "./internal/merge-props.js";
import type { Orientation } from "./internal/roving-focus.js";
import { clamp, fraction, isReversed, snap } from "./internal/range.js";
import { useControlled } from "./internal/controlled-state.js";
import { useFormReset } from "./internal/checkable-form.js";
import { FormValue } from "./internal/form-value.js";

type SliderState = {|
  readonly values     : $ReadOnlyArray<number>,
  readonly min        : number,
  readonly max        : number,
  readonly step       : number,
  readonly largeStep  : number,
  readonly orientation: Orientation,
  readonly disabled   : boolean,
  readonly valueText  : ((value: number, index: number) => string) | void,
  /** The values the last key left, which can be ahead of `values`. */
  readonly valuesNow: () => $ReadOnlyArray<number>,
  /** One thumb's value from [`valuesNow`]. */
  readonly valueAt: (index: number) => number,
  /** Move one thumb, holding it inside its neighbours. */
  readonly setAt: (index: number, value: number) => void,
  /** The thumb nearest a value, which is the one a press on the track moves. */
  readonly nearest : (value: number) => number,
  readonly trackRef: { current: HTMLElement | null },
  /** The gesture is over. The values are whatever `setAt` left. */
  readonly endGesture: () => void,
  /** Remember which movement key is held, so its `keyup` can end the gesture. */
  readonly noteKey: (key: string) => void,
  /** End that key's gesture. A blur passes no key and ends whichever is held. */
  readonly endKey: (key?: string) => void,
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
component SliderRoot(
  children         : React.Node,
  value?           : $ReadOnlyArray<number>,
  defaultValue?    : $ReadOnlyArray<number> = [0],
  onValueChange?   : (value: $ReadOnlyArray<number>) => void,
  onValueChangeEnd?: (value: $ReadOnlyArray<number>) => void,
  name?            : string,
  min?             : number = 0,
  max?             : number = 100,
  step?            : number = 1,
  largeStep?       : number = 10,
  orientation?     : Orientation = "horizontal",
  disabled?        : boolean = false,
  valueText?       : (value: number, index: number) => string,
  render?          : RenderProp,
  ...rest          : Rest
) {
  const [values, setValues] = useControlled(value, defaultValue, onValueChange);
  const trackRef            = useRef<HTMLElement | null>(null);
  // Ahead of `values` for the length of a batch React has not committed.
  // `useStableCallback` holds the render that installed the handler until its
  // insertion effect, so two arrow keys in one batch both used to read the
  // same number. Written here, and copied back after commit, so a controlled
  // parent that refuses a change wins the next keystroke. ubugeeei-prod/uf#1708
  const live = useRef<$ReadOnlyArray<number>>(values);
  useEffect(() => {
    live.current = values;
  });
  const valuesNow = useStableCallback((): $ReadOnlyArray<number> => live.current);
  const valueAt   = useStableCallback((index: number): number => live.current[index] ?? min);

  const setAt = useStableCallback((index: number, next: number) => {
    if (disabled) {
      return;
    }
    const currentValues = live.current;
    const current = currentValues[index];
    if (current === undefined) {
      return;
    }
    // Bounded by the neighbours rather than by the slider, which is what stops
    // the thumbs being dragged through each other — and it is the same pair of
    // numbers the thumb announces as its own min and max, so what a reader is
    // told and what the control does cannot drift apart.
    const [lower, upper] = boundsOf(currentValues, index, min, max);
    const settled = snap(next, lower, upper, step);
    if (settled === current) {
      return;
    }
    const changed = currentValues.slice();
    changed[index] = settled;
    live.current = changed;
    setValues(changed);
  });

  const endGesture = useStableCallback(() => {
    onValueChangeEnd?.(live.current);
  });
  // The movement key that is down. A held key repeats `keydown` and ends once,
  // on the `keyup` of that same key. Releasing Shift while ArrowRight is still
  // down is not the end of the gesture, and a blur before the key comes up is.
  const movementKey = useRef<string | null>(null);
  const noteKey     = useStableCallback((key: string) => {
    movementKey.current = key;
  });
  const endKey      = useStableCallback((key?: string) => {
    if (movementKey.current == null) {
      return;
    }
    if (key != null && movementKey.current !== key) {
      return;
    }
    movementKey.current = null;
    endGesture();
  });

  const nearest = useStableCallback((target: number): number => {
    const currentValues = live.current;
    let at = 0;
    let best = Infinity;
    for (let index = 0; index < currentValues.length; index += 1) {
      const distance = Math.abs((currentValues[index] ?? 0) - target);
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

  const state = {
    values,
    min,
    max,
    step,
    largeStep,
    orientation,
    disabled,
    valueText,
    valuesNow,
    valueAt,
    setAt,
    nearest,
    trackRef,
    endGesture,
    noteKey,
    endKey,
  };

  const rootRef = useRef<HTMLElement | null>(null);
  // React registers `wheel` as a passive listener, so `preventDefault` in
  // `onWheel` does not stop the page. This listener is not passive, and it
  // cancels only a wheel `onWheel` will apply.
  useEventListener(
    rootRef,
    "wheel",
    (event: Event) => {
      if (thumbWheel(event, disabled, step) == null) {
        return;
      }
      event.preventDefault();
    },
    { passive: false },
  );
  // Same rule as a number field: the browser restores the hidden input to the
  // value it was mounted with, and the thumbs are still wherever the reader
  // left them until this runs.
  const owned = value === undefined;
  const form = typeof rest.form === "string" ? rest.form : undefined;
  const restore = useStableCallback(() => {
    if (owned) {
      live.current = defaultValue;
    }
    setValues(defaultValue);
    if (owned) {
      return;
    }
    const input: $FlowFixMe = rootRef.current?.querySelector?.("input[type='hidden']");
    if (input != null) {
      input.value = values.join(",");
    }
  });
  useFormReset(rootRef, restore, form);
  const passed = withoutComposed(rest, ["onWheel", "ref"]);
  const onWheel = composeHandlers(rest.onWheel, (event: $FlowFixMe) => {
    const move = thumbWheel(event, disabled, step);
    if (move == null) {
      return;
    }
    // The page would scroll as well, and a slider that does both moves the
    // value and the thing the reader was looking at. The native listener above
    // is what actually stops it; this call is a no-op on React's passive one.
    event.preventDefault();
    setAt(move.index, valueAt(move.index) + move.delta);
    endGesture();
  });
  const fields =
    name == null ? null : (
      <FormValue disabled={disabled} form={form} name={name} value={values.join(",")} />
    );
  const props = withProps(passed, {
    children: (
      <>
        {fields}
        {children}
      </>
    ),
    "data-uf-slider": "",
    onWheel,
    // React calls callback refs during commit; the wheel listener reads this later.
    // uf-lint-disable-next-line react-compiler/refs
    ref: composeRefs(rest.ref, (element: HTMLElement | null) => {
      rootRef.current = element;
    }),
  });

  return (
    <SliderContext.Provider value={state}>
      {
        match (render) {
          undefined    => <div {...props} />,
          const custom => custom(props),
        }
      }
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
component SliderTrack(children: React.Node, render?: RenderProp, ...rest: Rest) {
  const slider = useSlider("Slider.Track");
  const passed = withoutComposed(rest, [
    "onPointerDown",
    "onPointerMove",
    "onPointerUp",
    "onPointerCancel",
    "onLostPointerCapture",
    "ref",
  ]);
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

  // `release` is false for `lostpointercapture`: the capture is already gone,
  // and asking for it again would re-enter this function. The drag is cleared
  // first, so the `pointerup` that released the capture and the capture event
  // that follows it end the gesture once.
  const finishDrag = (event: $FlowFixMe, release: boolean) => {
    const started = dragging.current != null;
    dragging.current = null;
    if (release) {
      event.currentTarget?.releasePointerCapture?.(event.pointerId);
    }
    if (started) {
      slider.endGesture();
    }
  };

  const props = withProps(passed, {
    children,
    // Pointer handlers keep the active thumb in a ref between events.
    // uf-lint-disable-next-line react-compiler/refs
    onPointerDown: composeHandlers(rest.onPointerDown, (event: $FlowFixMe) => {
      if (slider.disabled) {
        return;
      }
      // Otherwise the press selects the page's text on the way past, which
      // makes a drag paint everything blue.
      event.preventDefault();
      event.currentTarget?.setPointerCapture?.(event.pointerId);
      moveTo(event, null);
    }),
    // uf-lint-disable-next-line react-compiler/refs
    onPointerMove: composeHandlers(rest.onPointerMove, (event: $FlowFixMe) => {
      if (dragging.current != null) {
        moveTo(event, dragging.current);
      }
    }),
    // uf-lint-disable-next-line react-compiler/refs
    onPointerUp: composeHandlers(rest.onPointerUp, (event: $FlowFixMe) => {
      finishDrag(event, true);
    }),
    // A finger that the browser takes back never sends `pointerup`. Leaving
    // the drag set would let a later move change the value with no press.
    // uf-lint-disable-next-line react-compiler/refs
    onPointerCancel: composeHandlers(rest.onPointerCancel, (event: $FlowFixMe) => {
      finishDrag(event, true);
    }),
    // uf-lint-disable-next-line react-compiler/refs
    onLostPointerCapture: composeHandlers(rest.onLostPointerCapture, (event: $FlowFixMe) => {
      finishDrag(event, false);
    }),
    ref: composeRefs(rest.ref, (element: HTMLElement | null) => {
      // React calls callback refs during commit; pointer handlers read the track later.
      // uf-lint-disable-next-line react-compiler/immutability
      slider.trackRef.current = element;
    }),
  });

  return match (render) {
    undefined    => <div {...props} />,
    const custom => custom(props),
  };
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
component SliderRange(render?: RenderProp, ...rest: Rest) {
  const slider = useSlider("Slider.Range");
  const passed = withoutComposed(rest, ["style"]);
  const ends = [...slider.values].sort((first, second) => first - second);
  const start = slider.values.length > 1 ? (ends[0] ?? slider.min) : slider.min;
  const end = ends[ends.length - 1] ?? slider.min;
  const props = withProps(passed, {
    "aria-hidden": "true",
    style: {
      ...(rest.style as $FlowFixMe),
      "--uf-slider-start": fraction(start, slider.min, slider.max),
      "--uf-slider-end"  : fraction(end, slider.min, slider.max),
    },
  });

  return match (render) {
    undefined    => <div {...props} />,
    const custom => custom(props),
  };
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
component SliderThumb(index?: number = 0, render?: RenderProp, ...rest: Rest) {
  const slider = useSlider("Slider.Thumb");
  const passed = withoutComposed(rest, ["onKeyDown", "onKeyUp", "onBlur", "style"]);
  const value = slider.values[index] ?? slider.min;
  const [lower, upper] = boundsOf(slider.values, index, slider.min, slider.max);
  const props = withProps(passed, {
    "aria-disabled"   : slider.disabled ? "true" : undefined,
    "aria-orientation": slider.orientation,
    // The neighbour's value, not the slider's end. A reader told they may
    // set this thumb to 90 while the control refuses at 60 has been told
    // something the control disagrees with.
    "aria-valuemax"       : upper,
    "aria-valuemin"       : lower,
    "aria-valuenow"       : value,
    "aria-valuetext"      : slider.valueText?.(value, index),
    "data-uf-slider-index": String(index),
    onKeyDown: composeHandlers(rest.onKeyDown, (event: $FlowFixMe) => {
      if (slider.disabled) {
        return;
      }
      const reversed = isReversed(event.currentTarget, slider.orientation);
      const move = stepFor(event.key, slider.step, slider.largeStep, reversed);
      // Inside the handler, not from this render. Two keys can arrive before
      // React commits the first, and both would otherwise step from `value`.
      const [lowerNow, upperNow] = boundsOf(slider.valuesNow(), index, slider.min, slider.max);
      if (move != null) {
        // Before moving: the arrow keys scroll the page, and a slider that
        // moves the page under the reader as it moves the value is a control
        // they cannot watch.
        event.preventDefault();
        slider.noteKey(event.key);
        slider.setAt(index, slider.valueAt(index) + move);
        return;
      }
      if (event.key === "Home" || event.key === "End") {
        event.preventDefault();
        slider.noteKey(event.key);
        // This thumb's own ends, which for the lower thumb of a range is its
        // neighbour rather than the slider's maximum.
        slider.setAt(index, event.key === "Home" ? lowerNow : upperNow);
      }
    }),
    onKeyUp: composeHandlers(rest.onKeyUp, (event: $FlowFixMe) => {
      slider.endKey(event.key);
    }),
    // Focus left before the key came up. Ending here is the gesture; a later
    // keyup of something else must not report it again.
    onBlur: composeHandlers(rest.onBlur, () => {
      slider.endKey();
    }),
    role: "slider",
    style: {
      ...(rest.style as $FlowFixMe),
      "--uf-slider-fraction": fraction(value, slider.min, slider.max),
    },
    // In the tab sequence, and out of it while disabled — the browser does
    // this for a real control and there is no real control here to do it.
    tabIndex: slider.disabled ? -1 : 0,
  });

  return match (render) {
    undefined    => <span {...props} />,
    const custom => custom(props),
  };
}

/**
 * How far a key moves the value, or nothing when the key is not ours.
 *
 * `PageUp` and `PageDown` are never mirrored: they mean "a lot more" and "a
 * lot less", which is not a direction on the page. Neither are `ArrowUp` and
 * `ArrowDown`, since writing direction does not flip the vertical axis.
 */
/**
 * How far one wheel event moves the value, or nothing when it did not move.
 *
 * A vertical delta wins, and wheel up — a negative `deltaY` — heads toward
 * the maximum, which is what a native range does under the pointer. A purely
 * horizontal wheel heads toward the maximum when it goes right. One event is
 * one step: a trackpad that reports a stream of events takes a stream of
 * steps, the same way a held arrow does, and a smaller `step` is how a caller
 * makes that slower.
 */
/**
 * The thumb a wheel should move, and how far, or nothing when it should not.
 *
 * The focused thumb has to belong to this root. A thumb inside a nested slider
 * is inside both roots, and the outer one would otherwise take the same wheel.
 */
function thumbWheel(
  event   : $FlowFixMe,
  disabled: boolean,
  step    : number,
): { index: number, delta: number } | null {
  if (disabled) {
    return null;
  }
  const root: HTMLElement | null = event.currentTarget;
  const active: HTMLElement | null = root?.ownerDocument?.activeElement ?? null;
  if (root == null || active == null || !root.contains(active)) {
    return null;
  }
  if (active.getAttribute("role") !== "slider") {
    return null;
  }
  if (active.closest("[data-uf-slider]") !== root) {
    return null;
  }
  const index = Number(active.getAttribute("data-uf-slider-index"));
  if (!Number.isInteger(index)) {
    return null;
  }
  const delta = wheelStep(event.deltaY, event.deltaX, step);
  if (delta == null) {
    return null;
  }
  return { index, delta };
}

function finiteAxis(value: mixed): number {
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

function wheelStep(deltaY: mixed, deltaX: mixed, step: number): number | null {
  const verticalDelta = finiteAxis(deltaY);
  const horizontalDelta = finiteAxis(deltaX);
  const vertical = verticalDelta !== 0;
  const delta = vertical ? verticalDelta : horizontalDelta;
  if (delta === 0) {
    return null;
  }
  const move = step <= 0 ? 1 : step;
  const towardMax = vertical ? verticalDelta < 0 : horizontalDelta > 0;
  return towardMax ? move : -move;
}

function stepFor(key: string, step: number, largeStep: number, reversed: boolean): number | null {
  const forward = reversed ? -1 : 1;
  const move = step <= 0 ? 1 : step;
  return match (key) {
    "ArrowRight" => move * forward,
    "ArrowLeft"  => -move * forward,
    "ArrowUp"    => move,
    "ArrowDown"  => -move,
    "PageUp"     => largeStep <= 0 ? move : largeStep,
    "PageDown"   => -(largeStep <= 0 ? move : largeStep),
    _            => null,
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
  index : number,
  min   : number,
  max   : number,
): [number, number] {
  const below = index > 0 ? values[index - 1] : undefined;
  const above = index < values.length - 1 ? values[index + 1] : undefined;
  return [below ?? min, above ?? max];
}

/**
 * The parts, under the names the `Slider` namespace gives them.
 *
 * `index.js` re-exports this module whole — `export * as Slider from "./slider.js"` —
 * so a caller writes `<Slider.Root>`, and the namespace is the prefix. Each
 * part is still *declared* as `SliderRoot`, so React DevTools, a component
 * stack and an error name the part a reader can find rather than one of forty
 * `Root`s.
 */
export { SliderRoot as Root, SliderTrack as Track, SliderRange as Range, SliderThumb as Thumb };
