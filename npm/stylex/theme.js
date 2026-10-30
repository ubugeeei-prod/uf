// @flow
//
// `@uniflowed/stylex/theme`: the shipped themes, and how to replace them.
//
//   import { props } from "@uniflowed/stylex";
//   import { ufAutoTheme } from "@uniflowed/stylex/theme";
//
//   <body {...props(ufAutoTheme)}>…</body>
//
// This is the third of the three things uf's preset is, and the one that makes
// the other two a default rather than a decoration. A theme is a set of
// overrides for `./tokens.stylex.js`; `uf transform` turns each override into
// one class whose rule writes that token's custom property, so putting the
// theme's class on an ancestor changes what every `var(--…)` beneath it
// resolves to. No rule in `preset.js` knows a theme happened.
//
// # Three themes, because there are three questions
//
// [`ufAutoTheme`] answers "follow the reader's operating system". It overrides
// nothing at all in light mode — every entry is written *only* under
// `@media (prefers-color-scheme: dark)`, so outside dark mode its class matches
// no rule and the tokens keep their declared values. That is why it costs one
// media block rather than two full sets of custom properties.
//
// [`ufDarkTheme`] answers "this subtree is dark, whatever the system says": a
// preview pane, a code surface, a reader who chose. Its overrides are
// unconditional, so it also works as the thing a stored preference applies.
//
// [`ufContrastTheme`] answers "this subtree is high contrast, whatever the
// system says". It is unconditional too, over the same 19 colour tokens. The
// neutrals are grey — no channel is more than a few steps from the others —
// and the accent sits outside the violet band every generated palette falls
// into. The pairs the preset paints are checked in
// `crates/uf_stylex/src/tests/preset.rs`, and this theme is held to a higher
// floor than the other two.
//
// All three can be composed, and the merge decides the same way it decides
// anything else: `props(ufAutoTheme, ufDarkTheme)` is dark, and
// `props(ufDarkTheme, ufContrastTheme)` is the high-contrast one, because the
// later argument wins token by token.
//
// # Writing your own
//
// `createTheme` takes the token set and a subset of its keys, and Flow rejects
// a key the set does not declare — so a renamed token is a type error rather
// than a custom property nothing reads:
//
//   export const brandTheme = stylex.createTheme(ufTokens, {
//     accent: "#0f766e",
//     accentHover: "#115e59",
//   });
//
// Overriding four colours is a complete theme; there is no requirement to
// restate the set. Two themes that give the same token the same value compile
// to the same class and one rule, wherever they were written.
//
// # Contrast
//
// The dark values below meet WCAG AA at 4.5:1 for every pair the preset
// actually pairs, and `crates/uf_stylex/src/tests/preset.rs` computes those
// ratios from the compiled stylesheet.

import { stylex, type CompiledStyle } from "@uniflowed/stylex";

import { ufTokens } from "./tokens.stylex.js";

/**
 * The preset, following the reader's operating system.
 *
 * Every entry is conditional, so this theme is inert in light mode and costs
 * exactly one `@media (prefers-color-scheme: dark)` block.
 */
export const ufAutoTheme: CompiledStyle = stylex.createTheme(ufTokens, {
  canvas      : { "@media (prefers-color-scheme: dark)": "#141413" },
  sunken      : { "@media (prefers-color-scheme: dark)": "#0f0f0e" },
  surface     : { "@media (prefers-color-scheme: dark)": "#1c1c1a" },
  surfaceHover: { "@media (prefers-color-scheme: dark)": "#262623" },
  border      : { "@media (prefers-color-scheme: dark)": "#363632" },
  ink         : { "@media (prefers-color-scheme: dark)": "#ecece7" },
  muted       : { "@media (prefers-color-scheme: dark)": "#a5a59e" },
  accent      : { "@media (prefers-color-scheme: dark)": "#8eaee8" },
  accentHover : { "@media (prefers-color-scheme: dark)": "#a8c1ee" },
  accentInk   : { "@media (prefers-color-scheme: dark)": "#101318" },
  accentSoft  : { "@media (prefers-color-scheme: dark)": "#1f2838" },
  danger      : { "@media (prefers-color-scheme: dark)": "#ee8a80" },
  dangerHover : { "@media (prefers-color-scheme: dark)": "#f3a49c" },
  dangerInk   : { "@media (prefers-color-scheme: dark)": "#1d0f0d" },
  dangerSoft  : { "@media (prefers-color-scheme: dark)": "#35201d" },
  focus       : { "@media (prefers-color-scheme: dark)": "#8eaee8" },
  scrim       : { "@media (prefers-color-scheme: dark)": "rgba(0, 0, 0, 0.6)" },
});

/**
 * The preset in dark, unconditionally.
 *
 * For a subtree that is dark whatever the system says, and for the stored
 * preference a reader chose.
 */
export const ufDarkTheme: CompiledStyle = stylex.createTheme(ufTokens, {
  canvas      : "#141413",
  sunken      : "#0f0f0e",
  surface     : "#1c1c1a",
  surfaceHover: "#262623",
  border      : "#363632",
  ink         : "#ecece7",
  muted       : "#a5a59e",
  accent      : "#8eaee8",
  accentHover : "#a8c1ee",
  accentInk   : "#101318",
  accentSoft  : "#1f2838",
  danger      : "#ee8a80",
  dangerHover : "#f3a49c",
  dangerInk   : "#1d0f0d",
  dangerSoft  : "#35201d",
  focus       : "#8eaee8",
  scrim       : "rgba(0, 0, 0, 0.6)",
});

/**
 * The preset at high contrast, unconditionally.
 *
 * For a reader who asked for more separation than the light default or the
 * dark theme give them. The greys stay grey, the accent is a blue outside the
 * violet band, and a later theme still replaces it token by token.
 */
export const ufContrastTheme: CompiledStyle = stylex.createTheme(ufTokens, {
  canvas      : "#ffffff",
  sunken      : "#f3f3f1",
  surface     : "#ffffff",
  surfaceHover: "#e7e7e3",
  border      : "#2c2c2a",
  ink         : "#111110",
  muted       : "#3f3f3b",
  accent      : "#1d3f8f",
  accentHover : "#163475",
  accentInk   : "#ffffff",
  accentSoft  : "#e4ebf8",
  danger      : "#8e1b14",
  dangerHover : "#751610",
  dangerInk   : "#ffffff",
  dangerSoft  : "#f8e4e2",
  focus       : "#1d3f8f",
  scrim       : "rgba(17, 17, 16, 0.72)",
});
