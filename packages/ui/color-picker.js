// @flow
"use client";
import * as React from "@uniflowed/react";
import { createContext, useContext, useState } from "@uniflowed/react";
import { useControlled } from "./internal/controlled-state.js";
import type { RenderProp, Rest } from "./internal/merge-props.js";
import { composeHandlers, withProps } from "./internal/merge-props.js";
import { directionOf } from "./internal/roving-focus.js";

/** Canonical #rrggbb or #rrggbbaa, with CSS's short hex forms accepted at the boundary. */
export function parseColor(value: string): string | null {
  let hex = value.trim().toLowerCase();
  if (!/^#(?:[0-9a-f]{3}|[0-9a-f]{4}|[0-9a-f]{6}|[0-9a-f]{8})$/.test(hex)) return null;
  if (hex.length <= 5)
    hex =
      "#" +
      Array.from(hex.slice(1))
        .map((char) => char + char)
        .join("");
  return hex;
}
type ColorState = { value: string, disabled: boolean, set: (value: string) => void };
const ColorContext: React.Context<ColorState | null> = createContext(null);
hook useColor(): ColorState {
  const color = useContext(ColorContext);
  if (color == null) throw new Error("ColorPicker parts must be inside ColorPicker.Root");
  return color;
}
export component ColorPickerRoot(
  children: React.Node,
  value?: string,
  defaultValue?: string = "#000000",
  onValueChange?: (value: string) => void,
  disabled?: boolean = false,
  render?: RenderProp,
  ...rest: Rest
) {
  const [current, setCurrent] = useControlled(value, defaultValue, onValueChange);
  const normalized = parseColor(current);
  if (normalized == null) throw new RangeError("ColorPicker value must be a CSS hex color");
  const state = {
    value: normalized,
    disabled,
    set: (next: string) => {
      const color = parseColor(next);
      if (!disabled && color != null) setCurrent(color);
    },
  };
  const props = withProps(rest, { children });
  return (
    <ColorContext.Provider value={state}>
      {render != null ? render(props) : <div {...props} />}
    </ColorContext.Provider>
  );
}
export component ColorPickerInput(render?: RenderProp, ...rest: Rest) {
  const color = useColor();
  const props = withProps(rest, {
    type: "color",
    value: color.value.slice(0, 7),
    disabled: color.disabled,
    onChange: composeHandlers(rest.onChange, (event: $FlowFixMe) =>
      color.set(event.currentTarget.value + color.value.slice(7)),
    ),
  });
  return render != null ? render(props) : <input {...props} />;
}
export component ColorPickerField(render?: RenderProp, ...rest: Rest) {
  const color = useColor();
  const [draft, setDraft] = useState<string | null>(null);
  const text = draft ?? color.value;
  const invalid = parseColor(text) == null;
  const commit = () => {
    if (!invalid) {
      color.set(text);
      setDraft(null);
    }
  };
  const props = withProps(rest, {
    type: "text",
    value: text,
    disabled: color.disabled,
    "aria-invalid": invalid ? "true" : rest["aria-invalid"],
    onChange: composeHandlers(rest.onChange, (event: $FlowFixMe) =>
      setDraft(event.currentTarget.value),
    ),
    onBlur: composeHandlers(rest.onBlur, commit),
    onKeyDown: composeHandlers(rest.onKeyDown, (event: $FlowFixMe) => {
      if (event.key === "Enter") {
        event.preventDefault();
        commit();
      }
    }),
  });
  return render != null ? render(props) : <input {...props} />;
}
export component ColorPickerChannel(
  channel: "red" | "green" | "blue" | "alpha",
  render?: RenderProp,
  ...rest: Rest
) {
  const color = useColor();
  const at = ["red", "green", "blue", "alpha"].indexOf(channel);
  const value = parseInt(
    (color.value + (color.value.length === 7 ? "ff" : "")).slice(1 + at * 2, 3 + at * 2),
    16,
  );
  const change = (next: number) => {
    const hex = color.value.slice(1).padEnd(8, "f");
    const byte = Math.max(0, Math.min(255, Math.round(next)))
      .toString(16)
      .padStart(2, "0");
    const full = hex.slice(0, at * 2) + byte + hex.slice(at * 2 + 2);
    color.set("#" + (at === 3 || color.value.length === 9 ? full : full.slice(0, 6)));
  };
  const props = withProps(rest, {
    type: "range",
    min: 0,
    max: 255,
    step: 1,
    value,
    disabled: color.disabled,
    "aria-label": rest["aria-label"] ?? channel,
    "aria-valuetext": channel === "alpha" ? `${Math.round((value / 255) * 100)}%` : String(value),
    onChange: composeHandlers(rest.onChange, (event: $FlowFixMe) =>
      change(Number(event.currentTarget.value)),
    ),
    onKeyDown: composeHandlers(rest.onKeyDown, (event: $FlowFixMe) => {
      if (color.disabled) return;
      const key = event.key;
      if (
        ![
          "ArrowLeft",
          "ArrowRight",
          "ArrowUp",
          "ArrowDown",
          "PageUp",
          "PageDown",
          "Home",
          "End",
        ].includes(key)
      )
        return;
      event.preventDefault();
      const rtl = directionOf(event.currentTarget) === "rtl";
      change(
        key === "Home"
          ? 0
          : key === "End"
            ? 255
            : value +
              (key === "PageUp"
                ? 10
                : key === "PageDown"
                  ? -10
                  : key === "ArrowUp" || key === (rtl ? "ArrowLeft" : "ArrowRight")
                    ? 1
                    : -1),
      );
    }),
  });
  return render != null ? render(props) : <input {...props} />;
}
export component ColorPickerSwatch(render?: RenderProp, ...rest: Rest) {
  const color = useColor();
  const props = withProps(rest, {
    "aria-hidden": "true",
    "data-color": color.value,
    style: { ...(rest.style as $FlowFixMe), backgroundColor: color.value },
  });
  return render != null ? render(props) : <span {...props} />;
}
