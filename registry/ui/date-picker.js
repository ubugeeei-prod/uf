"use client";
// @flow
//
// Date picker: a text field for a date, with a calendar that opens beside it to
// choose one from.
//
// `uf ui add date-picker` wrote this file into the project, and it is the
// project's from then on. `uf ui diff date-picker` shows how it has moved away
// from the registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The field, the button that opens the calendar, and the panel the calendar
// shows in; the calendar itself is `calendar.js`'s. `@uniflowed/ui`'s `DatePicker`
// owns the rest: a real text field a reader can type a date into, read by
// `parse`, written by `format` and marked `aria-invalid` while it does not hold
// a date, a popover that opens from the button, and the field and the calendar
// kept to the same date. Every prop of the part's root, such as `value`,
// `locale`, `parse` and `format`, passes through `DatePicker` unchanged.
//
// # What to keep true when you change it
//
// * **Typing stays possible.** The calendar is a second way to enter a date,
//   not the only one.
// * **Say the format.** Text under the field, tied to it with
//   `aria-describedby`, tells a reader what `parse` accepts.
// * **Name the field.** A `<label>` for the input; the button's `label` says
//   what it opens.
// * **Text stays on measured pairs.** `ink` and `muted` on `surface`, which
//   `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1 in both themes. An
//   invalid field's border is `danger`.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import * as Primitive from "@uniflowed/ui";

import type { ButtonSize, ButtonTone } from "./button.js";
import { Button } from "./button.js";
import { CalendarHeader, CalendarMonth, CalendarNext, CalendarPrevious } from "./calendar.js";

/** Every prop a caller passes that this file does not name, for the part. */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  group: {
    display: "inline-flex",
    alignItems: "center",
    gap: ufTokens.space2,
  },
  input: {
    boxSizing: "border-box",
    width: "11rem",
    minHeight: "36px",
    margin: 0,
    paddingBlock: ufTokens.space2,
    paddingInline: ufTokens.space3,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    fontVariantNumeric: "tabular-nums",
    color: ufTokens.ink,
    backgroundColor: ufTokens.surface,
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: { default: ufTokens.border, ":is([aria-invalid=true])": ufTokens.danger },
    borderRadius: ufTokens.radiusMd,
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "1px",
  },
  // A block, not a grid: the month buttons hang on the caption's line from a
  // zero-height row, and a grid's gap would push the caption off it.
  content: {
    zIndex: 50,
    display: "block",
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

/** The date picker. Every prop of `DatePicker.Root` passes through. */
export component DatePicker(children: React.Node, ...rest: Rest) {
  return <Primitive.DatePickerRoot {...forwarded(rest)}>{children}</Primitive.DatePickerRoot>;
}

/** The field and its button, side by side. */
export component DatePickerGroup(children: React.Node, xstyle?: StyleArgument, className?: string) {
  return (
    <div className={classNames(props(styles.group, xstyle).className, className)}>{children}</div>
  );
}

/** The text field the date is typed into. Give it an `id` a `<label>` points at. */
export component DatePickerInput(xstyle?: StyleArgument, className?: string, ...rest: Rest) {
  return (
    <Primitive.DatePickerInput
      {...forwarded(rest)}
      className={classNames(props(styles.input, xstyle).className, className)}
    />
  );
}

/** The button that opens the calendar. It is a `Button` showing a calendar icon. */
export component DatePickerTrigger(
  label?: string = "Choose a date",
  tone?: ButtonTone = "neutral",
  size?: ButtonSize = "md",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.DatePickerTrigger
      {...forwarded(rest)}
      aria-label={label}
      render={(trigger: Rest) => (
        <Button
          {...forwarded(trigger)}
          className={className}
          size={size}
          tone={tone}
          xstyle={xstyle}
        />
      )}
    >
      <svg
        aria-hidden="true"
        fill="none"
        focusable="false"
        height="16"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="2"
        viewBox="0 0 24 24"
        width="16"
      >
        <rect height="18" rx="2" width="18" x="3" y="4" />
        <path d="M16 2v4M8 2v4M3 10h18" />
      </svg>
    </Primitive.DatePickerTrigger>
  );
}

/**
 * The panel the calendar opens in. With no children it holds the month buttons
 * and the month; give it children to lay the calendar out another way.
 */
export component DatePickerContent(
  children?: React.Node,
  sideOffset?: number = 4,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.DatePickerCalendar
      {...forwarded(rest)}
      className={classNames(props(styles.content, xstyle).className, className)}
      sideOffset={sideOffset}
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
    </Primitive.DatePickerCalendar>
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
