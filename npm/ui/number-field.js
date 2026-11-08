"use flow";
"use client";

import * as React from "@uniflowed/react";
import { createContext, useContext, useEffect, useRef, useState } from "@uniflowed/react";
import { useEventListener } from "@uniflowed/hooks/dom";
import { useStableCallback } from "@uniflowed/hooks/lifecycle";
import { useControlled } from "./internal/controlled-state.js";
import { useFormReset } from "./internal/checkable-form.js";
import type { RenderProp, Rest } from "./internal/merge-props.js";
import {
  composeHandlers,
  composeRefs,
  withProps,
  withoutComposed,
} from "./internal/merge-props.js";
import { useLocale } from "./i18n-provider.js";
import { FormValue } from "./internal/form-value.js";

/**
 * How long a held stepper waits before it repeats.
 *
 * Longer than a click, so a press that comes up immediately steps once. Short
 * enough that holding still feels like holding.
 */
const HOLD_BEFORE_REPEAT_MS = 400;
/** How often a held stepper repeats after `HOLD_BEFORE_REPEAT_MS`. */
const HOLD_REPEAT_MS = 60;

type NumberFormatter = {
  format         : (value: number) => string,
  formatToParts  : (value: number) => Array<{ type: string, value: string }>,
  resolvedOptions: () => { locale: string, style: string, ... },
};
export type NumberFormatOptions = {
  style?                : "decimal" | "percent" | "currency" | "unit",
  currency?             : string,
  currencyDisplay?      : "symbol" | "narrowSymbol" | "code" | "name",
  currencySign?         : "standard" | "accounting",
  unit?                 : string,
  unitDisplay?          : "long" | "short" | "narrow",
  useGrouping?          : boolean,
  minimumFractionDigits?: number,
  maximumFractionDigits?: number,
  minimumIntegerDigits? : number,
  numberingSystem?      : string,
};
// Flow's bundled Intl declarations predate formatToParts and unit formatting.
function numberFormatter(locale: string, options?: NumberFormatOptions): NumberFormatter {
  return new Intl.NumberFormat(locale, { maximumFractionDigits: 20, ...options }) as $FlowFixMe;
}

/** Parse only the symbols the formatter emits, never arbitrary trailing text. */
export function parseNumber(text: string, formatter: NumberFormatter): number | null {
  if (text.trim() === "") return null;
  const options = formatter.resolvedOptions();
  const digits = new Intl.NumberFormat(options.locale, { useGrouping: false });
  let clean = text.trim();
  for (let digit = 0; digit < 10; digit += 1)
    clean = clean.split(digits.format(digit)).join(String(digit));
  const parts = formatter.formatToParts(-12345.6);
  const decimal =
    numberFormatter(options.locale)
      .formatToParts(1.1)
      .find((part) => part.type === "decimal")?.value ?? ".";
  const accounting = clean.startsWith("(") && clean.endsWith(")");
  if (accounting) clean = clean.slice(1, -1);
  for (const part of parts) {
    if (["group", "currency", "unit", "percentSign", "literal"].includes(part.type))
      clean = clean.split(part.value).join("");
    if (part.type === "minusSign") clean = clean.split(part.value).join("-");
  }
  clean = clean
    .split(decimal)
    .join(".")
    .replace(/[\s\u200e\u200f\u061c]/g, "");
  if (!/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)$/.test(clean)) return NaN;
  const value = (Number(clean) * (accounting ? -1 : 1)) / (options.style === "percent" ? 100 : 1);
  return Number.isFinite(value) ? value : NaN;
}

type NumberState = {
  value    : number | null,
  text     : string,
  invalid  : boolean,
  disabled : boolean,
  readOnly : boolean,
  min      : number | void,
  max      : number | void,
  formatter: NumberFormatter,
  edit     : (text: string) => void,
  commit   : () => void,
  stepBy   : (direction: number) => void,
  boundary : (value: number | void) => void,
  /** The value a gesture left, after `stepBy` or `commit` has stored it. */
  finish: () => void,
  /** Step once and keep stepping while the pointer stays down. */
  hold: (direction: number) => void,
  /** The pointer came up. Stop repeating and finish the gesture. */
  release: () => void,
};
const NumberContext: React.Context<NumberState | null> = createContext(null);
hook useNumber(): NumberState {
  const state = useContext(NumberContext);
  if (state == null) throw new Error("NumberField parts must be inside NumberField.Root");
  return state;
}

