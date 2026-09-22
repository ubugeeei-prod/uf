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
import * as Primitive from "@uniflowed/ui";

type Rest = { readonly key?: empty, readonly [string]: mixed };
import { CalendarHeader, CalendarMonth, CalendarNext, CalendarPrevious } from "./calendar.js";
const styles = stylex.create({
  root: {
    boxSizing: "border-box",
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    color: ufTokens.ink,
    backgroundColor: ufTokens.surface,
    borderRadius: ufTokens.radiusSm,
    display: "inline-block",
    padding: ufTokens.space3,
  },
});
export component RangeCalendar(
  children?: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.RangeCalendarRoot
      {...forwarded(rest)}
      className={classNames(props(styles.root, xstyle).className, className)}
    >
      {children ?? (
        <>
          <CalendarHeader>
            <CalendarPrevious />
            <CalendarNext />
          </CalendarHeader>
          <CalendarMonth />
        </>
      )}
    </Primitive.RangeCalendarRoot>
  );
}

function forwarded(rest: Rest): $FlowFixMe {
  return rest;
}
function classNames(...names: $ReadOnlyArray<?string>): string | void {
  const present = names.filter((name) => name != null && name !== "");
  return present.length === 0 ? undefined : present.join(" ");
}
