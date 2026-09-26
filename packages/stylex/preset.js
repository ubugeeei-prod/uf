// @flow
//
// `@uniflowed/stylex/preset`: the default look, as props you spread.
//
//   import { buttonStyles, cardStyles } from "@uniflowed/stylex/preset";
//   import { Dialog } from "@uniflowed/ui";
//
//   <div {...cardStyles()}>
//     <button {...buttonStyles({ tone: "primary" })}>Save</button>
//   </div>
//
// This is the second of the three things uf's preset is: a base layer over
// `./tokens.stylex.js`, so a project has a coherent default look without
// authoring a declaration. Every function answers with `{ className }` and
// nothing else, which is why a `@uniflowed/ui` primitive takes them without
// knowing they exist — `className` is a DOM prop, not a StyleX type, and
// keeping it that way is what lets the headless package stay headless.
//
// # Why this is a separate module
//
// It is the only module in the package that has opinions. `props` is a merge,
// the tokens are names, the themes are values; this is where uf says a card has
// a 6px radius and a 1px border rather than a shadow. A project that wants its
// own look drops this import and keeps everything else, and a bundler drops
// the rules with it because nothing else references them.
//
// # Why functions rather than exported namespaces
//
// A namespace would make the caller responsible for the merge order —
// `props(button.base, button.primary, button.md)` — and getting that order
// wrong is silent. The functions own it, and they take the *variants* a caller
// actually has an opinion about. `match` makes each mapping exhaustive, so a
// tone added to the type without a rule is a Flow error rather than an
// unstyled button.
//
// # What the merge order here has to get right
//
// Every recipe puts `base` first and the variant after it, because the merge's
// unit is the property and the later argument wins. Two of them rely on that
// deliberately: `menuItemStyles({ active: true })` replaces the item's
// `backgroundColor` *including* its `:hover` state, which is what a selected
// row should do, and `buttonStyles({ disabled: true })` replaces the tone's
// cursor. Neither works if the arguments are swapped, and the compile-time
// model in `crates/uf_stylex/src/props.rs` is where that is pinned down.

// `stylex` comes from the package's own name because that is the specifier the
// compiler resolves a StyleX binding from; `props` and its types come from the
// module that owns them, which is the same module either way.
import { stylex } from "@uniflowed/stylex";

import type { StyleProps } from "./props.js";
import { props } from "./props.js";
import { ufTokens } from "./tokens.stylex.js";

/** How loud a control is, and therefore what it is for. */
export type Tone = "primary" | "neutral" | "ghost" | "danger";

/** The three sizes every control in the preset comes in. */
export type Size = "sm" | "md" | "lg";

/** How much a surface is lifted off the page. */
export type SurfaceKind = "page" | "card" | "panel" | "sunken";

/** Which of the type scale's steps a piece of text sits on. */
export type TextSize = "xs" | "sm" | "md" | "lg" | "xl" | "2xl";

/** What a piece of text is: body copy, a secondary note, or an error. */
export type TextTone = "ink" | "muted" | "danger";

const surfaces = stylex.create({
  base: {
    color: ufTokens.ink,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textMd,
    lineHeight: ufTokens.leadingBase,
  },
  page: {
    backgroundColor: ufTokens.canvas,
  },
  card: {
    backgroundColor: ufTokens.surface,
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderRadius: ufTokens.radiusLg,
    padding: ufTokens.space6,
  },
  panel: {
    backgroundColor: ufTokens.surface,
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderRadius: ufTokens.radiusLg,
    padding: ufTokens.space6,
  },
  sunken: {
    backgroundColor: ufTokens.sunken,
    borderRadius: ufTokens.radiusMd,
    padding: ufTokens.space4,
  },
});