component NumberFieldRoot(
  children           : React.Node,
  value?             : number | null,
  defaultValue?      : number | null = null,
  onValueChange?     : (value: number | null) => void,
  onValueChangeEnd?  : (value: number | null) => void,
  onValidationChange?: (invalid: boolean) => void,
  name?              : string,
  min?               : number,
  max?               : number,
  step?              : number,
  formatOptions?     : NumberFormatOptions,
  disabled?          : boolean = false,
  readOnly?          : boolean = false,
  render?            : RenderProp,
  ...rest            : Rest
) {
  const increment = step ?? (formatOptions?.style === "percent" ? 0.01 : 1);
  if (
    !Number.isFinite(increment) ||
    increment <= 0 ||
    (min != null && !Number.isFinite(min)) ||
    (max != null && !Number.isFinite(max)) ||
    (min != null && max != null && min > max)
  ) {
    throw new RangeError("NumberField requires finite bounds and a positive increment");
  }
  const upperBound =
    max == null
      ? undefined
      : Number(
          ((min ?? 0) + Math.floor((max - (min ?? 0)) / increment + 1e-9) * increment).toPrecision(
            15,
          ),
        );
  const { locale } = useLocale();
  const formatter = numberFormatter(locale, formatOptions);
  const [current, setCurrent] = useControlled(value, defaultValue, onValueChange);
  const [draft,   setDraft]   = useState<string | null>(null);
  // The text as the last event handler left it, which can be ahead of the
  // `draft` and `current` this render read: two keystrokes can land before React
  // renders the first, and a handler installed by `useStableCallback` sees the
  // values of the render that installed it. Without this, a second `ArrowUp`
  // stepped from the same number as the first, and `Enter` committed the value
  // from before either of them — `internal/segmented-field.js` holds a ref for
  // the same window (#1609); this is #1614.
  //
  // Cleared after every commit rather than only when a value is stored, so a
  // controlled parent that refuses a change still wins the next keystroke: the
  // ref only ever spans a batch React has not rendered yet.
  const proposed = useRef<string | null>(null);
  const rootRef  = useRef<HTMLElement | null>(null);
  // The number `assign` stored, ahead of the render a controlled parent may
  // then refuse. The effect copies `current` back, so the refusal wins the
  // next gesture the same way `proposed` does for text.
  const committed = useRef<number | null>(current);
  useEffect(() => {
    proposed.current = null;
    committed.current = current;
  });
  const assess = (source: string) => {
    const parsed = parseNumber(source, formatter);
    const invalid =
      parsed != null &&
      (!Number.isFinite(parsed) ||
        (min != null && parsed < min) ||
        (max != null && parsed > max) ||
        Math.abs(
          (parsed - (min ?? 0)) / increment - Math.round((parsed - (min ?? 0)) / increment),
        ) > 1e-7);
    return { parsed, invalid };
  };
  const text = draft ?? (current == null ? "" : formatter.format(current));
  const { invalid } = assess(text);
  // Falls back to `text`, not to `current`: a draft the reader typed and has not
  // committed is real state, and once the effect above has cleared `proposed` it
  // is the only record of it. Reading `current` here committed the old value and
  // threw away an invalid draft the field was meant to keep for correction.
  const latest      = useStableCallback((): string => proposed.current ?? text);
  const assign      = useStableCallback((next: number | null) => {
    if (disabled || readOnly) return;
    proposed.current = next == null ? "" : formatter.format(next);
    committed.current = next;
    setDraft(null);
    setCurrent(next);
    onValidationChange?.(false);
  });
  const finish      = useStableCallback(() => {
    onValueChangeEnd?.(committed.current);
  });
  const commit      = useStableCallback(() => {
    if (disabled || readOnly) return;
    const { parsed, invalid } = assess(latest());
    onValidationChange?.(invalid);
    if (!invalid) assign(parsed);
    // An invalid draft stays on screen and the value does not move, so the
    // gesture ends on the number the field already held.
    finish();
  });
  const stepBy      = useStableCallback((direction: number) => {
    const { parsed } = assess(latest());
    const start = parsed != null && Number.isFinite(parsed) ? parsed : (current ?? min ?? 0);
    const base = min ?? 0;
    const position = (start - base) / increment;
    const index =
      direction > 0
        ? Math.floor(position + 1e-9) + direction
        : Math.ceil(position - 1e-9) + direction;
    const next = Number((base + index * increment).toPrecision(15));
    const ceiling =
      max == null
        ? Infinity
        : Number((base + Math.floor((max - base) / increment + 1e-9) * increment).toPrecision(15));
    assign(Math.max(min ?? -Infinity, Math.min(ceiling, next)));
  });
  const repeatTimer = useRef<TimeoutID | null>(null);
  const holding     = useRef<boolean>(false);
  const stopRepeat = () => {
    if (repeatTimer.current != null) {
      clearTimeout(repeatTimer.current);
      repeatTimer.current = null;
    }
  };
  useEffect(
    () => () => {
      stopRepeat();
    },
    [],
  );
  const hold    = useStableCallback((direction: number) => {
    if (disabled || readOnly) return;
    stopRepeat();
    holding.current = true;
    stepBy(direction);
    const tick = () => {
      stepBy(direction);
      repeatTimer.current = setTimeout(tick, HOLD_REPEAT_MS);
    };
    repeatTimer.current = setTimeout(tick, HOLD_BEFORE_REPEAT_MS);
  });
  const release = useStableCallback(() => {
    if (!holding.current) return;
    holding.current = false;
    stopRepeat();
    finish();
  });
  const state = {
    value: current,
    text,
    invalid,
    disabled,
    readOnly,
    min,
    max: upperBound,
    formatter,
    edit: (next: string) => {
      if (disabled || readOnly) return;
      proposed.current = next;
      setDraft(next);
    },
    commit,
    stepBy,
    finish,
    hold,
    release,
    boundary: (next: number | void) => {
      if (next != null)
        assign(
          Number(
            (
              (min ?? 0) +
              Math.floor((next - (min ?? 0)) / increment + 1e-9) * increment
            ).toPrecision(15),
          ),
        );
    },
  };
  // A form reset puts both inputs back to the value they were mounted with.
  // That is not `defaultValue` once the reader has stepped. The microtask is
  // `useFormReset`'s: happy-dom resets before the event, a browser resets
  // after the listener, and either way the microtask is afterwards.
  const owned = value === undefined;
  const form = typeof rest.form === "string" ? rest.form : undefined;
  const restore = useStableCallback(() => {
    proposed.current = null;
    setDraft(null);
    setCurrent(defaultValue);
    if (owned) {
      return;
    }
    // The parent was told. Until it renders, both fields would show whatever
    // the browser just restored, which is not the number still on screen.
    const root: $FlowFixMe = rootRef.current;
    const hidden = root?.querySelector?.("input[type='hidden']");
    if (hidden != null) {
      hidden.value = current == null ? "" : String(current);
    }
    const spin = root?.querySelector?.("[role='spinbutton']");
    if (spin != null) {
      spin.value = text;
    }
  });
  useFormReset(rootRef, restore, form);
  const passed = withoutComposed(rest, ["ref"]);
  const props = withProps(passed, {
    children: (
      <>
        {name == null ? null : (
          <FormValue
            disabled={disabled}
            form={form}
            name={name}
            value={current == null ? "" : String(current)}
          />
        )}
        {children}
      </>
    ),
    "data-invalid": invalid ? "" : undefined,
    // React calls callback refs during commit; the reset listener reads this later.
    // uf-lint-disable-next-line react-compiler/refs
    ref: composeRefs(rest.ref, (element: HTMLElement | null) => {
      rootRef.current = element;
    }),
  });
  return (
    <NumberContext.Provider value={state}>
      {
        match (render) {
          undefined    => <div {...props} />,
          const custom => custom(props),
        }
      }
    </NumberContext.Provider>
  );
}

