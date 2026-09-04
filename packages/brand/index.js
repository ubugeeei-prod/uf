// @flow
//
// `@uniflowed/brand`.

export type BrandColorToken = {
  readonly name: string,
  readonly token: string,
  readonly value: string,
};

export type BrandScaleToken = {
  readonly token: string,
  readonly value: string,
};

export const ufBrand = {
  name: "uf",
  fullName: "uniflowed",
  headline: "Unified Toolchain for Flow",
  tagline: "All-in-one toolchain for Flow and React.",
};

export const ufBrandAssets = {
  sourceMark: "brand/uf.png",
  primaryLogo: "brand/uniflowed-logo.png",
  logo: "brand/uniflowed-logo.png",
  logoSvg: "brand/uniflowed-logo.svg",
  mark: "brand/uniflowed-mark.png",
  markSvg: "brand/uniflowed-mark.svg",
  wordmark: "brand/uniflowed-wordmark.png",
  wordmarkSvg: "brand/uniflowed-wordmark.svg",
  favicon: "brand/favicon.svg",
};

export const ufPalette: $ReadOnlyArray<BrandColorToken> = [
  { name: "Primary Cyan", token: "--uf-color-cyan-500", value: "#35D6F6" },
  { name: "Electric Blue", token: "--uf-color-blue-500", value: "#2677FF" },
  { name: "Indigo", token: "--uf-color-indigo-500", value: "#5C49FF" },
  { name: "Violet", token: "--uf-color-violet-500", value: "#8F4BFF" },
  { name: "Magenta", token: "--uf-color-magenta-500", value: "#D84BFF" },
  { name: "Ink", token: "--uf-color-ink-900", value: "#0F172A" },
  { name: "Slate", token: "--uf-color-slate-600", value: "#475569" },
  { name: "Mist", token: "--uf-color-mist-50", value: "#F8FAFC" },
];

export const ufTextScale: $ReadOnlyArray<BrandScaleToken> = [
  { token: "--uf-text-xs", value: "12px" },
  { token: "--uf-text-sm", value: "14px" },
  { token: "--uf-text-md", value: "16px" },
  { token: "--uf-text-lg", value: "20px" },
  { token: "--uf-text-xl", value: "28px" },
  { token: "--uf-text-2xl", value: "40px" },
];

export const ufSpacingScale: $ReadOnlyArray<BrandScaleToken> = [
  { token: "--uf-space-1", value: "4px" },
  { token: "--uf-space-2", value: "8px" },
  { token: "--uf-space-3", value: "12px" },
  { token: "--uf-space-4", value: "16px" },
  { token: "--uf-space-6", value: "24px" },
  { token: "--uf-space-8", value: "32px" },
  { token: "--uf-space-12", value: "48px" },
];

export const ufRadiusScale: $ReadOnlyArray<BrandScaleToken> = [
  { token: "--uf-radius-sm", value: "8px" },
  { token: "--uf-radius-md", value: "12px" },
  { token: "--uf-radius-lg", value: "16px" },
  { token: "--uf-radius-xl", value: "24px" },
  { token: "--uf-radius-pill", value: "999px" },
];

export const ufCommands = {
  curl: "curl -fsSL https://setup.uniflowed.dev | sh",
  nixRun: "nix run github:ubugeeei-prod/uf#uf -- --version",
  nixProfile: "nix profile install github:ubugeeei-prod/uf#uf",
  info: "uf info",
};
