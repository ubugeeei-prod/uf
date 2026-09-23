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
// the colour a primary action wears; it is a quiet cobalt today and a theme
// can make it anything, and no rule in `preset.js` has to change when it does.
// A token named `blue600` would have made the preset unthemeable the moment
// someone wanted a green product.
//
// # The palette
//
// `docs/ui-visual-language.md` explains it; in short: the interface is
// neutral first — warm greys with no blue cast, near-black ink on off-white
// paper — and colour is spent in one place, a single accent for what is
// selected, current or primary. Danger is a brick red rather than a signal
// red. Dark is designed as its own palette (charcoal, not navy; a lighter,
// desaturated accent that does not glow), not the light one inverted.
//
// The type and spacing steps are `@uniflowed/brand`'s scales. The colours and
// the corners are not: brand's cyan-to-magenta spectrum and its round corners
// are uf's logo and marketing identity, and an interface built from them looks
// like every other generated one, so this module's values are its own. Brand
// cannot hold the token module itself anyway — a StyleX token's name is
// computed by the compiler, and `defineVars` takes literals, not an imported
// array.
//
// # No shadows, no gradients
//
// The defaults draw depth with a rule and a change of surface, and nothing
// else. A shadow is the first thing a theme has to fight when the product's
// look is flat, and a gradient is a decision about a brand this module does
// not have, so neither is a token and neither appears in a default style.
// `crates/uf_stylex/src/tests/defaults.rs` compiles every default style —
// this module, `preset.js`, `theme.js` and every file in `registry/ui/` — and
// fails on a `box-shadow`, a `text-shadow`, a `drop-shadow()` or a gradient.
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
  canvas: "#f6f6f4",
  sunken: "#eeeeeb",
  surface: "#ffffff",
  surfaceHover: "#f1f1ee",
  border: "#d9d9d4",

  // Text.
  ink: "#1b1b19",
  muted: "#5c5c57",

  // The colour a primary action wears, and what is legible on it.
  accent: "#2b58b5",
  accentHover: "#234996",
  accentInk: "#ffffff",
  accentSoft: "#e8eef9",

  // The colour a destructive action wears.
  danger: "#b1271d",
  dangerHover: "#931f17",
  dangerInk: "#ffffff",
  dangerSoft: "#fbedeb",

  // Focus ring, and the wash behind a modal surface.
  focus: "#2b58b5",
  scrim: "rgba(27, 27, 25, 0.45)",

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

  // Shape. Three small steps and a pill, and nothing rounder: a control is
  // 2px, a button, field group or popover 4px, and the largest surface — a
  // card, a dialog — 6px. `radiusPill` is for the few things that are round
  // by what they are, such as a switch's track or a radio's dot, and not a
  // way to soften a rectangle.
  radiusSm: "2px",
  radiusMd: "4px",
  radiusLg: "6px",
  radiusPill: "999px",

  // The size of a control a finger or a pointer aims at.
  sizeControl: "18px",

  // Motion. Two short durations and one easing: `durationFast` for a colour
  // or a chevron, `durationBase` for something that travels, such as a
  // switch's thumb or a drawer. The easing decelerates and never overshoots,
  // so nothing bounces. A transition names the properties it moves, never
  // `all`, and is `0s` under `prefers-reduced-motion: reduce`;
  // `crates/uf_stylex/src/tests/defaults.rs` holds every default style to
  // that.
  //
  // There is no elevation either: a surface in front of another is told
  // apart by a 1px `border` and its own background, never by a shadow.
  durationFast: "120ms",
  durationBase: "160ms",
  easing: "cubic-bezier(0.2, 0, 0, 1)",
});
