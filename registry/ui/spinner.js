// @flow
//
// Spinner: something is under way and nobody knows how far along it is, drawn
// as a turning arc and announced as a progress bar with no value.
//
// `uf ui add spinner` wrote this file into the project, and it is the
// project's from then on. `uf ui diff spinner` shows how it has moved away
// from the registry in the uf you are running.
//
// # Why a component here, when `@uniflowed/ui` has none
//
// `@uniflowed/ui` already has the spinner's one decision, in `Progress`: a bar
// that does not know how far along it is omits `aria-valuenow`, so a reader
// hears "in progress" rather than a zero that never moves. A second headless
// component with the same role would be a second place for that to go wrong,
// so the package declines a `Spinner` (`crates/uf_lib/src/ui.rs` records it)
// and this file renders `Progress` with no value around a drawing.
//
// # What to keep true when you change it
//
// * **A spinner has a name.** It is `role="progressbar"`, and one with no name
//   is announced as "progressbar" with nothing to say what is progressing. The
//   name is "Loading" until you pass `label`; say what is loading when you can
//   ("Loading invoices").
// * **Unless something beside it already says so.** A spinner inside a button
//   that reads "Saving…" would be announced twice. Pass `label={null}` there:
//   the drawing is then `aria-hidden` and nothing about it is announced.
// * **It never claims a value.** The day the amount is known, use `Progress`
//   with a `value`, which is the component that can say how far along it is.
// * **Reduced motion is a mark that fades, not one that turns.** uf's StyleX
//   has no `@keyframes`, and a spinner cannot be a transition, because nothing
//   about it changes state. So the drawing is an SVG with two marks in it: an
//   arc that turns once a second through SMIL's `<animateTransform>`, and the
//   same arc still, whose opacity pulses once every two seconds. A
//   `prefers-reduced-motion: reduce` media query decides which one is
//   displayed, in CSS, so the choice is made before any script runs and a page
//   rendered on a server gets it right. Turning is travel and fading is not,
//   which is the line every other default style in the registry draws.
// * **It is a `<span>`.** `Progress` renders a `<div>` unless told otherwise,
//   and a `<div>` inside a `<button>` or a `<p>` is not allowed there. The
//   role and the attributes are the part's either way.
// * **The arc is drawn in `currentColor`.** It takes the colour of the text
//   around it, so a spinner in a primary button is `accentInk` and one on a
//   page is `accent`, without a prop.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import { Progress } from "@uniflowed/ui";

/** How big it is: `sm` sits in a line of text or a button, `lg` fills a panel. */
export type SpinnerSize = "sm" | "md" | "lg";

/** Every prop a caller passes that this file does not name, for the part. */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  root: {
    display: "inline-flex",
    flexShrink: 0,
    color: ufTokens.accent,
    verticalAlign: "middle",
  },
  inherit: {
    color: "inherit",
  },
  sm: {
    width: "16px",
    height: "16px",
  },
  md: {
    width: "24px",
    height: "24px",
  },
  lg: {
    width: "40px",
    height: "40px",
  },
  svg: {
    display: "block",
    width: "100%",
    height: "100%",
  },
  // Shown unless the reader asked for reduced motion.
  turning: {
    display: { default: "inline", "@media (prefers-reduced-motion: reduce)": "none" },
  },
  // Shown only when they did.
  still: {
    display: { default: "none", "@media (prefers-reduced-motion: reduce)": "inline" },
  },
});

/**
 * A spinner.
 *
 *     <Spinner label="Loading invoices" />
 *     <Button disabled><Spinner label={null} size="sm" tone="inherit" /> Saving…</Button>
 *
 * `tone="inherit"` draws it in the colour of the text around it rather than in
 * `accent`. `xstyle` takes a `stylex.create` namespace and wins property by
 * property; `className` adds a class of your own beside these.
 */
export component Spinner(
  label?: string | null = "Loading",
  size?: SpinnerSize = "md",
  tone?: "accent" | "inherit" = "accent",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  const styled = classNames(
    props(
      styles.root,
      tone === "inherit" && styles.inherit,
      match (size) {
        "sm" => styles.sm,
        "md" => styles.md,
        "lg" => styles.lg,
      },
      xstyle,
    ).className,
    className,
  );
  if (label == null) {
    return (
      <span {...rest} aria-hidden="true" className={styled}>
        <Drawing />
      </span>
    );
  }
  // A `<span>` rather than `Progress`'s `<div>`, so a spinner can sit in a
  // line of text or inside a `<button>`, which may hold only phrasing content.
  return (
    <Progress
      {...forwarded(rest)}
      aria-label={label}
      className={styled}
      render={(part) => <span {...part} />}
    >
      <Drawing />
    </Progress>
  );
}

/** The arc, turning, and the same arc still and fading for reduced motion. */
component Drawing() {
  return (
    <svg
      {...props(styles.svg)}
      aria-hidden="true"
      fill="none"
      focusable="false"
      viewBox="0 0 24 24"
    >
      <circle cx="12" cy="12" opacity="0.2" r="9" stroke="currentColor" strokeWidth="2.5" />
      <g {...props(styles.turning)}>
        <path d="M12 3a9 9 0 0 1 9 9" stroke="currentColor" strokeLinecap="round" strokeWidth="2.5">
          <animateTransform
            attributeName="transform"
            dur="1s"
            from="0 12 12"
            repeatCount="indefinite"
            to="360 12 12"
            type="rotate"
          />
        </path>
      </g>
      <g {...props(styles.still)}>
        <path d="M12 3a9 9 0 0 1 9 9" stroke="currentColor" strokeLinecap="round" strokeWidth="2.5">
          <animate attributeName="opacity" dur="2s" repeatCount="indefinite" values="1;0.35;1" />
        </path>
      </g>
    </svg>
  );
}

/**
 * A caller's props on their way into `Progress` rather than onto an element.
 * The same widening `progress.js` makes, for the same reason: Flow checks the
 * spread against the part's own `...rest`, whose `key` is `empty` where this
 * file's indexer says `mixed`, and nothing checked is lost, because the element
 * `Progress` renders has `any`-typed props in uf's library today.
 */
function forwarded(rest: Rest): $FlowFixMe {
  return rest;
}

/** The classes this file chose, then the caller's. */
function classNames(...names: $ReadOnlyArray<?string>): string | void {
  const present = names.filter((name) => name != null && name !== "");
  return present.length === 0 ? undefined : present.join(" ");
}