/** Field.Control may render this input to supply the label, description and error ids. */
component NumberFieldInput(render?: RenderProp, ...rest: Rest) {
  const state = useNumber();
  // Set while an arrow, page, Home or End key is down, so the matching
  // `keyup` ends the gesture once even when the key repeated.
  const spinning = useRef<boolean>(false);
  const inputRef = useRef<HTMLElement | null>(null);
  // React's own wheel listener is passive, so `preventDefault` inside `onWheel`
  // does not stop the page. This one is not passive, and it only cancels the
  // wheel the handler below will step.
  useEventListener(
    inputRef,
    "wheel",
    (event: Event) => {
      if (focusedWheelDirection(event, state.disabled, state.readOnly) == null) {
        return;
      }
      event.preventDefault();
    },
    { passive: false },
  );
  const passed = withoutComposed(rest, ["ref"]);
  const props = withProps(passed, {
    type: "text",
    role: "spinbutton",
    inputMode: "decimal",
    value: state.text,
    disabled: state.disabled,
    readOnly: state.readOnly,
    "aria-valuenow": state.value == null || !Number.isFinite(state.value) ? undefined : state.value,
    "aria-valuetext": state.text || undefined,
    "aria-valuemin": state.min,
    "aria-valuemax": state.max,
    "aria-invalid": state.invalid ? "true" : rest["aria-invalid"],
    onChange: composeHandlers(rest.onChange, (event: $FlowFixMe) =>
      state.edit(event.currentTarget.value),
    ),
    // Blur commits, and it is finished. A later keyup must not end it again.
    // uf-lint-disable-next-line react-compiler/refs
    onBlur: composeHandlers(rest.onBlur, () => {
      spinning.current = false;
      state.commit();
    }),
    // `spinning` is set on keydown and read on keyup, which is not this render.
    // uf-lint-disable-next-line react-compiler/refs
    onKeyDown: composeHandlers(rest.onKeyDown, (event: $FlowFixMe) => {
      if (state.disabled || state.readOnly) return;
      const key = event.key;
      if (["ArrowUp", "ArrowDown", "PageUp", "PageDown", "Home", "End", "Enter"].includes(key)) {
        event.preventDefault();
        if (key === "Enter") {
          state.commit();
          return;
        }
        spinning.current = true;
        if (key === "Home") state.boundary(state.min);
        else if (key === "End") state.boundary(state.max);
        else
          state.stepBy(
            key === "ArrowUp" ? 1 : key === "ArrowDown" ? -1 : key === "PageUp" ? 10 : -10,
          );
      }
    }),
    // uf-lint-disable-next-line react-compiler/refs
    onKeyUp: composeHandlers(rest.onKeyUp, (event: $FlowFixMe) => {
      if (!spinning.current) return;
      spinning.current = false;
      if (event.key === "Enter") return;
      state.finish();
    }),
    onWheel: composeHandlers(rest.onWheel, (event: $FlowFixMe) => {
      const direction = focusedWheelDirection(event, state.disabled, state.readOnly);
      if (direction == null) return;
      event.preventDefault();
      state.stepBy(direction);
      state.finish();
    }),
    // React calls callback refs during commit; the wheel listener reads this later.
    // uf-lint-disable-next-line react-compiler/refs
    ref: composeRefs(rest.ref, (element: HTMLElement | null) => {
      inputRef.current = element;
    }),
  });
  return match (render) {
    undefined    => <input {...props} />,
    const custom => custom(props),
  };
}

