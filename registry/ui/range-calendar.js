"use client";
// @flow
// RangeCalendar: an inclusive date range on the shared styled calendar.
//
// `uf ui add range-calendar` copies this component into the project. The headless
// primitive owns semantics and input behavior; this file owns its presentation.
// Keep accessible names, focus rings and disabled/selected states when editing.
import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import { RangeCalendar as HeadlessRangeCalendar } from "@uniflowed/ui";

type Rest = { readonly key?: empty, readonly [string]: mixed };
import * as Calendar from "./calendar.js";
const styles = stylex.create({
  // The same frame as `Calendar`'s. A block, because the month buttons hang
  // on the caption's line from a zero-height row; `calendar.js` says why.
  root: {
    display: "inline-block",
    boxSizing: "border-box",
    padding: ufTokens.space3,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    color: ufTokens.ink,
    backgroundColor: ufTokens.surface,
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderRadius: ufTokens.radiusMd,
  },
});
export component RangeCalendar(
  children?: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <HeadlessRangeCalendar.Root
      {...forwarded(rest)}
      className={classNames(props(styles.root, xstyle).className, className)}
    >
      {children ?? (
        <>
          <Calendar.Header>
            <Calendar.Previous />
            <Calendar.Next />
          </Calendar.Header>
          <Calendar.Month />
        </>
      )}
    </HeadlessRangeCalendar.Root>
  );
}

function forwarded(rest: Rest): $FlowFixMe {
  return rest;
}
function classNames(...names: $ReadOnlyArray<?string>): string | void {
  const present = names.filter((name) => name != null && name !== "");
  return present.length === 0 ? undefined : present.join(" ");
}
