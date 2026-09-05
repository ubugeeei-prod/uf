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
// # Two themes, because there are two questions
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
// Both can be composed, and the merge decides the same way it decides anything
// else: `props(ufAutoTheme, ufDarkTheme)` is dark, because the later argument
// wins token by token.
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

import { stylex } from "@uniflowed/stylex";

import { ufTokens } from "./tokens.stylex.js";

/**
 * The preset, following the reader's operating system.
 *
 * Every entry is conditional, so this theme is inert in light mode and costs
 * exactly one `@media (prefers-color-scheme: dark)` block.
 */
export const ufAutoTheme = stylex.createTheme(ufTokens, {
  canvas: { "@media (prefers-color-scheme: dark)": "#0b1220" },
  sunken: { "@media (prefers-color-scheme: dark)": "#0f1726" },
  surface: { "@media (prefers-color-scheme: dark)": "#131c2e" },
  surfaceHover: { "@media (prefers-color-scheme: dark)": "#1b2540" },
  border: { "@media (prefers-color-scheme: dark)": "#26324a" },
  ink: { "@media (prefers-color-scheme: dark)": "#e8eefc" },
  muted: { "@media (prefers-color-scheme: dark)": "#9fb0cc" },
  accent: { "@media (prefers-color-scheme: dark)": "#8f8bff" },
  accentHover: { "@media (prefers-color-scheme: dark)": "#a49fff" },
  accentInk: { "@media (prefers-color-scheme: dark)": "#0b1220" },
  accentSoft: { "@media (prefers-color-scheme: dark)": "#1e2547" },
  danger: { "@media (prefers-color-scheme: dark)": "#ff6b5e" },
  dangerHover: { "@media (prefers-color-scheme: dark)": "#ff8378" },
  dangerInk: { "@media (prefers-color-scheme: dark)": "#2a0b08" },
  dangerSoft: { "@media (prefers-color-scheme: dark)": "#2a1512" },
  focus: { "@media (prefers-color-scheme: dark)": "#7fb0ff" },
  scrim: { "@media (prefers-color-scheme: dark)": "rgba(2, 6, 16, 0.62)" },
  shadowCard: { "@media (prefers-color-scheme: dark)": "0 1px 2px rgba(0, 0, 0, 0.40)" },
  shadowPanel: { "@media (prefers-color-scheme: dark)": "0 16px 48px rgba(0, 0, 0, 0.55)" },
});

/**
 * The preset in dark, unconditionally.
 *
 * For a subtree that is dark whatever the system says, and for the stored
 * preference a reader chose.
 */
export const ufDarkTheme = stylex.createTheme(ufTokens, {
  canvas: "#0b1220",
  sunken: "#0f1726",
  surface: "#131c2e",
  surfaceHover: "#1b2540",
  border: "#26324a",
  ink: "#e8eefc",
  muted: "#9fb0cc",
  accent: "#8f8bff",
  accentHover: "#a49fff",
  accentInk: "#0b1220",
  accentSoft: "#1e2547",
  danger: "#ff6b5e",
  dangerHover: "#ff8378",
  dangerInk: "#2a0b08",
  dangerSoft: "#2a1512",
  focus: "#7fb0ff",
  scrim: "rgba(2, 6, 16, 0.62)",
  shadowCard: "0 1px 2px rgba(0, 0, 0, 0.40)",
  shadowPanel: "0 16px 48px rgba(0, 0, 0, 0.55)",
});
