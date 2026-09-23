// @flow
//
// Radio group: one answer out of several, drawn as a list of circles beside
// their labels.
//
// `uf ui add radio-group` wrote this file into the project, and it is the
// project's from then on. `uf ui diff radio-group` shows how it has moved away
// from the registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The circles, the dot in the chosen one, and the layout of the list.
// `@uniflowed/ui`'s `RadioGroup` owns the pattern: `role="radiogroup"`, radios
// with `aria-checked`, one tab stop for the whole group — the chosen answer, or
// the first while there is none — arrow keys that check as they move, and the
// hidden input a form submits under `name`. The dot follows `aria-checked`
// through `:is([aria-checked=true])`.
//
// # What to keep true when you change it
//
// * **Name the group.** `aria-label` on `RadioGroup`, or `aria-labelledby`
//   pointing at the question it answers.
// * **Each answer's words are its name.** Put them inside `RadioGroupItem`.
// * **The circle is an edge a reader can see.** A `muted` ring on `surface`,
//   and an `accent` ring and dot when chosen; `muted` on `surface` is a pair
//   `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1 in both themes.
// * **A disabled answer stays in the list,** announced as unavailable and
//   passed over by the arrow keys.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import * as Primitive from "@uniflowed/ui";

/** Which way the answers are listed, and which arrow keys move between them. */
export type RadioGroupOrientation = "horizontal" | "vertical";

/** Every prop a caller passes that this file does not name, for the part. */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  root: {
    display: "flex",
    flexDirection: { default: "column", ":is([aria-orientation=horizontal])": "row" },
    flexWrap: "wrap",
    alignItems: "flex-start",
    gap: { default: ufTokens.space1, ":is([aria-orientation=horizontal])": ufTokens.space4 },
  },
  item: {
    // Read by the circle and the dot, which cannot see the button's state.
    "--uf-radio-ring": { default: ufTokens.muted, ":is([aria-checked=true])": ufTokens.accent },
    "--uf-radio-dot": { default: "0", ":is([aria-checked=true])": "1" },
    display: "inline-flex",
    alignItems: "center",
    gap: ufTokens.space2,
    minHeight: "32px",
    margin: 0,
    padding: 0,
    borderWidth: 0,
    borderRadius: ufTokens.radiusSm,
    backgroundColor: "transparent",
    color: ufTokens.ink,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    lineHeight: ufTokens.leadingTight,
    textAlign: "start",
    cursor: {
      default: "pointer",
      ":disabled": "not-allowed",
      ":is([aria-disabled=true])": "not-allowed",
    },
    opacity: { default: 1, ":disabled": 0.55, ":is([aria-disabled=true])": 0.55 },
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
  },
  circle: {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    flexShrink: 0,
    boxSizing: "border-box",
    width: ufTokens.sizeControl,
    height: ufTokens.sizeControl,
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--uf-radio-ring)",
    borderRadius: ufTokens.radiusPill,
    backgroundColor: ufTokens.surface,
  },
  dot: {
    width: "8px",
    height: "8px",
    borderRadius: ufTokens.radiusPill,
    backgroundColor: ufTokens.accent,
    // Fades in rather than growing from nothing: a dot that scales up reads
    // as a pop, and the change is the same without the motion.
    opacity: "var(--uf-radio-dot)",
    transitionProperty: "opacity",
    transitionDuration: {
      default: ufTokens.durationFast,
      "@media (prefers-reduced-motion: reduce)": "0s",
    },
    transitionTimingFunction: ufTokens.easing,
  },
});

/** The group. Uncontrolled from `defaultValue` unless `value` is given. */
export component RadioGroup(
  children: React.Node,
  defaultValue?: string | null = null,
  value?: string | null,
  onValueChange?: (value: string) => void,
  orientation?: RadioGroupOrientation = "vertical",
  name?: string,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.RadioGroupRoot
      {...forwarded(rest)}
      className={classNames(props(styles.root, xstyle).className, className)}
      defaultValue={defaultValue}
      name={name}
      onValueChange={onValueChange}
      orientation={orientation}
      value={value}
    >
      {children}
    </Primitive.RadioGroupRoot>
  );
}

/** One answer and its words. */
export component RadioGroupItem(
  value: string,
  children?: React.Node,
  disabled?: boolean = false,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.RadioGroupItem
      {...forwarded(rest)}
      className={classNames(props(styles.item, xstyle).className, className)}
      disabled={disabled}
      value={value}
    >
      <span {...props(styles.circle)} aria-hidden="true">
        <span {...props(styles.dot)} />
      </span>
      {children}
    </Primitive.RadioGroupItem>
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
