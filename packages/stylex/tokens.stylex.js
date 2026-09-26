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

// The explicit key set is also the module interface downstream themes read.
export type UFTokens = {|
  readonly canvas: string,
  readonly sunken: string,
  readonly surface: string,
  readonly surfaceHover: string,
  readonly border: string,
  readonly ink: string,
  readonly muted: string,
  readonly accent: string,
  readonly accentHover: string,
  readonly accentInk: string,
  readonly accentSoft: string,
  readonly danger: string,
  readonly dangerHover: string,
  readonly dangerInk: string,
  readonly dangerSoft: string,
  readonly focus: string,
  readonly scrim: string,
  readonly fontSans: string,
  readonly fontMono: string,
  readonly textXs: string,
  readonly textSm: string,
  readonly textMd: string,
  readonly textLg: string,
  readonly textXl: string,
  readonly text2Xl: string,
  readonly leadingTight: number,
  readonly leadingBase: number,
  readonly weightRegular: number,
  readonly weightMedium: number,
  readonly weightBold: number,
  readonly space1: string,
  readonly space2: string,
  readonly space3: string,
  readonly space4: string,
  readonly space6: string,
  readonly space8: string,
  readonly space12: string,
  readonly radiusSm: string,
  readonly radiusMd: string,
  readonly radiusLg: string,
  readonly radiusPill: string,
  readonly sizeControl: string,
  readonly durationFast: string,
  readonly durationBase: string,
  readonly durationSlow: string,
  readonly easing: string,
  readonly easingEnter: string,
  readonly easingExit: string,
|};

export const ufTokens: UFTokens = stylex.defineVars({
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

  // There is no elevation: a surface in front of another is told apart by a
  // 1px `border` and its own background, never by a shadow.

  // # Motion
  //
  // Three durations and three easings, and every default transition is one of
  // each. `docs/ui-visual-language.md` has the table of what moves how; the
  // values are argued here, where a theme that wants to change them will look.
  //
  // **Durations, by how far the eye has to follow.**
  //
  //   * `durationFast`, 120ms: a change *in place* — a colour, a pressed
  //     button giving way, a focus ring drawing itself, a chevron turning.
  //     Nothing travels, so anything longer reads as lag between the press
  //     and the answer; anything under ~100ms is not seen at all, which is
  //     how #1414's defaults came to look like they had no motion.
  //   * `durationBase`, 200ms: something small *arriving* or *travelling* — a
  //     popover, menu or listbox leaving its trigger, a switch's thumb, a
  //     check being drawn. Long enough to see where it came from, short enough
  //     that a reader who opens a menu to act in it is never waiting for it.
  //   * `durationSlow`, 280ms: something *large* — a dialog, a sheet, a
  //     drawer, a toast, a progress bar's fill. A surface that covers a third
  //     of the screen and moves as fast as a menu looks thrown. It stops short
  //     of the ~300ms where an interface starts to feel like it is performing.
  //
  // **Easings, by where the motion starts and ends.** All three are
  // cubic-béziers with both y values inside 0..1, so nothing overshoots its
  // destination and nothing bounces; `defaults.rs` holds them to that.
  //
  //   * `easing`, `cubic-bezier(0.2, 0, 0, 1)`: the standard curve, for
  //     something moving between two places it rests at — a switch's thumb, a
  //     progress fill, a drawer between snap points, a colour. It eases out of
  //     rest a little and settles for a long time, so it never stops abruptly.
  //   * `easingEnter`, `cubic-bezier(0, 0, 0.2, 1)`: a decelerate, for
  //     something *arriving*. It leaves at full speed and spends its second
  //     half settling: half the distance is covered by a fifth of the time,
  //     so the content is legible almost at once, while the settle is long
  //     enough to be seen. A stronger curve (Material's emphasised
  //     `0.05, 0.7, 0.1, 1`) was tried and rejected: it is 78% there by a
  //     fifth of the time, which on a 200ms popover is the "almost no motion"
  //     this replaces.
  //   * `easingExit`, `cubic-bezier(0.4, 0, 1, 1)`: an accelerate, for
  //     something *leaving*. It starts slowly, so the eye registers that it is
  //     going, then gets out of the way at full speed. Exits pair it with a
  //     shorter duration than the matching enter, because nobody watches a
  //     thing leave. (Exit transitions need the behaviour layer to keep a
  //     closing part mounted until they finish; until it does, this token is
  //     declared and not yet read — see `docs/ui-visual-language.md`.)
  //
  // A transition names the properties it moves, never `all`. Under
  // `prefers-reduced-motion: reduce` nothing travels, grows or is drawn:
  // either the duration is `0s`, or only opacity and colour still transition,
  // so an overlay still fades rather than cutting. `defaults.rs` checks every
  // default style for each of these.
  durationFast: "120ms",
  durationBase: "200ms",
  durationSlow: "280ms",
  easing: "cubic-bezier(0.2, 0, 0, 1)",
  easingEnter: "cubic-bezier(0, 0, 0.2, 1)",
  easingExit: "cubic-bezier(0.4, 0, 1, 1)",
});
