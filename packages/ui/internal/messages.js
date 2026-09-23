// @flow
//
// The words this package says on its own: announcements and default labels.
//
// A headless package still speaks. A collection says "3 selected", a drag says
// where the item landed, a stepper button is called "Increase". Those strings
// used to be English literals scattered through the modules, and a Japanese
// or Arabic application had no way to change the ones without a prop — a
// screen reader user got an English sentence in the middle of a page in their
// own language. React Aria ships its strings translated; this is uf's answer
// to that, in three parts:
//
//   * **One table.** Every string is a member of `UiMessages`, and a message
//     with a value in it is a function of the value, so word order is the
//     translation's business rather than the call site's: Japanese puts the
//     count first and the verb last.
//   * **Built-in translations, honestly few.** English, and Japanese. A locale
//     with no table gets English rather than a guess; there is no claim here of
//     the thirty-odd locales React Aria ships, and a table added here should be
//     one a reader of that language has checked.
//   * **Overrides through `I18nProvider`.** `messages` replaces any subset for
//     a subtree, which is how an application adds a language or changes a
//     phrase without forking a component.
//
// Numbers and dates inside a message are formatted with `Intl` in the same
// locale before the message sees them — "1,234 selected", "1.234 ausgewählt" —
// so a translation only arranges words.
//
// Internal, because the table's public face is `I18nProvider`'s `messages` and
// the `UiMessages` type; a caller has no reason to read the English wording.

import * as React from "@uniflowed/react";
import { createContext, useContext } from "@uniflowed/react";

/** A colour channel, as `ColorPicker.Channel` names it. */
export type ColorChannelName = "red" | "green" | "blue" | "alpha";

/**
 * Every string `@uniflowed/ui` produces by itself.
 *
 * `count` arguments arrive as numbers for plural choice, with the formatted
 * spelling beside them; `date`, `start`, `end` and `value` arrive formatted.
 */
export type UiMessages = {|
  /** A collection's selection changed. */
  readonly selectedCount: (count: number, formatted: string) => string,
  /** A tag was removed. */
  readonly removed: (item: string) => string,
  /** A tag's remove button. */
  readonly removeItem: (item: string) => string,
  /** A keyboard drag picked an item up. */
  readonly dragStarted: (item: string) => string,
  /** A drag ended on a target. */
  readonly dropped: (target: string) => string,
  readonly dragCancelled: string,
  /** A range calendar's first click. */
  readonly rangeStarted: (date: string) => string,
  readonly rangeSelected: (start: string, end: string) => string,
  readonly rangeUnavailable: string,
  /** A date or time field was left holding a value it cannot accept. */
  readonly fieldInvalid: string,
  readonly fieldCleared: string,
  /** A number field's stepper buttons. */
  readonly increment: string,
  readonly decrement: string,
  /** A colour channel slider's name. */
  readonly colorChannel: (channel: ColorChannelName) => string,
  /** An open combobox's result count. */
  readonly results: (count: number, formatted: string) => string,
|};

const ENGLISH: UiMessages = {
  selectedCount: (_, formatted) => `${formatted} selected`,
  removed: (item) => `Removed ${item}`,
  removeItem: (item) => `Remove ${item}`,
  dragStarted: (item) =>
    `Picked up ${item}. Move to a drop target and press Enter. Escape cancels.`,
  dropped: (target) => `Dropped on ${target}`,
  dragCancelled: "Drag cancelled",
  rangeStarted: (date) => `Start ${date}. Choose an end date.`,
  rangeSelected: (start, end) => `Selected ${start} to ${end}`,
  rangeUnavailable: "The range contains an unavailable date",
  fieldInvalid: "Enter a valid value within the allowed range",
  fieldCleared: "Cleared",
  increment: "Increase",
  decrement: "Decrease",
  colorChannel: (channel) =>
    match (channel) {
      "red" => "Red",
      "green" => "Green",
      "blue" => "Blue",
      "alpha" => "Alpha",
    },
  results: (count, formatted) =>
    count === 0
      ? "No results available."
      : count === 1
        ? "1 result available."
        : `${formatted} results available.`,
};

const JAPANESE: UiMessages = {
  selectedCount: (_, formatted) => `${formatted}件を選択中`,
  removed: (item) => `${item}を削除しました`,
  removeItem: (item) => `${item}を削除`,
  dragStarted: (item) =>
    `${item}を持ち上げました。ドロップ先に移動して Enter キーを押してください。Escape キーで取り消します。`,
  dropped: (target) => `${target}にドロップしました`,
  dragCancelled: "ドラッグを取り消しました",
  rangeStarted: (date) => `開始日は${date}です。終了日を選んでください。`,
  rangeSelected: (start, end) => `${start}から${end}までを選択しました`,
  rangeUnavailable: "選択できない日付が期間に含まれています",
  fieldInvalid: "許可された範囲の正しい値を入力してください",
  fieldCleared: "消去しました",
  increment: "増やす",
  decrement: "減らす",
  colorChannel: (channel) =>
    match (channel) {
      "red" => "赤",
      "green" => "緑",
      "blue" => "青",
      "alpha" => "不透明度",
    },
  results: (count, formatted) =>
    count === 0 ? "結果はありません。" : `${formatted}件の結果があります。`,
};

