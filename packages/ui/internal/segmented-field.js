// @flow
"use client";
import * as React from "@uniflowed/react";
import { useMemo, useState } from "@uniflowed/react";
import { useStableCallback } from "@uniflowed/hooks/lifecycle";
import { Temporal } from "@uniflowed/core/temporal";
import { useControlled } from "./controlled-state.js";
import { composeHandlers, withProps } from "./merge-props.js";
import type { RenderProp, Rest } from "./merge-props.js";
import { directionOf } from "./roving-focus.js";
import { useLocale } from "../i18n-provider.js";
import { visuallyHiddenStyle } from "./visually-hidden-style.js";

type Segment = "year" | "month" | "day" | "hour" | "minute" | "second" | "dayPeriod";
type Fields = { [string]: string };
export type DateFieldProps = {
  value?: string | null,
  defaultValue?: string | null,
  onValueChange?: (value: string | null) => void,
  minValue?: string,
  isDateUnavailable?: (value: string) => boolean,
  maxValue?: string,
  disabled?: boolean,
  readOnly?: boolean,
  required?: boolean,
  granularity?: "minute" | "second",
  hourCycle?: "h12" | "h23",
  segmentLabels?: { [string]: string },
  onValidationChange?: (invalid: boolean) => void,
  render?: RenderProp,
  readonly key?: empty,
  readonly [string]: mixed,
};
function names(time: boolean, seconds: boolean): Array<Segment> {
  return time
    ? seconds
      ? ["hour", "minute", "second"]
      : ["hour", "minute"]
    : ["year", "month", "day"];
}
function fieldsFor(value: string | null, time: boolean, seconds: boolean): Fields {
  if (value == null) return {};
  const parsed = time ? Temporal.PlainTime.from(value) : Temporal.PlainDate.from(value);
  const fields: Fields = {};
  for (const name of names(time, seconds)) fields[name] = String((parsed as $FlowFixMe)[name]);
  return fields;
}
function serialize(fields: Fields, time: boolean, seconds: boolean): string | null {
  const parts = names(time, seconds);
  if (parts.some((part) => !/^\d+$/.test(fields[part] ?? ""))) return null;
  try {
    if (time) {
      const value = `${fields.hour.padStart(2, "0")}:${fields.minute.padStart(2, "0")}:${seconds ? fields.second.padStart(2, "0") : "00"}`;
      const parsed = Temporal.PlainTime.from(value);
      return parsed.toString().slice(0, seconds ? 8 : 5);
    }
    const year = Number(fields.year),
      month = Number(fields.month),
      day = Number(fields.day);
    if (year < 1 || year > 9999) return null;
    const parsed = Temporal.PlainDate.from({ year, month, day });
    if (parsed.year !== year || parsed.month !== month || parsed.day !== day) return null;
    return parsed.toString();
  } catch {
    return null;
  }
}
function maximum(name: Segment, fields: Fields): number {
  if (name === "year") return 9999;
  if (name === "month") return 12;
  if (name === "hour") return 23;
  if (name !== "day") return 59;
  try {
    return Temporal.PlainDate.from({
      year: Number(fields.year) || 2000,
      month: Number(fields.month) || 1,
      day: 1,
    }).daysInMonth;
  } catch {
    return 31;
  }
}