const texts = stylex.create({
  base: {
    fontFamily: ufTokens.fontSans,
    lineHeight: ufTokens.leadingBase,
    margin: 0,
  },
  xs: { fontSize: ufTokens.textXs },
  sm: { fontSize: ufTokens.textSm },
  md: { fontSize: ufTokens.textMd },
  lg: { fontSize: ufTokens.textLg, lineHeight: ufTokens.leadingTight },
  xl: { fontSize: ufTokens.textXl, lineHeight: ufTokens.leadingTight },
  xxl: { fontSize: ufTokens.text2Xl, lineHeight: ufTokens.leadingTight },
  ink: { color: ufTokens.ink },
  muted: { color: ufTokens.muted },
  danger: { color: ufTokens.danger },
  strong: { fontWeight: ufTokens.weightBold },
});

const buttons = stylex.create({
  base: {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    gap: ufTokens.space2,
    fontFamily: ufTokens.fontSans,
    fontWeight: ufTokens.weightMedium,
    lineHeight: ufTokens.leadingTight,
    borderRadius: ufTokens.radiusMd,
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "transparent",
    cursor: "pointer",
    // Press: the button gives a little under the pointer, to 97% of its size
    // in `durationFast`, and comes back on release without passing 1. Under
    // reduced motion it does not move; only the colours still change.
    transform: {
      default: "none",
      ":active": "scale(0.97)",
      "@media (prefers-reduced-motion: reduce)": "none",
    },
    // The focus ring draws itself outward from the edge (`outline-width`
    // 0 → 2px) rather than blinking on, in `durationFast`.
    transitionProperty: {
      default: "background-color, border-color, color, outline-width, transform",
      "@media (prefers-reduced-motion: reduce)": "background-color, border-color, color",
    },
    transitionDuration: ufTokens.durationFast,
    transitionTimingFunction: ufTokens.easing,
    // The ring is drawn only for a keyboard focus, which is the whole reason
    // `:focus-visible` exists: a mouse click should not light the control up.
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
  },
  primary: {
    backgroundColor: { default: ufTokens.accent, ":hover": ufTokens.accentHover },
    color: ufTokens.accentInk,
  },
  neutral: {
    backgroundColor: { default: ufTokens.surface, ":hover": ufTokens.surfaceHover },
    borderColor: ufTokens.border,
    color: ufTokens.ink,
  },
  ghost: {
    backgroundColor: { default: "transparent", ":hover": ufTokens.surfaceHover },
    color: ufTokens.ink,
  },
  danger: {
    backgroundColor: { default: ufTokens.danger, ":hover": ufTokens.dangerHover },
    color: ufTokens.dangerInk,
  },
  sm: {
    fontSize: ufTokens.textSm,
    paddingBlock: ufTokens.space1,
    paddingInline: ufTokens.space3,
  },
  md: {
    fontSize: ufTokens.textSm,
    paddingBlock: ufTokens.space2,
    paddingInline: ufTokens.space4,
  },
  lg: {
    fontSize: ufTokens.textMd,
    paddingBlock: ufTokens.space3,
    paddingInline: ufTokens.space6,
  },
  disabled: {
    opacity: 0.55,
    cursor: "not-allowed",
  },
});

const fields = stylex.create({
  base: {
    display: "block",
    width: "100%",
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    lineHeight: ufTokens.leadingBase,
    color: ufTokens.ink,
    backgroundColor: ufTokens.surface,
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderRadius: ufTokens.radiusMd,
    paddingBlock: ufTokens.space2,
    paddingInline: ufTokens.space3,
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "1px",
    // The border turns and the focus ring draws outward in `durationFast`;
    // only the colours stay under reduced motion.
    transitionProperty: {
      default: "background-color, border-color, outline-width",
      "@media (prefers-reduced-motion: reduce)": "background-color, border-color",
    },
    transitionDuration: ufTokens.durationFast,
    transitionTimingFunction: ufTokens.easing,
  },
  invalid: {
    borderColor: ufTokens.danger,
    backgroundColor: ufTokens.dangerSoft,
  },
  disabled: {
    backgroundColor: ufTokens.sunken,
    color: ufTokens.muted,
    cursor: "not-allowed",
  },
});