component NumberFieldIncrement(children?: React.Node = "+", render?: RenderProp, ...rest: Rest) {
  const state = useNumber();
  // Set on pointer down and still set when the click that follows it arrives,
  // so that click does not step a second time. Cleared after that click.
  const fromPointer = useRef<boolean>(false);
  // The repeat timer is the root's. Unmounting the button while it is held
  // has to release that hold, or the timer keeps stepping and the gesture
  // never reports that it ended.
  useEffect(
    () => () => {
      state.release();
    },
    [state.release],
  );
  const props = withProps(rest, {
    children,
    type        : "button",
    "aria-label": rest["aria-label"] ?? "Increase",
    disabled:
      state.disabled ||
      state.readOnly ||
      (state.max != null && state.value != null && state.value >= state.max),
    // `fromPointer` is set on pointer down and still set when the click that
    // follows it arrives, so that click does not step a second time.
    // uf-lint-disable-next-line react-compiler/refs
    onPointerDown: composeHandlers(rest.onPointerDown, (event: $FlowFixMe) => {
      if (event.button != null && event.button !== 0) return;
      fromPointer.current = true;
      state.hold(1);
    }),
    // uf-lint-disable-next-line react-compiler/refs
    onPointerUp: composeHandlers(rest.onPointerUp, () => {
      state.release();
      // The click is dispatched before the next microtask, and still needs to
      // see this. A pointer that never produces a click must not swallow the
      // keyboard click that comes later.
      queueMicrotask(() => {
        fromPointer.current = false;
      });
    }),
    // uf-lint-disable-next-line react-compiler/refs
    onPointerLeave: composeHandlers(rest.onPointerLeave, () => {
      state.release();
      queueMicrotask(() => {
        fromPointer.current = false;
      });
    }),
    // uf-lint-disable-next-line react-compiler/refs
    onPointerCancel: composeHandlers(rest.onPointerCancel, () => {
      state.release();
      queueMicrotask(() => {
        fromPointer.current = false;
      });
    }),
    // uf-lint-disable-next-line react-compiler/refs
    onClick: composeHandlers(rest.onClick, () => {
      if (fromPointer.current) {
        fromPointer.current = false;
        return;
      }
      state.stepBy(1);
      state.finish();
    }),
  });
  return match (render) {
    undefined    => <button {...props} />,
    const custom => custom(props),
  };
}
component NumberFieldDecrement(children?: React.Node = "−", render?: RenderProp, ...rest: Rest) {
  const state       = useNumber();
  const fromPointer = useRef<boolean>(false);
  useEffect(
    () => () => {
      state.release();
    },
    [state.release],
  );
  const props = withProps(rest, {
    children,
    type        : "button",
    "aria-label": rest["aria-label"] ?? "Decrease",
    disabled:
      state.disabled ||
      state.readOnly ||
      (state.min != null && state.value != null && state.value <= state.min),
    // Same as Increment: the pointer's click must not step again.
    // uf-lint-disable-next-line react-compiler/refs
    onPointerDown: composeHandlers(rest.onPointerDown, (event: $FlowFixMe) => {
      if (event.button != null && event.button !== 0) return;
      fromPointer.current = true;
      state.hold(-1);
    }),
    // uf-lint-disable-next-line react-compiler/refs
    onPointerUp: composeHandlers(rest.onPointerUp, () => {
      state.release();
      queueMicrotask(() => {
        fromPointer.current = false;
      });
    }),
    // uf-lint-disable-next-line react-compiler/refs
    onPointerLeave: composeHandlers(rest.onPointerLeave, () => {
      state.release();
      queueMicrotask(() => {
        fromPointer.current = false;
      });
    }),
    // uf-lint-disable-next-line react-compiler/refs
    onPointerCancel: composeHandlers(rest.onPointerCancel, () => {
      state.release();
      queueMicrotask(() => {
        fromPointer.current = false;
      });
    }),
    // uf-lint-disable-next-line react-compiler/refs
    onClick: composeHandlers(rest.onClick, () => {
      if (fromPointer.current) {
        fromPointer.current = false;
        return;
      }
      state.stepBy(-1);
      state.finish();
    }),
  });
  return match (render) {
    undefined    => <button {...props} />,
    const custom => custom(props),
  };
}

