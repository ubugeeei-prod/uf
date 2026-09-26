// @flow
"use client";

import * as React from "@uniflowed/react";
import { createContext, useContext, useEffect, useMemo, useRef, useState } from "@uniflowed/react";
import { useStableCallback } from "@uniflowed/hooks/lifecycle";
import { useControlled } from "./internal/controlled-state.js";
import type { RenderProp, Rest } from "./internal/merge-props.js";
import { composeHandlers, withProps } from "./internal/merge-props.js";
import { useLocale } from "./i18n-provider.js";

type NumberFormatter = {
  format: (value: number) => string,
  formatToParts: (value: number) => Array<{ type: string, value: string }>,
  resolvedOptions: () => { locale: string, style: string, ... },
};
export type NumberFormatOptions = {
  style?: "decimal" | "percent" | "currency" | "unit",
  currency?: string,
  currencyDisplay?: "symbol" | "narrowSymbol" | "code" | "name",
  currencySign?: "standard" | "accounting",
  unit?: string,
  unitDisplay?: "long" | "short" | "narrow",
  useGrouping?: boolean,
  minimumFractionDigits?: number,
  maximumFractionDigits?: number,
  minimumIntegerDigits?: number,
  numberingSystem?: string,
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
  value: number | null,
  text: string,
  invalid: boolean,
  disabled: boolean,
  readOnly: boolean,
  min: number | void,
  max: number | void,
  formatter: NumberFormatter,
  edit: (text: string) => void,
  commit: () => void,
  stepBy: (direction: number) => void,
  boundary: (value: number | void) => void,
};
const NumberContext: React.Context<NumberState | null> = createContext(null);
hook useNumber(): NumberState {
  const state = useContext(NumberContext);
  if (state == null) throw new Error("NumberField parts must be inside NumberField.Root");
  return state;
}

component NumberFieldRoot(
  children: React.Node,
  value?: number | null,
  defaultValue?: number | null = null,
  onValueChange?: (value: number | null) => void,
  onValidationChange?: (invalid: boolean) => void,
  min?: number,
  max?: number,
  step?: number,
  formatOptions?: NumberFormatOptions,
  disabled?: boolean = false,
  readOnly?: boolean = false,
  render?: RenderProp,
  ...rest: Rest
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
  const formatter = useMemo(() => numberFormatter(locale, formatOptions), [locale, formatOptions]);
  const [current, setCurrent] = useControlled(value, defaultValue, onValueChange);
  const [draft, setDraft] = useState<string | null>(null);
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
  useEffect(() => {
    proposed.current = null;
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
  const latest = useStableCallback((): string => proposed.current ?? text);
  const assign = useStableCallback((next: number | null) => {
    if (disabled || readOnly) return;
    proposed.current = next == null ? "" : formatter.format(next);
    setDraft(null);
    setCurrent(next);
    onValidationChange?.(false);
  });
  const commit = useStableCallback(() => {
    if (disabled || readOnly) return;
    const { parsed, invalid } = assess(latest());
    onValidationChange?.(invalid);
    if (!invalid) assign(parsed);
  });
  const stepBy = useStableCallback((direction: number) => {
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
  const props = withProps(rest, { children, "data-invalid": invalid ? "" : undefined });
  return (
    <NumberContext.Provider value={state}>
      {render != null ? render(props) : <div {...props} />}
    </NumberContext.Provider>
  );
}

/** Field.Control may render this input to supply the label, description and error ids. */
component NumberFieldInput(render?: RenderProp, ...rest: Rest) {
  const state = useNumber();
  const props = withProps(rest, {
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
    onBlur: composeHandlers(rest.onBlur, state.commit),
    onKeyDown: composeHandlers(rest.onKeyDown, (event: $FlowFixMe) => {
      if (state.disabled || state.readOnly) return;
      const key = event.key;
      if (["ArrowUp", "ArrowDown", "PageUp", "PageDown", "Home", "End", "Enter"].includes(key)) {
        event.preventDefault();
        if (key === "Home") state.boundary(state.min);
        else if (key === "End") state.boundary(state.max);
        else if (key === "Enter") state.commit();
        else
          state.stepBy(
            key === "ArrowUp" ? 1 : key === "ArrowDown" ? -1 : key === "PageUp" ? 10 : -10,
          );
      }
    }),
  });
  return render != null ? render(props) : <input {...props} />;
}

component NumberFieldIncrement(children?: React.Node = "+", render?: RenderProp, ...rest: Rest) {
  const state = useNumber();
  const props = withProps(rest, {
    children,
    type: "button",
    "aria-label": rest["aria-label"] ?? "Increase",
    disabled:
      state.disabled ||
      state.readOnly ||
      (state.max != null && state.value != null && state.value >= state.max),
    onClick: composeHandlers(rest.onClick, () => state.stepBy(1)),
  });
  return render != null ? render(props) : <button {...props} />;
}
component NumberFieldDecrement(children?: React.Node = "−", render?: RenderProp, ...rest: Rest) {
  const state = useNumber();
  const props = withProps(rest, {
    children,
    type: "button",
    "aria-label": rest["aria-label"] ?? "Decrease",
    disabled:
      state.disabled ||
      state.readOnly ||
      (state.min != null && state.value != null && state.value <= state.min),
    onClick: composeHandlers(rest.onClick, () => state.stepBy(-1)),
  });
  return render != null ? render(props) : <button {...props} />;
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
export {
  NumberFieldRoot as Root,
  NumberFieldInput as Input,
  NumberFieldIncrement as Increment,
  NumberFieldDecrement as Decrement,
};
