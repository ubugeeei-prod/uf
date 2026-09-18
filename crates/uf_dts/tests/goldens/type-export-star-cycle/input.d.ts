import type { Locale } from "./locale/types.js";
export type * from "./locale/types.js";

export interface LocalizedOptions<LocaleFields extends keyof Locale> {
  locale?: Pick<Locale, LocaleFields>;
}
