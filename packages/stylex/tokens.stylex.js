// @flow
//
// `@uniflowed/stylex/tokens.stylex.js`: the tokens a project gets for free.
//
// This is an ordinary `stylex.defineVars` module — the compiler binds it by its
// `.stylex.js` suffix like any other — and it is the first of the three things
// uf's preset is. Import it and every name below is a `var(--…)` the build
// already declared on `:root`; nothing here is computed in a browser, and a
// project that never imports it ships none of it.
//
// # Roles, not colours
//
// The names are what a token is *for*, never what it looks like. `accent` is
// the colour a primary action wears; it is indigo today and a theme can make it
// anything, and no rule in `preset.js` has to change when it does. A token
// named `indigo500` would have made the preset unthemeable the moment someone
// wanted a green product.
//
// The values are uf's identity, and `@uniflowed/brand` is where that identity
// is decided: `accent`, `ink`, `canvas` and `muted` are brand's Indigo, Ink,
// Mist and Slate, and the type, spacing and radius steps are brand's scales.
// Brand cannot hold the token module itself — a StyleX token's name is computed
// by the compiler, and `defineVars` takes literals, not an imported array — so
// this module is brand's projection into the semantic roles a design system
// needs and an identity does not have.
//
// # What is not a token
//
// Geometry that exactly one control uses. A switch's track is 36×20 because
// that is what a switch is, and promoting it to a token would invite a theme to
// change it into something the thumb no longer fits. A token is a decision more
// than one rule reads.
//
// # Contrast
//
// Every foreground/background pair the preset actually pairs meets WCAG AA at
// 4.5:1, in both the light default and the shipped dark theme, and
// `crates/uf_stylex/src/tests/preset.rs` computes the ratios from the compiled
// stylesheet rather than trusting this comment.

import { stylex } from "@uniflowed/stylex";

// Deliberately unannotated: `defineVars` hands back exactly what it was given,
// so the inferred type is the token set itself — which is what makes
// `ThemeOverrides<typeof ufTokens>` reject a token this module does not declare.
export const ufTokens = stylex.defineVars({
  // Surfaces, from furthest back to nearest front.
  canvas: "#f8fafc",
  sunken: "#eef2f7",
  surface: "#ffffff",
  surfaceHover: "#f1f5f9",
  border: "#dbe3ec",

  // Text.
  ink: "#0f172a",
  muted: "#475569",

  // The colour a primary action wears, and what is legible on it.
  accent: "#5c49ff",
  accentHover: "#4a37f0",
  accentInk: "#ffffff",
  accentSoft: "#eeecff",

  // The colour a destructive action wears.
  danger: "#b42318",
  dangerHover: "#9a1c12",
  dangerInk: "#ffffff",
  dangerSoft: "#fef3f2",

  // Focus ring, and the wash behind a modal surface.
  focus: "#2677ff",
  scrim: "rgba(15, 23, 42, 0.48)",

  // Type.
  fontSans: "ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, sans-serif",
  fontMono: "ui-monospace, SFMono-Regular, Menlo, monospace",
  textXs: "12px",
  textSm: "14px",
  textMd: "16px",
  textLg: "20px",
  textXl: "28px",
  text2Xl: "40px",
  leadingTight: 1.25,
  leadingBase: 1.55,
  weightRegular: 400,
  weightMedium: 500,
  weightBold: 700,

  // Space.
  space1: "4px",
  space2: "8px",
  space3: "12px",
  space4: "16px",
  space6: "24px",
  space8: "32px",
  space12: "48px",

  // Shape.
  radiusSm: "8px",
  radiusMd: "12px",
  radiusLg: "16px",
  radiusXl: "24px",
  radiusPill: "999px",

  // The size of a control a finger or a pointer aims at.
  sizeControl: "18px",

  // Elevation and motion.
  shadowCard: "0 1px 2px rgba(15, 23, 42, 0.06), 0 1px 3px rgba(15, 23, 42, 0.10)",
  shadowPanel: "0 16px 48px rgba(15, 23, 42, 0.24)",
  durationFast: "120ms",
  durationBase: "200ms",
  easing: "cubic-bezier(0.2, 0, 0, 1)",
});