const overlays = stylex.create({
  // Enter: the wash fades in over `durationSlow`, the same time as the panel
  // in front of it. Exit: it clears with the panel, in `durationBase` on the
  // accelerating curve, while `@uniflowed/ui` keeps it on the page with
  // `data-state="closed"` and `inert`. A fade is what reduced motion keeps,
  // so it keeps both.
  backdrop: {
    position: "fixed",
    inset: 0,
    backgroundColor: ufTokens.scrim,
    opacity: { default: 1, "@starting-style": 0, ":is([data-state=closed])": 0 },
    transitionProperty: "opacity",
    transitionDuration: {
      default: ufTokens.durationSlow,
      ":is([data-state=closed])": ufTokens.durationBase,
    },
    transitionTimingFunction: {
      default: ufTokens.easingEnter,
      ":is([data-state=closed])": ufTokens.easingExit,
    },
  },
  // Enter: the panel fades in and grows from 96% of its size where it rests,
  // on the decelerating curve, so it is legible almost at once and is seen
  // settling rather than popping. The centring translate is part of both
  // values, or the panel would start in the corner. Exit: back to 96% as it
  // fades, in `durationBase` on the accelerating curve. Under reduced motion
  // both only fade: `--uf-exit-travel` is 0 there, so the scale does not jump.
  dialog: {
    position: "fixed",
    top: "50%",
    left: "50%",
    "--uf-exit-travel": { default: "1", "@media (prefers-reduced-motion: reduce)": "0" },
    opacity: { default: 1, "@starting-style": 0, ":is([data-state=closed])": 0 },
    transform: {
      default: "translate(-50%, -50%)",
      "@starting-style": "translate(-50%, -50%) scale(0.96)",
      ":is([data-state=closed])":
        "translate(-50%, -50%) scale(calc(1 - 0.04 * var(--uf-exit-travel)))",
    },
    transitionProperty: {
      default: "opacity, transform",
      "@media (prefers-reduced-motion: reduce)": "opacity",
    },
    transitionDuration: {
      default: ufTokens.durationSlow,
      ":is([data-state=closed])": ufTokens.durationBase,
    },
    transitionTimingFunction: {
      default: ufTokens.easingEnter,
      ":is([data-state=closed])": ufTokens.easingExit,
    },
    width: "calc(100% - 32px)",
    maxWidth: "32rem",
    backgroundColor: ufTokens.surface,
    color: ufTokens.ink,
    fontFamily: ufTokens.fontSans,
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderRadius: ufTokens.radiusLg,
    padding: ufTokens.space6,
  },
});

