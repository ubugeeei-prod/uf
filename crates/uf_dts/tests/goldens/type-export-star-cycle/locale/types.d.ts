import type { LocalizedOptions } from "../input.js";

export interface FormatDistanceOptions {
  addSuffix?: boolean;
}

export interface Locale {
  code: string;
  formatDistance: FormatDistanceOptions;
  options?: LocalizedOptions<"formatDistance">;
}
