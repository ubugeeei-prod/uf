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
import { DateRangePicker } from "@uniflowed/ui";

type Rest = { readonly key?: empty, readonly [string]: mixed };
import * as Calendar from "./calendar.js";
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
    // Exit: back towards the trigger it came from, on the accelerating curve
    // and in `durationFast` against the entrance's `durationBase`, so it gets
    // out of the way rather than lingering. `@uniflowed/ui` keeps it on the
    // page, closed and `inert`, until this has finished. Under reduced motion
    // it only fades: `--uf-exit-travel` is 0 there, so nothing jumps either.
    "--uf-exit-travel": { default: "1", "@media (prefers-reduced-motion: reduce)": "0" },
    opacity: { default: 1, "@starting-style": 0, ":is([data-state=closed])": 0 },
    transform: {
      default: "none",
      "@starting-style": "translate(var(--uf-enter-x), var(--uf-enter-y))",
      ":is([data-state=closed])":
        "translate(calc(var(--uf-enter-x) * var(--uf-exit-travel)), calc(var(--uf-enter-y) * var(--uf-exit-travel)))",
    },
    transitionProperty: {
      default: "opacity, transform",
      "@media (prefers-reduced-motion: reduce)": "opacity",
    },
    transitionDuration: {
      default: ufTokens.durationBase,
      ":is([data-state=closed])": ufTokens.durationFast,
    },
    transitionTimingFunction: {
      default: ufTokens.easingEnter,
      ":is([data-state=closed])": ufTokens.easingExit,
    },
  },
});
component DateRangePickerRoot(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <DateRangePicker.Root {...forwarded(rest)}>
      <div className={classNames(props(styles.root, xstyle).className, className)}>{children}</div>
    </DateRangePicker.Root>
  );
}
component DateRangePickerStartField(xstyle?: StyleArgument, className?: string, ...rest: Rest) {
  return (
    <DateRangePicker.StartField
      {...forwarded(rest)}
      className={classNames(props(styles.field, xstyle).className, className)}
    />
  );
}
component DateRangePickerEndField(xstyle?: StyleArgument, className?: string, ...rest: Rest) {
  return (
    <DateRangePicker.EndField
      {...forwarded(rest)}
      className={classNames(props(styles.field, xstyle).className, className)}
    />
  );
}
component DateRangePickerTrigger(
  children?: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <DateRangePicker.Trigger
      {...forwarded(rest)}
      className={classNames(props(styles.trigger, xstyle).className, className)}
    >
      {children}
    </DateRangePicker.Trigger>
  );
}
component DateRangePickerCalendar(
  children?: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <DateRangePicker.Calendar
      {...forwarded(rest)}
      className={classNames(props(styles.panel, xstyle).className, className)}
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
    </DateRangePicker.Calendar>
  );
}

function forwarded(rest: Rest): $FlowFixMe {
  return rest;
}
function classNames(...names: $ReadOnlyArray<?string>): string | void {
  const present = names.filter((name) => name != null && name !== "");
  return present.length === 0 ? undefined : present.join(" ");
}

/**
 * The parts, under the names `import * as DateRangePicker from "./date-range-picker.js"` gives them.
 *
 * One name per component (ubugeeei-prod/uf#1453): a page writes `<DateRangePicker.Root>`
 * and `<DateRangePicker.StartField>`, the way it writes `@uniflowed/ui`'s own parts. Each
 * is declared under its full name, so React DevTools and an error say
 * `DateRangePickerRoot` rather than `Root`.
 */
export {
  DateRangePickerRoot as Root,
  DateRangePickerStartField as StartField,
  DateRangePickerEndField as EndField,
  DateRangePickerTrigger as Trigger,
  DateRangePickerCalendar as Calendar,
};