const menus = stylex.create({
  // Enter: it fades in while travelling 4px out of its trigger, from the side
  // `@uniflowed/ui` put it on (`data-side`), the way `registry/ui/menu.js`
  // does. Exit: back towards the trigger in `durationFast` on the
  // accelerating curve. Under reduced motion both only fade.
  menu: {
    "--uf-enter-x": {
      default: "0px",
      ":is([data-side=left])": "4px",
      ":is([data-side=right])": "-4px",
    },
    "--uf-enter-y": {
      default: "0px",
      ":is([data-side=top])": "4px",
      ":is([data-side=bottom])": "-4px",
    },
    "--uf-exit-travel": { default: "1", "@media (prefers-reduced-motion: reduce)": "0" },
    opacity: { default: 1, "@starting-style": 0, ":is([data-state=closed])": 0 },
    transform: {
      default: "none",
      "@starting-style": "translate(var(--uf-enter-x), var(--uf-enter-y))",
      ":is([data-state=closed])":
        "translate(calc(var(--uf-enter-x) * var(--uf-exit-travel)), calc(var(--uf-enter-y) * var(--uf-exit-travel)))",
    },
    transitionProperty: {
      default: "opacity, transform",
      "@media (prefers-reduced-motion: reduce)": "opacity",
    },
    transitionDuration: {
      default: ufTokens.durationBase,
      ":is([data-state=closed])": ufTokens.durationFast,
    },
    transitionTimingFunction: {
      default: ufTokens.easingEnter,
      ":is([data-state=closed])": ufTokens.easingExit,
    },
    minWidth: "12rem",
    margin: 0,
    padding: ufTokens.space1,
    listStyle: "none",
    backgroundColor: ufTokens.surface,
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderRadius: ufTokens.radiusMd,
  },
  item: {
    display: "flex",
    alignItems: "center",
    gap: ufTokens.space2,
    width: "100%",
    paddingBlock: ufTokens.space2,
    paddingInline: ufTokens.space3,
    borderRadius: ufTokens.radiusSm,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    textAlign: "start",
    color: ufTokens.ink,
    backgroundColor: { default: "transparent", ":hover": ufTokens.surfaceHover },
    cursor: "pointer",
    // The highlight follows the pointer in `durationFast`: long enough to be
    // seen, short enough never to trail it. Colour, so reduced motion keeps it.
    transitionProperty: "background-color, color",
    transitionDuration: ufTokens.durationFast,
    transitionTimingFunction: ufTokens.easing,
  },
  active: {
    backgroundColor: ufTokens.accentSoft,
    color: ufTokens.accent,
  },
  disabled: {
    color: ufTokens.muted,
    cursor: "not-allowed",
  },
});

const tabs = stylex.create({
  list: {
    display: "flex",
    gap: ufTokens.space1,
    borderBottomWidth: "1px",
    borderBottomStyle: "solid",
    borderBottomColor: ufTokens.border,
  },
  tab: {
    // `borderWidth: 0` before `borderBottomWidth: "2px"` is the case the sheet's
    // ordering exists for: both are one class selector, and the longhand only
    // survives because the shorthand is emitted before it.
    borderWidth: 0,
    borderStyle: "solid",
    borderColor: "transparent",
    borderBottomWidth: "2px",
    backgroundColor: "transparent",
    paddingBlock: ufTokens.space2,
    paddingInline: ufTokens.space3,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    fontWeight: ufTokens.weightMedium,
    color: { default: ufTokens.muted, ":hover": ufTokens.ink },
    cursor: "pointer",
    // The underline and the label change together, and the focus ring draws
    // outward; only the colours stay under reduced motion.
    transitionProperty: {
      default: "border-color, color, outline-width",
      "@media (prefers-reduced-motion: reduce)": "border-color, color",
    },
    transitionDuration: ufTokens.durationBase,
    transitionTimingFunction: ufTokens.easing,
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "-2px",
  },
  selected: {
    color: ufTokens.ink,
    borderBottomColor: ufTokens.accent,
  },
});

const controls = stylex.create({
  base: {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    flexShrink: 0,
    padding: 0,
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    backgroundColor: ufTokens.surface,
    color: ufTokens.accentInk,
    cursor: "pointer",
    // Colours, and the focus ring drawing outward; only the colours stay
    // under reduced motion.
    transitionProperty: {
      default: "background-color, border-color, outline-width",
      "@media (prefers-reduced-motion: reduce)": "background-color, border-color",
    },
    transitionDuration: ufTokens.durationFast,
    transitionTimingFunction: ufTokens.easing,
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
  },
  box: {
    width: ufTokens.sizeControl,
    height: ufTokens.sizeControl,
    borderRadius: ufTokens.radiusSm,
  },
  // A switch's track is the one piece of geometry here that is not a token:
  // nothing but a switch reads it, and a theme that changed it would leave the
  // thumb somewhere else.
  track: {
    width: "36px",
    height: "20px",
    justifyContent: "flex-start",
    borderRadius: ufTokens.radiusPill,
  },
  on: {
    backgroundColor: ufTokens.accent,
    borderColor: ufTokens.accent,
  },
  disabled: {
    backgroundColor: ufTokens.sunken,
    cursor: "not-allowed",
    opacity: 0.6,
  },
});

