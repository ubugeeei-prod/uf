"use client";
// @flow
// VisuallyHidden: text for a screen reader, and a skip link that appears on focus.
//
// `uf ui add visually-hidden` copies this component into the project. The
// headless primitive owns the hiding — a clipped one-pixel box that stays in the
// accessibility tree — and this file owns only what a `focusable` one looks like
// while a keyboard is on it: a skip link pinned to the top of the page.
// Keep the focus ring and the contrast when editing; that state is the whole
// reason this file has styles.
import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import * as Primitive from "@uniflowed/ui";

type Rest = { readonly key?: empty, readonly [string]: mixed };
const styles = stylex.create({
  shown: {
    position: "fixed",
    insetBlockStart: ufTokens.space2,
    insetInlineStart: ufTokens.space2,
    zIndex: 50,
    paddingBlock: ufTokens.space2,
    paddingInline: ufTokens.space3,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    color: ufTokens.ink,
    backgroundColor: ufTokens.surface,
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderRadius: ufTokens.radiusSm,
    outlineWidth: "2px",
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
  },
});
export component VisuallyHidden(
  children?: React.Node,
  focusable?: boolean = false,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.VisuallyHidden
      {...forwarded(rest)}
      focusable={focusable}
      // Only visible while a `focusable` one holds focus; hidden, the
      // primitive's inline style wins over any class.
      className={classNames(props(styles.shown, xstyle).className, className)}
    >
      {children}
    </Primitive.VisuallyHidden>
  );
}

function forwarded(rest: Rest): $FlowFixMe {
  return rest;
}
function classNames(...names: $ReadOnlyArray<?string>): string | void {
  const present = names.filter((name) => name != null && name !== "");
  return present.length === 0 ? undefined : present.join(" ");
}
