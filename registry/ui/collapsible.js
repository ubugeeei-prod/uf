"use client";
// @flow
//
// Collapsible: a button that shows and hides one region, with the region's
// text still findable while it is closed.
//
// `uf ui add collapsible` wrote this file into the project, and it is the
// project's from then on. `uf ui diff collapsible` shows how it has moved away
// from the registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The trigger, its chevron and the spacing of the region. The disclosure
// pattern is imported from `@uniflowed/ui`'s `Collapsible`: `aria-expanded` and
// `aria-controls` on the trigger, and a closed region that is
// `hidden="until-found"` rather than removed, so the browser's find-in-page
// still reaches its text and opens it. The chevron turns from `aria-expanded`
// through `:is([aria-expanded=true])`.
//
// # What to keep true when you change it
//
// * **The trigger's words say what opens.** "Show the 3 older replies" rather
//   than "More"; a reader hears the words and whether they are expanded.
// * **Nothing is only in the closed region.** A reader who never opens it has
//   to be able to finish the task.
// * **The chevron is decoration.** It is `aria-hidden`: `aria-expanded` is what
//   a reader is told.
// * **Colour comes from tokens, in measured pairs:** `ink` on `surface` and on
//   `surfaceHover`, which `crates/uf_stylex/src/tests/preset.rs` holds to
//   4.5:1 in both themes.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import * as Primitive from "@uniflowed/ui";

/** Every prop a caller passes that this file does not name, for the part. */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  root: {
    display: "grid",
    gap: ufTokens.space2,
    fontFamily: ufTokens.fontSans,
    color: ufTokens.ink,
  },
  trigger: {
    // Read by the chevron, which cannot see the button's state.
    "--uf-collapsible-turn": { default: "0deg", ":is([aria-expanded=true])": "180deg" },
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: ufTokens.space2,
    boxSizing: "border-box",
    minHeight: "36px",
    margin: 0,
    paddingInline: ufTokens.space3,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    fontWeight: ufTokens.weightMedium,
    lineHeight: ufTokens.leadingTight,
    textAlign: "start",
    color: ufTokens.ink,
    backgroundColor: { default: "transparent", ":hover": ufTokens.surfaceHover },
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderRadius: ufTokens.radiusMd,
    cursor: { default: "pointer", ":disabled": "not-allowed" },
    opacity: { default: 1, ":disabled": 0.55 },
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
  },
  chevron: {
    flexShrink: 0,
    transform: "rotate(var(--uf-collapsible-turn))",
    // A half turn is travel, so it takes `durationBase`; under reduced
    // motion the chevron is simply the other way up.
    transitionProperty: { default: "transform", "@media (prefers-reduced-motion: reduce)": "none" },
    transitionDuration: ufTokens.durationBase,
    transitionTimingFunction: ufTokens.easing,
  },
  content: {
    fontSize: ufTokens.textSm,
    lineHeight: ufTokens.leadingBase,
  },
});

/** The trigger and its region, open or closed. Uncontrolled unless `open` is given. */
export component Collapsible(
  children: React.Node,
  defaultOpen?: boolean = false,
  open?: boolean,
  onOpenChange?: (open: boolean) => void,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <div {...rest} className={classNames(props(styles.root, xstyle).className, className)}>
      <Primitive.CollapsibleRoot defaultOpen={defaultOpen} onOpenChange={onOpenChange} open={open}>
        {children}
      </Primitive.CollapsibleRoot>
    </div>
  );
}

/** The button that shows and hides the region. */
export component CollapsibleTrigger(
  children: React.Node,
  disabled?: boolean = false,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.CollapsibleTrigger
      {...forwarded(rest)}
      className={classNames(props(styles.trigger, xstyle).className, className)}
      disabled={disabled}
    >
      {children}
      <svg
        {...props(styles.chevron)}
        aria-hidden="true"
        fill="none"
        focusable="false"
        height="16"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="2"
        viewBox="0 0 24 24"
        width="16"
      >
        <path d="m6 9 6 6 6-6" />
      </svg>
    </Primitive.CollapsibleTrigger>
  );
}

/** The region the trigger shows. */
export component CollapsibleContent(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.CollapsibleContent
      {...forwarded(rest)}
      className={classNames(props(styles.content, xstyle).className, className)}
    >
      {children}
    </Primitive.CollapsibleContent>
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
