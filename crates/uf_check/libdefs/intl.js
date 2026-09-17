/**
 * @fileoverview TypeScript's Intl namespace option names, typed for Flow.
 *
 * Flow's own `intl.js` and `core.js` type the constructors and their `$`
 * option aliases, such as `Intl$DateTimeFormatOptions`, but TypeScript
 * declaration files spell those same surfaces as namespace members:
 * `Intl.DateTimeFormatOptions`, `Intl.ResolvedDateTimeFormatOptions` and
 * `Intl.RelativeTimeFormatOptions`. A translated `.d.ts` that names one of
 * those members should not fall back to Flow's recovery type just because the
 * JavaScript runtime API already existed under another Flow-only name. See
 * ubugeeei-prod/uf#1089.
 */

declare namespace Intl {
  declare type LocaleMatcher = "lookup" | "best fit";

  declare type DateTimeFormatOptions = Intl$DateTimeFormatOptions & {
    calendar?: string,
    dateStyle?: "full" | "long" | "medium" | "short",
    dayPeriod?: "narrow" | "short" | "long",
    fractionalSecondDigits?: 1 | 2 | 3,
    hourCycle?: "h11" | "h12" | "h23" | "h24",
    numberingSystem?: string,
    timeStyle?: "full" | "long" | "medium" | "short",
    ...
  };

  declare type ResolvedDateTimeFormatOptions = {
    locale: string,
    calendar: string,
    numberingSystem: string,
    timeZone?: string,
    hour12: boolean,
    weekday?: "narrow" | "short" | "long",
    era?: "narrow" | "short" | "long",
    year?: "numeric" | "2-digit",
    month?: "numeric" | "2-digit" | "narrow" | "short" | "long",
    day?: "numeric" | "2-digit",
    hour?: "numeric" | "2-digit",
    minute?: "numeric" | "2-digit",
    second?: "numeric" | "2-digit",
    timeZoneName?: "short" | "long",
    ...
  };

  declare type RelativeTimeFormatOptions = {
    localeMatcher?: LocaleMatcher,
    numeric?: "always" | "auto",
    style?: "long" | "short" | "narrow",
    ...
  };

  declare type ResolvedRelativeTimeFormatOptions = {
    locale: string,
    numberingSystem: string,
    style: "long" | "short" | "narrow",
    numeric: "always" | "auto",
    ...
  };
}
