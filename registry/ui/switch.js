// @flow
//
// Switch: a setting that takes effect the moment it is flipped, drawn as a
// track and a thumb beside its label.
//
// `uf ui add switch` wrote this file into the project, and it is the project's
// from then on. `uf ui diff switch` shows how it has moved away from the
// registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The track, the thumb and the label beside them, all inside the one button the
// part renders, so the whole row is the target and its words are the name.
// `@uniflowed/ui`'s `Switch` owns `role="switch"`, `aria-checked`, and `Space` and
// `Enter` both flipping it. The thumb and the track follow `aria-checked`
// through `:is([aria-checked=true])`, so a click, a key and a controlled
// `checked` all draw the same thing.
//
// # What to keep true when you change it
//
// * **It is for a setting that applies at once.** An answer on the way to a
//   submit button is a `checkbox`, and a reader is told something different.
// * **Its words are its name.** Put the label inside `Switch`; a switch with no
//   words needs `aria-label`.
// * **On is more than a colour.** The thumb moves as the track fills, so the
//   state is not carried by colour alone (WCAG 1.4.1).
// * **The colours are measured pairs.** Off, a `surface` thumb on a `muted`
//   track; on, an `accentInk` thumb on an `accent` track. Both are pairs
//   `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1 in both themes.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import { Switch as SwitchPart } from "@uniflowed/ui";

/** Every prop a caller passes that this file does not name, for the part. */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  root: {
    // Read by the track and the thumb, which cannot see the button's state.
    "--uf-switch-track": { default: ufTokens.muted, ":is([aria-checked=true])": ufTokens.accent },
    "--uf-switch-thumb": {
      default: ufTokens.surface,
      ":is([aria-checked=true])": ufTokens.accentInk,
    },
    "--uf-switch-shift": { default: "0px", ":is([aria-checked=true])": "16px" },
    display: "inline-flex",
    alignItems: "center",
    gap: ufTokens.space3,
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
    cursor: { default: "pointer", ":disabled": "not-allowed" },
    opacity: { default: 1, ":disabled": 0.55 },
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
  },
  track: {
    position: "relative",
    flexShrink: 0,
    boxSizing: "border-box",
    width: "36px",
    height: "20px",
    borderRadius: ufTokens.radiusPill,
    backgroundColor: "var(--uf-switch-track)",
    transitionProperty: "background-color",
    transitionDuration: {
      default: ufTokens.durationFast,
      "@media (prefers-reduced-motion: reduce)": "0s",
    },
    transitionTimingFunction: ufTokens.easing,
  },
  thumb: {
    position: "absolute",
    top: "2px",
    left: "2px",
    width: "16px",
    height: "16px",
    borderRadius: ufTokens.radiusPill,
    backgroundColor: "var(--uf-switch-thumb)",
    transform: "translateX(var(--uf-switch-shift))",
    transitionProperty: "transform",
    transitionDuration: {
      default: ufTokens.durationFast,
      "@media (prefers-reduced-motion: reduce)": "0s",
    },
    transitionTimingFunction: ufTokens.easing,
  },
});

/** A switch and its label. Uncontrolled from `defaultChecked` unless `checked` is given. */
export component Switch(
  children?: React.Node,
  checked?: boolean,
  defaultChecked?: boolean = false,
  onCheckedChange?: (checked: boolean) => void,
  disabled?: boolean = false,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <SwitchPart
      {...forwarded(rest)}
      checked={checked}
      className={classNames(props(styles.root, xstyle).className, className)}
      defaultChecked={defaultChecked}
      disabled={disabled}
      onCheckedChange={onCheckedChange}
    >
      <span {...props(styles.track)} aria-hidden="true">
        <span {...props(styles.thumb)} />
      </span>
      {children}
    </SwitchPart>
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