/**
 * The parts, under the names the `NumberField` namespace gives them.
 *
 * `index.js` re-exports this module whole — `export * as NumberField from "./number-field.js"` —
 * so a caller writes `<NumberField.Root>`, and the namespace is the prefix. Each
 * part is still *declared* as `NumberFieldRoot`, so React DevTools, a component
 * stack and an error name the part a reader can find rather than one of forty
 * `Root`s.
 */
/**
 * Which way a wheel over the focused field steps, or nothing when it does not.
 *
 * A wheel over a field the reader is not editing still belongs to the page.
 * Taking it would trap the scroll on the way past. The native listener and
 * `onWheel` both ask this, so one of them cannot start cancelling wheels the
 * other will not step.
 */
function focusedWheelDirection(
  event   : $FlowFixMe,
  disabled: boolean,
  readOnly: boolean,
): number | null {
  if (disabled || readOnly) return null;
  const node = event.currentTarget;
  if (node == null || node.ownerDocument?.activeElement !== node) return null;
  return wheelDirection(event.deltaY, event.deltaX);
}

/**
 * Which way one wheel event steps, or nothing when it did not move.
 *
 * Wheel up — a negative `deltaY` — increases, as it does on a native number
 * input. A purely horizontal wheel increases when it goes right. One event is
 * one step.
 */
function wheelDirection(deltaY: mixed, deltaX: mixed): number | null {
  const vertical = typeof deltaY === "number" && Number.isFinite(deltaY) ? deltaY : 0;
  const horizontal = typeof deltaX === "number" && Number.isFinite(deltaX) ? deltaX : 0;
  if (vertical !== 0) return vertical < 0 ? 1 : -1;
  if (horizontal !== 0) return horizontal > 0 ? 1 : -1;
  return null;
}

export {
  NumberFieldRoot as Root,
  NumberFieldInput as Input,
  NumberFieldIncrement as Increment,
  NumberFieldDecrement as Decrement,
};
