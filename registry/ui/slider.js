"use client";
// @flow
//
// Slider: a value, or a range between two values, chosen by dragging a thumb
// along a rail or with the arrow keys.
//
// `uf ui add slider` wrote this file into the project, and it is the project's
// from then on. `uf ui diff slider` shows how it has moved away from the
// registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The rail, the filled part of it and the thumbs, across or down. Where they
// sit comes from what `@uniflowed/ui`'s `Slider` writes: `--uf-slider-start` and
// `--uf-slider-end` on the range, and `--uf-slider-fraction` on each thumb,
// all fractions of the way from `min` to `max`. The part owns the rest: a
// `role="slider"` thumb for every value with `aria-valuenow` and the bounds
// its neighbours set, the arrow keys, `Page Up` and `Page Down`, `Home` and
// `End`, a drag anywhere on the track, and a right-to-left page reversing the
// horizontal axis, which the logical `inset-inline` properties here follow.
//
// # What to keep true when you change it
//
// * **Every thumb has a name.** `labels` gives one per value, in order: a
//   reader hears "Minimum price, 20", not "slider, 20".
// * **Keep the thumbs inside the track.** The track takes the pointer; a thumb
//   outside it cannot be dragged.
// * **Keep the ring.** A thumb is focused by the keyboard, and its outline is
//   how a sighted keyboard user finds it.
// * **The fill is not the only sign.** The thumb's position says the value
//   too, and `valueText` says it in words for a reader.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import * as Primitive from "@uniflowed/ui";

/** Every prop a caller passes that this file does not name, for the part. */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  root: {
    display: "flex",
    alignItems: "center",
    boxSizing: "border-box",
    width: "100%",
    minWidth: "8rem",
    touchAction: "none",
    userSelect: "none",
  },
  rootVertical: {
    display: "inline-flex",
    flexDirection: "column",
    width: "auto",
    minWidth: 0,
    height: "10rem",
  },
  // Taller than the rail it draws, so the pointer has something to land on.
  track: {
    position: "relative",
    display: "flex",
    alignItems: "center",
    flexGrow: 1,
    height: "20px",
    cursor: "pointer",
  },
  trackVertical: {
    flexDirection: "column",
    width: "20px",
    height: "100%",
  },
  trackDisabled: {
    cursor: "not-allowed",
  },
  rail: {
    position: "relative",
    flexGrow: 1,
    height: "4px",
    borderRadius: ufTokens.radiusPill,
    backgroundColor: ufTokens.border,
  },
  railVertical: {
    width: "4px",
    height: "auto",
  },
  range: {
    position: "absolute",
    insetBlockStart: 0,
    insetBlockEnd: 0,
    insetInlineStart: "calc(var(--uf-slider-start, 0) * 100%)",
    insetInlineEnd: "calc((1 - var(--uf-slider-end, 0)) * 100%)",
    borderRadius: "inherit",
    backgroundColor: ufTokens.accent,
  },
  rangeVertical: {
    insetBlockStart: "calc((1 - var(--uf-slider-end, 0)) * 100%)",
    insetBlockEnd: "calc(var(--uf-slider-start, 0) * 100%)",
    insetInlineStart: 0,
    insetInlineEnd: 0,
  },
  rangeDisabled: {
    backgroundColor: ufTokens.muted,
  },
  thumb: {
    position: "absolute",
    insetBlockStart: "50%",
    insetInlineStart: "calc(var(--uf-slider-fraction, 0) * 100%)",
    boxSizing: "border-box",
    width: "18px",
    height: "18px",
    marginBlockStart: "-9px",
    marginInlineStart: "-9px",
    borderRadius: "50%",
    backgroundColor: ufTokens.surface,
    borderWidth: "2px",
    borderStyle: "solid",
    borderColor: { default: ufTokens.accent, ":is([aria-disabled=true])": ufTokens.muted },
    cursor: { default: "grab", ":is([aria-disabled=true])": "not-allowed" },
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
  },
  thumbVertical: {
    insetBlockStart: "auto",
    insetBlockEnd: "calc(var(--uf-slider-fraction, 0) * 100%)",
    insetInlineStart: "50%",
    marginBlockStart: 0,
    marginBlockEnd: "-9px",
  },
});

/**
 * The slider. `labels` names each thumb and sets how many there are: one for
 * a value, two for a range.
 */
export component Slider(
  labels: $ReadOnlyArray<string>,
  value?: $ReadOnlyArray<number>,
  defaultValue?: $ReadOnlyArray<number> = [0],
  onValueChange?: (value: $ReadOnlyArray<number>) => void,
  min?: number = 0,
  max?: number = 100,
  step?: number = 1,
  largeStep?: number = 10,
  orientation?: "horizontal" | "vertical" = "horizontal",
  disabled?: boolean = false,
  valueText?: (value: number, index: number) => string,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  const vertical = orientation === "vertical";
  return (
    <Primitive.Slider.Root
      {...forwarded(rest)}
      className={classNames(
        props(styles.root, vertical && styles.rootVertical, xstyle).className,
        className,
      )}
      defaultValue={defaultValue}
      disabled={disabled}
      largeStep={largeStep}
      max={max}
      min={min}
      onValueChange={onValueChange}
      orientation={orientation}
      step={step}
      value={value}
      valueText={valueText}
    >
      <Primitive.Slider.Track
        className={
          props(styles.track, vertical && styles.trackVertical, disabled && styles.trackDisabled)
            .className
        }
      >
        <span {...props(styles.rail, vertical && styles.railVertical)}>
          <Primitive.Slider.Range
            className={
              props(
                styles.range,
                vertical && styles.rangeVertical,
                disabled && styles.rangeDisabled,
              ).className
            }
          />
        </span>
        {labels.map((label, index) => (
          <Primitive.Slider.Thumb
            aria-label={label}
            className={props(styles.thumb, vertical && styles.thumbVertical).className}
            index={index}
            key={`${index}:${label}`}
          />
        ))}
      </Primitive.Slider.Track>
    </Primitive.Slider.Root>
  );
}

/**
 * A caller's props on their way into a part rather than onto an element. Flow
 * checks that spread against the part's own `...rest`, whose `key` is `empty`
 * where this file's indexer says `mixed`; `@uniflowed/ui` papers over the same
 * hole the same way, and nothing checked is lost, because the elements its
 * parts render have `any`-typed props in uf's library today.
 */
function forwarded(rest: Rest): $FlowFixMe {
  return rest;
}

/** The classes this file chose, then the caller's. */
function classNames(...names: $ReadOnlyArray<?string>): string | void {
  const present = names.filter((name) => name != null && name !== "");
  return present.length === 0 ? undefined : present.join(" ");
}