/** A page or a card, with the preset's type already on it. */
export function surfaceStyles(options?: { readonly kind?: SurfaceKind }): StyleProps {
  const kind = options?.kind ?? "card";
  return props(
    surfaces.base,
    match (kind) {
      "page" => surfaces.page,
      "card" => surfaces.card,
      "panel" => surfaces.panel,
      "sunken" => surfaces.sunken,
    },
  );
}

/** A card: the surface most application chrome is made of. */
export function cardStyles(): StyleProps {
  return surfaceStyles({ kind: "card" });
}

/** One step of the type scale, in one of the three text roles. */
export function textStyles(options?: {
  readonly size?: TextSize,
  readonly tone?: TextTone,
  readonly strong?: boolean,
}): StyleProps {
  const size = options?.size ?? "md";
  const tone = options?.tone ?? "ink";
  return props(
    texts.base,
    match (size) {
      "xs" => texts.xs,
      "sm" => texts.sm,
      "md" => texts.md,
      "lg" => texts.lg,
      "xl" => texts.xl,
      "2xl" => texts.xxl,
    },
    match (tone) {
      "ink" => texts.ink,
      "muted" => texts.muted,
      "danger" => texts.danger,
    },
    options?.strong === true && texts.strong,
  );
}

/** A button, in one of four tones and three sizes. */
export function buttonStyles(options?: {
  readonly tone?: Tone,
  readonly size?: Size,
  readonly disabled?: boolean,
}): StyleProps {
  const tone = options?.tone ?? "neutral";
  const size = options?.size ?? "md";
  return props(
    buttons.base,
    match (tone) {
      "primary" => buttons.primary,
      "neutral" => buttons.neutral,
      "ghost" => buttons.ghost,
      "danger" => buttons.danger,
    },
    match (size) {
      "sm" => buttons.sm,
      "md" => buttons.md,
      "lg" => buttons.lg,
    },
    options?.disabled === true && buttons.disabled,
  );
}

/** A text input, a select, or anything else that takes typing. */
export function fieldStyles(options?: {
  readonly invalid?: boolean,
  readonly disabled?: boolean,
}): StyleProps {
  return props(
    fields.base,
    options?.invalid === true && fields.invalid,
    options?.disabled === true && fields.disabled,
  );
}

/** The wash behind a modal surface. */
export function backdropStyles(): StyleProps {
  return props(overlays.backdrop);
}

/** A centred modal panel. */
export function dialogStyles(): StyleProps {
  return props(overlays.dialog);
}

/** The box a menu's options sit in. */
export function menuStyles(): StyleProps {
  return props(menus.menu);
}

/** One option in a menu. */
export function menuItemStyles(options?: {
  readonly active?: boolean,
  readonly disabled?: boolean,
}): StyleProps {
  return props(
    menus.item,
    options?.active === true && menus.active,
    options?.disabled === true && menus.disabled,
  );
}

/** The row a set of tabs sits in. */
export function tabListStyles(): StyleProps {
  return props(tabs.list);
}

/** One tab, selected or not. */
export function tabStyles(options?: { readonly selected?: boolean }): StyleProps {
  return props(tabs.tab, options?.selected === true && tabs.selected);
}

/** A checkbox's box or a switch's track. */
export function controlStyles(options?: {
  readonly shape?: "box" | "track",
  readonly on?: boolean,
  readonly disabled?: boolean,
}): StyleProps {
  const shape = options?.shape ?? "box";
  return props(
    controls.base,
    match (shape) {
      "box" => controls.box,
      "track" => controls.track,
    },
    options?.on === true && controls.on,
    options?.disabled === true && controls.disabled,
  );
}
