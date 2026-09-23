"use client";
// @flow
// DateRangePicker: two segmented date fields and a shared range calendar.
//
// `uf ui add date-range-picker` copies this component into the project. The headless
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
  root: { display: "inline-flex", flexWrap: "wrap", alignItems: "center", gap: ufTokens.space2 },
  field: {
    boxSizing: "border-box",
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    color: ufTokens.ink,
    backgroundColor: ufTokens.surface,
    borderRadius: ufTokens.radiusMd,
    display: "inline-flex",
    alignItems: "center",
    gap: ufTokens.space1,
    minHeight: "36px",
    paddingBlock: ufTokens.space1,
    paddingInline: ufTokens.space2,
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: { default: ufTokens.border, ":is([aria-invalid=true])": ufTokens.danger },
    outlineWidth: { default: "0", ":focus-within": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
    fontVariantNumeric: "tabular-nums",
  },
  trigger: {
    boxSizing: "border-box",
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    color: ufTokens.ink,
    backgroundColor: ufTokens.surface,
    borderRadius: ufTokens.radiusMd,
    display: "inline-flex",
    alignItems: "center",
    gap: ufTokens.space1,
    minHeight: "36px",
    paddingBlock: ufTokens.space1,
    paddingInline: ufTokens.space2,
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: { default: ufTokens.border, ":is([aria-invalid=true])": ufTokens.danger },
    outlineWidth: { default: "0", ":focus-within": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
    fontVariantNumeric: "tabular-nums",
    cursor: "pointer",
  },
  panel: {
    boxSizing: "border-box",
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    color: ufTokens.ink,
    backgroundColor: ufTokens.surface,
    borderRadius: ufTokens.radiusMd,
    padding: ufTokens.space3,
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    // Enter: it fades in while travelling 4px out of its trigger, from the
    // side `data-side` says it opened on, so the eye is led from the button
    // to what it opened. `durationBase` on the decelerating curve: most of
    // the distance is covered at once, so it is legible before it has
    // settled. Under reduced motion it only fades.
    "--uf-enter-x": {
      default: "0px",
      ":is([data-side=left])": "4px",
      ":is([data-side=right])": "-4px",
    },
    "--uf-enter-y": {
      default: "0px",
      ":is([data-side=top])": "4px",
      ":is([data-side=bottom])": "-4px",
    },
    opacity: { default: 1, "@starting-style": 0 },
    transform: {
      default: "none",
      "@starting-style": "translate(var(--uf-enter-x), var(--uf-enter-y))",
    },
    transitionProperty: {
      default: "opacity, transform",
      "@media (prefers-reduced-motion: reduce)": "opacity",
    },
    transitionDuration: ufTokens.durationBase,
    transitionTimingFunction: ufTokens.easingEnter,
  },
});
export component DateRangePicker(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.DateRangePicker.Root {...forwarded(rest)}>
      <div className={classNames(props(styles.root, xstyle).className, className)}>{children}</div>
    </Primitive.DateRangePicker.Root>
  );
}
export component DateRangePickerStartField(
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.DateRangePicker.StartField
      {...forwarded(rest)}
      className={classNames(props(styles.field, xstyle).className, className)}
    />
  );
}
export component DateRangePickerEndField(
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.DateRangePicker.EndField
      {...forwarded(rest)}
      className={classNames(props(styles.field, xstyle).className, className)}
    />
  );
}
export component DateRangePickerTrigger(
  children?: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.DateRangePicker.Trigger
      {...forwarded(rest)}
      className={classNames(props(styles.trigger, xstyle).className, className)}
    >
      {children}
    </Primitive.DateRangePicker.Trigger>
  );
}
export component DateRangePickerCalendar(
  children?: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.DateRangePicker.Calendar
      {...forwarded(rest)}
      className={classNames(props(styles.panel, xstyle).className, className)}
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
    </Primitive.DateRangePicker.Calendar>
  );
}

function forwarded(rest: Rest): $FlowFixMe {
  return rest;
}
function classNames(...names: $ReadOnlyArray<?string>): string | void {
  const present = names.filter((name) => name != null && name !== "");
  return present.length === 0 ? undefined : present.join(" ");
}
