// @flow
//
// Badge: a short label beside something, such as a status, a count or a
// category, in five tones.
//
// `uf ui add badge` wrote this file into the project, and it is the project's
// from then on. `uf ui diff badge` shows how it has moved away from the
// registry in the uf you are running.
//
// # Why a component here, when `@uniflowed/ui` has none
//
// A badge makes no accessibility decision: it has no role, no state and no
// key. `@uniflowed/ui` declines one for that reason (`crates/uf_lib/src/ui.rs`
// records it). An application still wants every badge to look the same, and
// that is a decision about this application. So the badge is written here, in
// the application's repository, as a `<span>` that passes through everything a
// caller gives it.
//
// # What to keep true when you change it
//
// * **The words carry the meaning.** "Failed" in a red badge is read aloud as
//   "Failed". A red dot with no text is read as nothing. A badge that
//   stands for a status holds the status in text, even if the text is visually
//   hidden.
// * **A badge is not a button.** Nothing about it can be focused or pressed.
//   If clicking it does something, it is a `Button` (or a link) styled small,
//   because a keyboard user has to be able to reach it.
// * **A count needs its noun.** "3" beside "Inbox" is read as "Inbox 3". Put
//   the noun in an `aria-label` on the thing the badge belongs to, or in
//   visually hidden text, so it is read as "3 unread".
// * **Text stays on measured pairs.** `ink` on `sunken`, `accent` on
//   `accentSoft`, `accentInk` on `accent`, `danger` on `dangerSoft` and `ink`
//   on `surface`. `crates/uf_stylex/src/tests/preset.rs` holds each of these
//   to 4.5:1 in the light default and the dark theme, so a theme of your own
//   that keeps them keeps the contrast.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

/** How loud a badge is. `solid` is the one to reach for least. */
export type BadgeTone = "neutral" | "accent" | "solid" | "danger" | "outline";

/**
 * Every prop a caller passes that this file does not name, on its way to the
 * element. `key` is `empty` because React takes it off before a component is
 * called.
 */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  base: {
    display: "inline-flex",
    alignItems: "center",
    gap: ufTokens.space1,
    boxSizing: "border-box",
    minHeight: "20px",
    paddingBlock: 0,
    paddingInline: ufTokens.space2,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textXs,
    fontWeight: ufTokens.weightMedium,
    lineHeight: ufTokens.leadingTight,
    whiteSpace: "nowrap",
    verticalAlign: "middle",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "transparent",
    borderRadius: ufTokens.radiusSm,
  },
  neutral: {
    backgroundColor: ufTokens.sunken,
    color: ufTokens.ink,
  },
  accent: {
    backgroundColor: ufTokens.accentSoft,
    color: ufTokens.accent,
  },
  solid: {
    backgroundColor: ufTokens.accent,
    color: ufTokens.accentInk,
  },
  danger: {
    backgroundColor: ufTokens.dangerSoft,
    color: ufTokens.danger,
  },
  outline: {
    backgroundColor: ufTokens.surface,
    borderColor: ufTokens.border,
    color: ufTokens.ink,
  },
});

/**
 * A badge.
 *
 *     <Badge tone="danger">Failed</Badge>
 *
 * `xstyle` takes a `stylex.create` namespace and wins property by property;
 * `className` adds a class of your own beside these.
 */
export component Badge(
  children: React.Node,
  tone?: BadgeTone = "neutral",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  const styled = props(
    styles.base,
    match (tone) {
      "neutral" => styles.neutral,
      "accent" => styles.accent,
      "solid" => styles.solid,
      "danger" => styles.danger,
      "outline" => styles.outline,
    },
    xstyle,
  );
  return (
    <span {...rest} className={classNames(styled.className, className)}>
      {children}
    </span>
  );
}

/** The classes this file chose, then the caller's. */
function classNames(...names: $ReadOnlyArray<?string>): string | void {
  const present = names.filter((name) => name != null && name !== "");
  return present.length === 0 ? undefined : present.join(" ");
}