/** Segment order, digits and labels come from the locale; the stored value is ISO. */
export component SegmentedField(time: boolean, options: DateFieldProps) {
  const {
    value,
    defaultValue = null,
    onValueChange,
    minValue,
    isDateUnavailable,
    maxValue,
    disabled = false,
    readOnly = false,
    required = false,
    granularity = "minute",
    hourCycle,
    segmentLabels,
    onValidationChange,
    render,
    ...rest
  } = options;
  const { locale } = useLocale();
  const seconds = granularity === "second";
  const [current, setCurrent] = useControlled(value, defaultValue, onValueChange);
  const [draft, setDraft] = useState<Fields | null>(null);
  const [announcement, announce] = useState("");
  const fields = draft ?? fieldsFor(current, time, seconds);
  const empty = names(time, seconds).every((part) => !fields[part]);
  const serialized = serialize(fields, time, seconds);
  const minimum =
    minValue == null ? null : serialize(fieldsFor(minValue, time, seconds), time, seconds);
  const maximumValue =
    maxValue == null ? null : serialize(fieldsFor(maxValue, time, seconds), time, seconds);
  if (minimum != null && maximumValue != null && minimum > maximumValue)
    throw new RangeError("DateField minimum exceeds maximum");
  const invalid =
    (empty ? required : serialized == null) ||
    (serialized != null &&
      ((minimum != null && serialized < minimum) ||
        (maximumValue != null && serialized > maximumValue) ||
        isDateUnavailable?.(serialized) === true));
  const digits = useMemo(() => new Intl.NumberFormat(locale, { useGrouping: false }), [locale]);
  const labels: $FlowFixMe = useMemo(
    () => new (Intl as $FlowFixMe).DisplayNames(locale, { type: "dateTimeField" }),
    [locale],
  );
  const parts: Array<{ type: string, value: string }> = useMemo(() => {
    const formatter: $FlowFixMe = new Intl.DateTimeFormat(
      locale,
      time
        ? {
            hour: "2-digit",
            minute: "2-digit",
            second: seconds ? "2-digit" : undefined,
            hourCycle,
            timeZone: "UTC",
          }
        : {
            year: "numeric",
            month: "2-digit",
            day: "2-digit",
            calendar: "gregory",
            timeZone: "UTC",
          },
    );
    return formatter.formatToParts(new Date("2000-01-02T12:34:56Z"));
  }, [locale, time, seconds, hourCycle]);
  const hour12 = time && parts.some((part) => part.type === "dayPeriod");
  const periodNames: Array<string> = useMemo(() => {
    const formatter: $FlowFixMe = new Intl.DateTimeFormat(locale, {
      hour: "numeric",
      hour12: true,
      timeZone: "UTC",
    });
    return [0, 12].map(
      (hour) =>
        formatter
          .formatToParts(new Date(Date.UTC(2000, 0, 1, hour)))
          .find((part) => part.type === "dayPeriod")?.value ?? (hour === 0 ? "AM" : "PM"),
    );
  }, [locale]);
  const encode = (text: string): string =>
    Array.from(text)
      .map((digit) => digits.format(Number(digit)))
      .join("");
  const decode = (text: string): string => {
    let result = text;
    for (let digit = 0; digit < 10; digit += 1)
      result = result.split(digits.format(digit)).join(String(digit));
    return result.replace(/[^0-9]/g, "");
  };
  const commit = useStableCallback(() => {
    if (disabled || readOnly) return;
    onValidationChange?.(invalid);
    if (invalid) {
      announce("Enter a valid value within the allowed range");
      return;
    }
    setCurrent(empty ? null : serialized);
    setDraft(null);
    announce(serialized ?? "Cleared");
  });
  const edit = useStableCallback((part: Segment, text: string) => {
    if (!disabled && !readOnly) setDraft({ ...fields, [part]: text });
  });
  const keyboard = useStableCallback((event: $FlowFixMe, part: Segment) => {
    if (disabled || readOnly) return;
    const key = event.key;
    if (key === "Enter") {
      event.preventDefault();
      commit();
      return;
    }
    if (key === "ArrowLeft" || key === "ArrowRight") {
      event.preventDefault();
      const root = event.currentTarget.closest("[data-segmented]");
      const segments = Array.from(root.querySelectorAll("[data-segment]"));
      const at = segments.indexOf(event.currentTarget);
      const forward =
        key === (directionOf(event.currentTarget) === "rtl" ? "ArrowLeft" : "ArrowRight");
      (segments[at + (forward ? 1 : -1)] as $FlowFixMe)?.focus();
      return;
    }
    if (!["ArrowUp", "ArrowDown", "PageUp", "PageDown", "Home", "End"].includes(key)) return;
    event.preventDefault();
    const min = time && !(part === "hour" && hour12) ? 0 : 1,
      max = part === "hour" && hour12 ? 12 : maximum(part, fields);
    const raw = Number(fields[part] || min);
    const current = part === "hour" && hour12 ? ((raw + 11) % 12) + 1 : raw;
    const next =
      key === "Home"
        ? min
        : key === "End"
          ? max
          : current +
            (key === "ArrowUp" ? 1 : key === "ArrowDown" ? -1 : key === "PageUp" ? 10 : -10);
    const bounded = next > max ? min : next < min ? max : next;
    const adjusted =
      part === "hour" && hour12 ? (bounded % 12) + (Number(fields.hour) >= 12 ? 12 : 0) : bounded;
    const nextFields = { ...fields, [part]: String(adjusted) };
    if (!time && (part === "month" || part === "year") && nextFields.day)
      nextFields.day = String(Math.min(Number(nextFields.day), maximum("day", nextFields)));
    setDraft(nextFields);
  });
  const segments = parts.map((part, index) => {
    if (part.type === "dayPeriod") {
      const pm = Number(fields.hour) >= 12;
      const toggle = () => edit("hour", String((Number(fields.hour) || 0) + (pm ? -12 : 12)));
      return (
        <span
          key="period"
          role="spinbutton"
          tabIndex={disabled ? -1 : 0}
          data-segment="dayPeriod"
          aria-label={segmentLabels?.dayPeriod ?? labels.of("dayPeriod")}
          aria-valuemin={0}
          aria-valuemax={1}
          aria-valuenow={pm ? 1 : 0}
          aria-valuetext={periodNames[pm ? 1 : 0]}
          aria-disabled={disabled || undefined}
          aria-readonly={readOnly || undefined}
          onClick={toggle}
          onKeyDown={(event) => {
            if (disabled || readOnly) return;
            if (["ArrowUp", "ArrowDown", "Home", "End", " "].includes(event.key)) {
              event.preventDefault();
              if ((event.key !== "Home" || pm) && (event.key !== "End" || !pm)) toggle();
            } else if (["ArrowLeft", "ArrowRight", "Enter"].includes(event.key))
              keyboard(event, "dayPeriod");
          }}
        >
          {periodNames[pm ? 1 : 0]}
        </span>
      );
    }

    if (!names(time, seconds).includes(part.type as $FlowFixMe))
      return (
        <span key={index} aria-hidden="true">
          {part.value}
        </span>
      );
    const name: Segment = part.type as $FlowFixMe;
    const raw = fields[name] ?? "";
    const text =
      name === "hour" && hour12 && raw !== "" ? String(((Number(raw) + 11) % 12) + 1) : raw;
    return (
      <input
        key={name}
        type="text"
        role="spinbutton"
        inputMode="numeric"
        data-segment={name}
        aria-label={segmentLabels?.[name] ?? labels.of(name)}
        aria-valuemin={time && !(name === "hour" && hour12) ? 0 : 1}
        aria-valuemax={name === "hour" && hour12 ? 12 : maximum(name, fields)}
        aria-valuenow={text ? Number(text) : undefined}
        aria-valuetext={text ? encode(text) : undefined}
        aria-invalid={invalid || undefined}
        disabled={disabled}
        readOnly={readOnly}
        required={required}
        maxLength={name === "year" ? 4 : 2}
        placeholder={name === "year" ? "yyyy" : "--"}
        value={text ? encode(text) : ""}
        onChange={(event) => {
          const text = decode(event.currentTarget.value);
          edit(
            name,
            name === "hour" && hour12 && text && Number(text) <= 12
              ? String((Number(text) % 12) + (Number(fields.hour) >= 12 ? 12 : 0))
              : text,
          );
        }}
        onKeyDown={(event) => keyboard(event, name)}
      />
    );
  });
  const props = withProps(rest, {
    role: "group",
    "data-segmented": time ? "time" : "date",
    "aria-invalid": invalid || rest["aria-invalid"],
    onBlur: composeHandlers(rest.onBlur, (event: $FlowFixMe) => {
      if (!event.currentTarget.contains(event.relatedTarget)) commit();
    }),
    children: (
      <>
        {segments}
        <span role="status" aria-live="polite" style={visuallyHiddenStyle}>
          {announcement}
        </span>
      </>
    ),
  });
  return render != null ? render(props) : <div {...props} />;
}