/** The built-in tables, by language subtag. */
const BUILT_IN: { readonly [language: string]: UiMessages } = { en: ENGLISH, ja: JAPANESE };

/** The languages with a built-in table, for documentation and tests. */
export const builtInLanguages: $ReadOnlyArray<string> = Object.keys(BUILT_IN);

/**
 * The built-in table for a locale: its language's, or English.
 *
 * By language rather than by full tag, because `ja-JP` and `ja` read the same
 * sentence; a regional difference is what an override is for.
 */
export function messagesFor(locale: string): UiMessages {
  let language = "en";
  try {
    language = new Intl.Locale(locale).language;
  } catch {
    // An unparseable tag gets the fallback rather than an exception mid-render.
  }
  return BUILT_IN[language] ?? ENGLISH;
}

/**
 * The overrides an `I18nProvider` above has set, and the locale they were set
 * for — a nested provider that changes language drops them, because a Japanese
 * phrase is not an override for an English island.
 */
export type MessageOverrides = {|
  readonly language: string | null,
  readonly messages: Partial<UiMessages>,
|};

export const MessagesContext: React.Context<MessageOverrides> = createContext({
  language: null,
  messages: {},
});

/** The strings for `locale`: the built-in table with any overrides above laid on top. */
export hook useMessages(locale: string): UiMessages {
  const overrides = useContext(MessagesContext);
  const base = messagesFor(locale);
  let language = "en";
  try {
    language = new Intl.Locale(locale).language;
  } catch {
    // As `messagesFor`.
  }
  if (overrides.language != null && overrides.language !== language) return base;
  return { ...base, ...overrides.messages };
}

// Formatters are cached per locale: announcements are frequent and
// `Intl.NumberFormat` is not cheap to construct. Bounded, like the collators
// in `i18n-provider.js`, for a server rendering many requested locales.
const numberFormats = new Map<string, Intl$NumberFormat>();

/** `count` in `locale`'s digits and grouping. */
export function formatCount(count: number, locale: string): string {
  let format = numberFormats.get(locale);
  if (format == null) {
    try {
      format = new Intl.NumberFormat(locale);
    } catch {
      format = new Intl.NumberFormat("en-US");
    }
    if (numberFormats.size >= 32) numberFormats.clear();
    numberFormats.set(locale, format);
  }
  return format.format(count);
}

/**
 * An ISO calendar date (`2026-09-03`) as `locale` writes it in full, for an
 * announcement: "Thursday, September 3, 2026", "2026年9月3日木曜日".
 *
 * Formatted in UTC so the day cannot move with the reader's time zone, and in
 * the Gregorian calendar the ISO string is in. A string that is not an ISO date
 * is returned as it came.
 */
export function formatIsoDate(iso: string, locale: string): string {
  const parts = /^(\d{4})-(\d{2})-(\d{2})$/.exec(iso);
  if (parts == null) return iso;
  const time = Date.UTC(Number(parts[1]), Number(parts[2]) - 1, Number(parts[3]));
  try {
    return new Intl.DateTimeFormat(locale, {
      dateStyle: "full",
      timeZone: "UTC",
      calendar: "gregory",
    }).format(time);
  } catch {
    return iso;
  }
}

/**
 * An ISO time (`14:05`, `14:05:30`) as `locale` says it, for an announcement.
 *
 * `hourCycle` is the field's, so a field showing a 24-hour clock is not
 * announced in a 12-hour one. Anything else is returned as it came.
 */
export function formatIsoTime(iso: string, locale: string, hourCycle?: "h12" | "h23"): string {
  const parts = /^(\d{2}):(\d{2})(?::(\d{2}))?$/.exec(iso);
  if (parts == null) return iso;
  const time = Date.UTC(2000, 0, 1, Number(parts[1]), Number(parts[2]), Number(parts[3] ?? 0));
  try {
    return new Intl.DateTimeFormat(locale, {
      hour: "numeric",
      minute: "2-digit",
      second: parts[3] == null ? undefined : "2-digit",
      hourCycle,
      timeZone: "UTC",
    }).format(time);
  } catch {
    return iso;
  }
}
