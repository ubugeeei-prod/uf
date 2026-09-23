// @flow
//
// Progress: a bar that says how far a task has got, and says nothing about an
// amount it does not know.
//
// `uf ui add progress` wrote this file into the project, and it is the
// project's from then on. `uf ui diff progress` shows how it has moved away
// from the registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The track and the fill. `@uniflowed/ui`'s `Progress` owns what a reader is told:
// `role="progressbar"`, `aria-valuemin`, `aria-valuemax`, and an
// `aria-valuenow` that is left off, rather than set to zero, while the amount
// is unknown — zero would say that nothing has happened. The fill is drawn from
// the same `value`, `min` and `max`, through the `--uf-progress` property this
// file sets, so the bar and the announcement cannot disagree.
//
// # What to keep true when you change it
//
// * **Name it.** `aria-label`, or `aria-labelledby` pointing at the words
//   beside it; a progress bar with no name is announced as a number.
// * **Say the value in words when a number is not the answer.** `valueText`,
//   such as "3 of 10 photos", is what a reader hears instead of "30".
// * **An unknown amount looks unknown.** The whole track turns `accentSoft`
//   and no fill is drawn, so it cannot be mistaken for a bar at zero.
// * **The fill is `accent` on `sunken`,** a boundary rather than text, and it
//   stops easing for a reader who asked for reduced motion.

import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import { Progress as ProgressPart } from "@uniflowed/ui";

/** Every prop a caller passes that this file does not name, for the part. */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  track: {
    position: "relative",
    display: "block",
    width: "100%",
    height: "8px",
    overflow: "hidden",
    borderRadius: ufTokens.radiusSm,
    backgroundColor: { default: ufTokens.sunken, ":not([aria-valuenow])": ufTokens.accentSoft },
  },
  // Full width, scaled along the inline axis to the fraction done: moving a
  // transform repaints the bar, where moving `width` laid it out again on
  // every frame.
  fill: {
    display: "block",
    width: "100%",
    height: "100%",
    borderRadius: ufTokens.radiusSm,
    backgroundColor: ufTokens.accent,
    transform: "scaleX(var(--uf-progress, 0))",
    transformOrigin: { default: "left", ":dir(rtl)": "right" },
    // `durationSlow` on the standard curve: a step of progress is seen to
    // fill in rather than jump, and a run of quick updates still reads as one
    // steady movement. Under reduced motion the bar is set, not moved.
    transitionProperty: { default: "transform", "@media (prefers-reduced-motion: reduce)": "none" },
    transitionDuration: ufTokens.durationSlow,
    transitionTimingFunction: ufTokens.easing,
  },
});

/** A progress bar. Leave `value` off while the amount is unknown. */
export component Progress(
  value?: number | null = null,
  min?: number = 0,
  max?: number = 100,
  valueText?: string,
  style?: { readonly [string]: mixed },
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  const fraction =
    value == null || max <= min ? null : Math.min(1, Math.max(0, (value - min) / (max - min)));
  return (
    <ProgressPart
      {...forwarded(rest)}
      className={classNames(props(styles.track, xstyle).className, className)}
      max={max}
      min={min}
      style={fraction == null ? style : { ...style, "--uf-progress": String(fraction) }}
      value={value}
      valueText={valueText}
    >
      <span {...props(styles.fill)} />
    </ProgressPart>
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
