"use client";
// @flow
//
// Scroll area: a box that scrolls what it holds, with thin scrollbars drawn in
// the page's colours instead of the platform's.
//
// `uf ui add scroll-area` wrote this file into the project, and it is the
// project's from then on. `uf ui diff scroll-area` shows how it has moved away
// from the registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The box, the tracks and the thumbs, and hiding the platform's own scrollbar
// inside the box. A thumb's length and position come from what
// `@uniflowed/ui`'s `ScrollArea` writes on its scrollbar: `--uf-scroll-thumb-size`
// and `--uf-scroll-thumb-offset` down the box, and the same names ending in
// `-x` across it, each a fraction of the track. The part owns the rest: a
// viewport that is a named `role="region"` and a tab stop, so the keyboard
// scrolls it, and scrollbars that are `aria-hidden`, because the viewport
// already scrolls for every reader.
//
// # What to keep true when you change it
//
// * **Name it.** `label` names the region a keyboard user tabs into.
// * **Keep the ring.** The viewport takes focus, and its outline is how a
//   sighted keyboard user knows the arrow keys will scroll it.
// * **The drawn bars show; they do not scroll.** The wheel, a touch and the
//   keyboard scroll the viewport. A thumb fades out while everything fits.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import * as Primitive from "@uniflowed/ui";

/** Every prop a caller passes that this file does not name, for the part. */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  root: {
    position: "relative",
    display: "flex",
    flexDirection: "column",
    boxSizing: "border-box",
    overflow: "hidden",
  },
  viewport: {
    flexGrow: 1,
    minHeight: 0,
    overflow: "auto",
    scrollbarWidth: "none",
    borderRadius: "inherit",
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "-2px",
  },
  scrollbar: {
    position: "absolute",
    borderRadius: ufTokens.radiusPill,
    pointerEvents: "none",
  },
  scrollbarVertical: {
    insetBlockStart: "2px",
    insetBlockEnd: "2px",
    insetInlineEnd: "2px",
    width: "6px",
  },
  scrollbarHorizontal: {
    insetInlineStart: "2px",
    insetInlineEnd: "2px",
    insetBlockEnd: "2px",
    height: "6px",
  },
  thumb: {
    position: "absolute",
    borderRadius: "inherit",
    backgroundColor: ufTokens.muted,
  },
  // Gone while the thumb is the whole track, which is when nothing overflows.
  thumbVertical: {
    insetInlineStart: 0,
    insetInlineEnd: 0,
    insetBlockStart: "calc(var(--uf-scroll-thumb-offset, 0) * 100%)",
    height: "calc(var(--uf-scroll-thumb-size, 1) * 100%)",
    opacity: "calc((1 - var(--uf-scroll-thumb-size, 1)) * 1000)",
  },
  thumbHorizontal: {
    insetBlockStart: 0,
    insetBlockEnd: 0,
    insetInlineStart: "calc(var(--uf-scroll-thumb-offset-x, 0) * 100%)",
    width: "calc(var(--uf-scroll-thumb-size-x, 1) * 100%)",
    opacity: "calc((1 - var(--uf-scroll-thumb-size-x, 1)) * 1000)",
  },
});

/**
 * The box. Give it a height through `xstyle`; `orientation` says which way
 * its content overflows, and so which scrollbars it draws.
 */
export component ScrollArea(
  children: React.Node,
  label: string,
  orientation?: "vertical" | "horizontal" | "both" = "vertical",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  const down = orientation !== "horizontal";
  const across = orientation !== "vertical";
  return (
    <Primitive.ScrollAreaRoot
      {...forwarded(rest)}
      className={classNames(props(styles.root, xstyle).className, className)}
      label={label}
    >
      <Primitive.ScrollAreaViewport className={props(styles.viewport).className}>
        {children}
      </Primitive.ScrollAreaViewport>
      {down ? (
        <Primitive.ScrollAreaScrollbar
          className={props(styles.scrollbar, styles.scrollbarVertical).className}
          orientation="vertical"
        >
          <div {...props(styles.thumb, styles.thumbVertical)} />
        </Primitive.ScrollAreaScrollbar>
      ) : null}
      {across ? (
        <Primitive.ScrollAreaScrollbar
          className={props(styles.scrollbar, styles.scrollbarHorizontal).className}
          orientation="horizontal"
        >
          <div {...props(styles.thumb, styles.thumbHorizontal)} />
        </Primitive.ScrollAreaScrollbar>
      ) : null}
    </Primitive.ScrollAreaRoot>
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
