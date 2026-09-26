// @flow
//
// Alert: a callout that draws attention on the page, and announces itself only
// when it appears after the page has loaded.
//
// `uf ui add alert` wrote this file into the project, and it is the project's
// from then on. `uf ui diff alert` shows how it has moved away from the
// registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The box, its tones and its type. `@uniflowed/ui`'s `Alert` owns the decision that
// makes it more than a box: `role="alert"` is a live region, so it is only put
// on an alert rendered with `live`, and a callout that was on the page when it
// loaded gets none — otherwise it interrupts every reader on every load, or is
// never heard at all. The title is a real heading at the level it is given.
//
// # What to keep true when you change it
//
// * **`live` is for an alert that appears.** A save that failed, a connection
//   that dropped. A notice that is always on the page leaves it off.
// * **The heading level fits the page.** `Alert.Title` takes `level`, 3 by
//   default.
// * **A tone is more than a colour.** `danger` and `info` add a thick edge and
//   the title's words should say what happened; the colour alone says nothing
//   to a reader who cannot see it.
// * **Text stays on measured pairs.** `ink` and `muted` on `surface`, which
//   `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1 in both themes; the
//   tone colours are edges, never text.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import { Alert } from "@uniflowed/ui";

/** What the alert is about, drawn as the colour of its edge. */
export type AlertTone = "neutral" | "info" | "danger";

/** Every prop a caller passes that this file does not name, for the part. */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  root: {
    display: "grid",
    gap: ufTokens.space1,
    boxSizing: "border-box",
    width: "100%",
    paddingBlock: ufTokens.space3,
    paddingInline: ufTokens.space4,
    backgroundColor: ufTokens.surface,
    color: ufTokens.ink,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    lineHeight: ufTokens.leadingBase,
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderRadius: ufTokens.radiusMd,
  },
  info: {
    borderInlineStartWidth: "4px",
    borderInlineStartColor: ufTokens.accent,
  },
  danger: {
    borderInlineStartWidth: "4px",
    borderInlineStartColor: ufTokens.danger,
  },
  title: {
    margin: 0,
    fontSize: ufTokens.textSm,
    fontWeight: ufTokens.weightBold,
    lineHeight: ufTokens.leadingTight,
  },
  description: {
    margin: 0,
    color: ufTokens.muted,
  },
});

/** A callout. `live` only for one that appears after the page has loaded. */
component AlertRoot(
  children: React.Node,
  tone?: AlertTone = "neutral",
  live?: boolean = false,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  const styled = props(
    styles.root,
    match (tone) {
      "neutral" => null,
      "info" => styles.info,
      "danger" => styles.danger,
    },
    xstyle,
  );
  return (
    <Alert.Root
      {...forwarded(rest)}
      className={classNames(styled.className, className)}
      live={live}
    >
      {children}
    </Alert.Root>
  );
}

/** What happened, as a heading at `level`. */
component AlertTitle(
  children: React.Node,
  level?: number = 3,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Alert.Title
      {...forwarded(rest)}
      className={classNames(props(styles.title, xstyle).className, className)}
      level={level}
    >
      {children}
    </Alert.Title>
  );
}

/** What it means, and what to do about it. */
component AlertDescription(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Alert.Description
      {...forwarded(rest)}
      className={classNames(props(styles.description, xstyle).className, className)}
    >
      {children}
    </Alert.Description>
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

/**
 * The parts, under the names `import * as Alert from "./alert.js"` gives them.
 *
 * One name per component (ubugeeei-prod/uf#1453): a page writes `<Alert.Root>`
 * and `<Alert.Title>`, the way it writes `@uniflowed/ui`'s own parts. Each
 * is declared under its full name, so React DevTools and an error say
 * `AlertRoot` rather than `Root`.
 */
export { AlertRoot as Root, AlertTitle as Title, AlertDescription as Description };
