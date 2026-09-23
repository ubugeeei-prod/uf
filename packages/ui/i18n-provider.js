// @flow
"use client";

import * as React from "@uniflowed/react";
import { createContext, useContext, useMemo } from "@uniflowed/react";
import type { RenderProp, Rest } from "./internal/merge-props.js";
import { withProps } from "./internal/merge-props.js";
import { MessagesContext } from "./internal/messages.js";
import type { MessageOverrides, UiMessages } from "./internal/messages.js";

export type Locale = {| readonly locale: string, readonly direction: "ltr" | "rtl" |};
const LocaleContext: React.Context<Locale> = createContext({ locale: "en-US", direction: "ltr" });

/**
 * An explicit locale keeps server and client output identical. Nested providers form islands.
 *
 * `messages` replaces any of the strings the components say on their own —
 * announcements such as "3 selected" and default labels such as "Increase" —
 * for this subtree. English and Japanese are built in; any other language
 * falls back to English until it is given here. A nested provider that keeps
 * the language inherits the overrides above it, and one that changes language
 * starts again from that language's built-in table.
 */
export component I18nProvider(
  children: React.Node,
  locale?: string,
  direction?: "ltr" | "rtl",
  messages?: Partial<UiMessages>,
  render?: RenderProp,
  ...rest: Rest
) {
  const parent = useLocale();
  const inherited = useContext(MessagesContext);
  const state = useMemo((): Locale => {
    const resolved = new Intl.Locale(locale ?? parent.locale);
    const script = resolved.maximize().script;
    const rtl = ["Arab", "Hebr", "Thaa", "Nkoo", "Adlm", "Rohg"].includes(script ?? "");
    return {
      locale: resolved.toString(),
      direction: direction ?? (locale == null ? parent.direction : rtl ? "rtl" : "ltr"),
    };
  }, [locale, direction, parent]);
  const language = new Intl.Locale(state.locale).language;
  const kept = inherited.language == null || inherited.language === language;
  const overrides: MessageOverrides = {
    language,
    messages: kept ? { ...inherited.messages, ...messages } : { ...messages },
  };
  const props = withProps(rest, { children, lang: state.locale, dir: state.direction });
  return (
    <LocaleContext.Provider value={state}>
      <MessagesContext.Provider value={overrides}>
        {render != null ? render(props) : <div {...props} />}
      </MessagesContext.Provider>
    </LocaleContext.Provider>
  );
}

export hook useLocale(): Locale {
  return useContext(LocaleContext);
}

/** Share Intl's language-specific ordering with caller-owned collections. */
export hook useCollator(options?: Intl$CollatorOptions): Intl$Collator {
  const { locale } = useLocale();
  return useMemo(() => new Intl.Collator(locale, options), [locale, options]);
}

// Bound the cache when a long-lived server serves many requested locales.
const searchCollators = new Map<string, Intl$Collator>();
function searchCollator(locale: string): Intl$Collator {
  const cached = searchCollators.get(locale);
  if (cached != null) return cached;
  const collator = new Intl.Collator(locale, { usage: "search", sensitivity: "base" });
  if (searchCollators.size >= 32) searchCollators.clear();
  searchCollators.set(locale, collator);
  return collator;
}

export function startsWithLocale(text: string, query: string, locale: string): boolean {
  const value = text.normalize("NFC");
  const needle = query.normalize("NFC");
  return (
    searchCollator(locale).compare(
      Array.from(value).slice(0, Array.from(needle).length).join(""),
      needle,
    ) === 0
  );
}

/** Filtering uses the same collation as typeahead; consumers own the result list. */
export hook useFilter(): {
  startsWith: (text: string, query: string) => boolean,
  contains: (text: string, query: string) => boolean,
} {
  const { locale } = useLocale();
  return useMemo(
    () => ({
      startsWith: (text, query) => startsWithLocale(text, query, locale),
      contains: (text, query) => {
        const chars = Array.from(text.normalize("NFC"));
        return (
          query === "" ||
          chars.some((_, index) => startsWithLocale(chars.slice(index).join(""), query, locale))
        );
      },
    }),
    [locale],
  );
}
