// @flow
//
// A progress bar, whose one line is the line everybody gets wrong.
//
// This is the smallest component in the package and the one most often written
// inline, which is exactly why it is worth owning: `<div className="bar" />`
// with a width in a style attribute is invisible to a screen reader, and the
// version that adds a role usually adds the rest of it wrong.
//
// # The indeterminate state
//
// A progress bar that does not know how far along it is **omits
// `aria-valuenow` entirely**. It does not set it to `0`.
//
// The two are opposite statements. `aria-valuenow="0"` says "nothing has
// happened yet", and a reader who asks again in ten seconds and hears zero
// again concludes the operation is stuck. Omitting it says "in progress,
// amount unknown", which is what a spinner means and what is actually true.
// The whole component is a conditional, and this is the condition.
//
// # `aria-valuetext`, for when the percentage is not the answer
//
// "3 of 10 files" is what a reader wants to hear; "30" is a number they then
// have to do arithmetic on. `aria-valuetext` replaces the announced value
// without touching `aria-valuenow`, so the assistive technology still has the
// number to draw a gauge with and the reader still hears the sentence.
//
// It is a prop rather than something a caller adds afterwards because
// `aria-valuenow` and `aria-valuetext` have to agree, and a caller spreading
// one of them onto a component that owns the other gets a control that says
// two different things.
//
// # No `"use client"`
//
// It holds no state, listens to nothing and manages no focus, so it renders on
// a server. That is the reason this is a component and not a hook: everything
// it knows, it was told.

import * as React from "@uniflowed/react";

import type { Rest } from "./internal/merge-props.js";
import { clamp } from "./internal/range.js";

/**
 * How far along something is, or that it is under way at all.
 *
 * `value` is `null` for a bar that does not know — the default, because a bar
 * that has not been told anything genuinely does not know, and the safe answer
 * has to be the honest one rather than "nothing has happened".
 *
 * A name is the caller's: `aria-label="Uploading"` or an `aria-labelledby`
 * pointing at a heading. A progress bar with no name is announced as
 * "progressbar" and a reader is left to guess what is progressing, which is
 * why every example in the documentation carries one.
 *
 * Nothing here is drawn and nothing is measured for the caller — unlike
 * `Slider`, whose value may be its own, a progress bar's value arrived as a
 * prop, so the caller already has everything they need to size a bar with and
 * a helpful custom property would only be their own arithmetic handed back.
 */
export component Progress(
  value?: number | null = null,
  min?: number = 0,
  max?: number = 100,
  valueText?: string,
  children?: React.Node,
  ...rest: Rest
) {
  const known = value == null ? null : clamp(value, min, max);

  return (
    <div
      {...rest}
      aria-valuemax={max}
      aria-valuemin={min}
      // Omitted, not zeroed. `aria-valuenow="0"` tells a reader that nothing
      // has happened; leaving it out tells them the amount is unknown, which
      // is the true one and the one a spinner means.
      aria-valuenow={known ?? undefined}
      aria-valuetext={valueText}
      role="progressbar"
    >
      {children}
    </div>
  );
}
