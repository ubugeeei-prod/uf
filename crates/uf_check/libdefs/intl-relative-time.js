/**
 * @fileoverview `Intl.RelativeTimeFormat`, which Flow's vendored `intl.js`
 * leaves out of `Intl`.
 *
 * ECMA-402 has specified `Intl.RelativeTimeFormat` since 2020, and every
 * runtime uf targets ships it. Flow's `intl.js` declares `Intl` with
 * `Collator`, `DateTimeFormat`, `Locale`, `NumberFormat`, `PluralRules` and
 * `Segmenter`, and no `RelativeTimeFormat`, so `new Intl.RelativeTimeFormat()`
 * was "property RelativeTimeFormat is missing" (ubugeeei-prod/uf#1451).
 *
 * # How this replaces the vendored one
 *
 * `Intl` is a `declare var`, and a later one shadows an earlier one, so the
 * value is repeated **whole**: the members are `intl.js`'s, unchanged, plus
 * `RelativeTimeFormat`. This file is listed after `intl.js` and **before**
 * `uf-intl.js`: `Intl.RelativeTimeFormatOptions` and the rest of the
 * namespace types that file declares are looked up on whichever `Intl` was
 * declared last, and a `declare var Intl` after the namespace would hide them.
 */

declare var Intl: {
  Collator: Class<Intl$Collator>,
  DateTimeFormat: Class<Intl$DateTimeFormat>,
  Locale: Class<Intl$LocaleClass>,
  NumberFormat: Class<Intl$NumberFormat>,
  PluralRules: ?Class<Intl$PluralRules>,
  RelativeTimeFormat: Class<Intl$RelativeTimeFormat>,
  Segmenter: Class<Intl$Segmenter>,
  getCanonicalLocales?: (locales?: Intl$Locales) => Intl$Locale[],
  ...
};

/** The units `format` takes, singular and plural, as ECMA-402 lists them. */
type Intl$RelativeTimeFormatUnit =
  | "year"
  | "years"
  | "quarter"
  | "quarters"
  | "month"
  | "months"
  | "week"
  | "weeks"
  | "day"
  | "days"
  | "hour"
  | "hours"
  | "minute"
  | "minutes"
  | "second"
  | "seconds";

type Intl$RelativeTimeFormatOptions = {
  localeMatcher?: "lookup" | "best fit",
  numeric?: "always" | "auto",
  style?: "long" | "short" | "narrow",
  ...
};

/** One piece of a formatted string: literal text, or the number and its unit. */
type Intl$RelativeTimeFormatPart =
  | { type: "literal", value: string, ... }
  | { type: string, value: string, unit: string, ... };

declare class Intl$RelativeTimeFormat {
  constructor(locales?: Intl$Locales, options?: Intl$RelativeTimeFormatOptions): void;
  format(value: number, unit: Intl$RelativeTimeFormatUnit): string;
  formatToParts(value: number, unit: Intl$RelativeTimeFormatUnit): Array<Intl$RelativeTimeFormatPart>;
  resolvedOptions(): {
    locale: string,
    numberingSystem: string,
    style: "long" | "short" | "narrow",
    numeric: "always" | "auto",
    ...
  };
  static supportedLocalesOf(
    locales?: Intl$Locales,
    options?: { localeMatcher?: "lookup" | "best fit", ... },
  ): Array<string>;
}
