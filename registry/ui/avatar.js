"use client";
// @flow
//
// Avatar: a person's picture in a circle, and their initials when the picture
// cannot be shown.
//
// `uf ui add avatar` wrote this file into the project, and it is the project's
// from then on. `uf ui diff avatar` shows how it has moved away from the
// registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The circle, its three sizes, the picture's fit and the initials' type.
// `@uniflowed/ui`'s `Avatar` owns the three states: loading, loaded and failed,
// with the initials held back for a moment so a cached picture does not flash
// somebody's initials first, and an `alt` that is empty unless it is given.
//
// # What to keep true when you change it
//
// * **`alt` is empty beside a name.** An avatar next to "Ada Lovelace" is
//   decoration, and naming it again makes a screen reader say the name twice.
//   An avatar standing alone passes `alt="Ada Lovelace"`.
// * **The initials are decoration too** when a name is beside them; they are
//   what a sighted reader sees while the picture is missing.
// * **The initials sit on a measured pair:** `ink` on `sunken`, which
//   `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1 in both themes.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import * as Primitive from "@uniflowed/ui";

/** How big the circle is. */
export type AvatarSize = "sm" | "md" | "lg";

/** Every prop a caller passes that this file does not name, for the part. */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  root: {
    position: "relative",
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    flexShrink: 0,
    overflow: "hidden",
    borderRadius: ufTokens.radiusPill,
    backgroundColor: ufTokens.sunken,
    color: ufTokens.ink,
    fontFamily: ufTokens.fontSans,
    fontWeight: ufTokens.weightMedium,
    lineHeight: 1,
    userSelect: "none",
  },
  sm: {
    width: "32px",
    height: "32px",
    fontSize: ufTokens.textXs,
  },
  md: {
    width: "40px",
    height: "40px",
    fontSize: ufTokens.textSm,
  },
  lg: {
    width: "56px",
    height: "56px",
    fontSize: ufTokens.textMd,
  },
  image: {
    width: "100%",
    height: "100%",
    objectFit: "cover",
  },
  fallback: {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    width: "100%",
    height: "100%",
  },
});

/** The circle. */
export component Avatar(
  children: React.Node,
  size?: AvatarSize = "md",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  const styled = props(
    styles.root,
    match (size) {
      "sm" => styles.sm,
      "md" => styles.md,
      "lg" => styles.lg,
    },
    xstyle,
  );
  return (
    <Primitive.AvatarRoot {...forwarded(rest)} className={classNames(styled.className, className)}>
      {children}
    </Primitive.AvatarRoot>
  );
}

/** The picture. `alt` stays empty when the person's name is beside it. */
export component AvatarImage(
  src?: string | null,
  alt?: string = "",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.AvatarImage
      {...forwarded(rest)}
      alt={alt}
      className={classNames(props(styles.image, xstyle).className, className)}
      src={src}
    />
  );
}

/** What shows while there is no picture: initials, usually. */
export component AvatarFallback(
  children: React.Node,
  delay?: number = 300,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.AvatarFallback
      {...forwarded(rest)}
      className={classNames(props(styles.fallback, xstyle).className, className)}
      delay={delay}
    >
      {children}
    </Primitive.AvatarFallback>
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
