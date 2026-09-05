// @flow
//
// `@uniflowed/stylex`: uf's style engine, and the preset it ships.
//
// Almost all of StyleX happens at compile time. `uf transform` rewrites every
// `stylex.create({ … })` into a plain object of class names, turns every
// `stylex.defineVars({ … })` into CSS custom properties, turns every
// `stylex.createTheme(tokens, { … })` into the classes that override them, and
// collects the rules into a stylesheet. By the time this module is loaded there
// are no style values left — only names.
//
// This module is the front door. The merge lives in `./props.js` beside its
// compile-time twin; the preset lives in `./tokens.stylex.js`, `./preset.js`
// and `./theme.js`. What is here is the merge re-exported, and the three
// functions that must never run.
//
// # What "preset" means here
//
// A framework's styling default is worth something only if it removes work
// without removing control, so uf's preset is three things and stops there:
//
// 1. **A token set that already exists.** `@uniflowed/stylex/tokens.stylex.js`
//    is a real `stylex.defineVars` module — colour roles, a type scale,
//    spacing, radii, elevation and motion — so a project has a coherent palette
//    without authoring one, and gets it as `:root` custom properties the build
//    inlined rather than as anything computed in a browser.
// 2. **A base layer over those tokens.** `@uniflowed/stylex/preset` is
//    `stylex.create` namespaces for the surfaces a real application has, behind
//    small functions that answer with `{ className }`. A project gets a default
//    look by spreading them and nothing else.
// 3. **A way to replace any of it.** `stylex.createTheme(ufTokens, { … })`
//    compiles to one class per overridden token; put that class on an ancestor
//    and every `var(--…)` beneath it resolves differently. `./theme.js` ships
//    two, and a project writes its own the same way.
//
// It is deliberately *not*: a global reset (uf emits no rule a project did not
// ask for), a component library (`@uniflowed/ui` ships behaviour and no
// styles, and must keep doing so — nothing here appears in its types), or a
// configuration flag. The preset is reached by importing it, so a project that
// does not import it pays nothing, and one that imports half of it keeps half.
//
// # Where the token values come from
//
// `@uniflowed/brand` owns uf's visual identity — the palette, the type,
// spacing and radius scales — and it stays there. It cannot own the token
// module: a StyleX token's name is computed by the compiler from the binding
// and key it was declared under, `defineVars` accepts only literals, and
// brand's `--uf-*` names are hand-written for a different consumer. So brand
// holds the identity values and `./tokens.stylex.js` is their StyleX-shaped
// projection into semantic roles — `accent`, `ink`, `canvas` — which is the
// layer a design system needs and an identity does not have.
//
// # Readiness
//
// **Implemented.** `props`, `create`, `defineVars`, `createTheme`, pseudo-class
// and pseudo-element conditions, at-rule conditions (`@media`, `@supports`),
// shorthand-versus-longhand ordering, unit inference, the preset tokens, the
// base layer, and the shipped themes. Every one of them is compiled: the
// runtime half of this package is `props` and three functions that throw.
//
// **Experimental.** The preset's own visual choices. The token *names* are the
// contract and are meant to be stable; the values behind them are uf's opinion
// and may change within the alpha series. A project that wants them pinned
// should say so with a theme.
//
// **Not there.** `keyframes`, `firstThatWorks` and `positionTry`. Each needs a
// rule shape the sheet does not have — a named `@keyframes` block, several
// declarations of one property in one rule, and an `@position-try` block
// respectively — and the sheet's whole guarantee is that a rule is one class,
// one declaration, one state, ordered by what it is. Widening that is a change
// to the ordering model, not an addition to it, so none of the three is
// half-present: writing any of them is a compile error today, not a call that
// silently does nothing. A preset does not need them, which is why they are
// not in the way.

import { nativeRuntimeRequired } from "@uniflowed/core/native";

import type { CompiledStyle } from "./props.js";
import { props } from "./props.js";

const MODULE = "@uniflowed/stylex";

export type { CompiledClasses, CompiledStyle, StyleArgument, StyleProps } from "./props.js";
export { props } from "./props.js";

/** A value a token may hold, and therefore a value a theme may give it. */
export type ThemeValue = string | number;

/**
 * What `createTheme` accepts for one token set.
 *
 * Every key is optional — a theme that changes one colour is a theme — and no
 * key outside the token set is allowed, so a typo is a Flow error rather than a
 * custom property nothing reads. A value may carry states, which is how a theme
 * follows `@media (prefers-color-scheme: dark)` without a second theme.
 */
export type ThemeOverrides<Tokens extends { readonly [string]: ThemeValue }> = Partial<{
  [Key in keyof Tokens]: ThemeValue | { readonly [state: string]: ThemeValue },
}>;

/**
 * Declare a set of style namespaces.
 *
 * Never runs. `uf transform` replaces the whole call with the object it
 * computed, so reaching this means the module was loaded without going through
 * uf — a bundler configured by hand, a plain `node` invocation — and the styles
 * it declares are in no stylesheet. Throwing says so; returning the input would
 * render an application with no styles and no explanation.
 */
export function create<T extends { readonly [string]: mixed }>(styles: T): T {
  return nativeRuntimeRequired(MODULE, "stylex.create");
}

/**
 * Declare design tokens, and hand back the `var(--…)` references to them.
 *
 * Compile-time, for the same reason as `create`.
 */
export function defineVars<T extends { readonly [string]: ThemeValue }>(tokens: T): T {
  return nativeRuntimeRequired(MODULE, "stylex.defineVars");
}

/**
 * Override a set of tokens, and hand back the class that applies the override.
 *
 * The result is a compiled namespace like any other, so it is applied by
 * spreading it — `<div {...props(ufDarkTheme)}>` — and it composes: a second
 * theme merged after the first replaces the tokens it names and leaves the rest
 * alone, because the merge's unit is the property and a token is a property.
 *
 * Compile-time, for the same reason as `create`.
 */
export function createTheme<
  Tokens extends { readonly [string]: ThemeValue },
  Overrides extends ThemeOverrides<Tokens>,
>(tokens: Tokens, overrides: Overrides): CompiledStyle {
  return nativeRuntimeRequired(MODULE, "stylex.createTheme");
}

/**
 * The namespace form, so `stylex.create` and `stylex.props` read the way
 * StyleX documents them.
 *
 * The named exports are the ones a bundler can drop individually; this object
 * is for call sites that prefer the qualified spelling, and the compiler
 * recognises both.
 */
export const stylex: {
  readonly create: typeof create,
  readonly props: typeof props,
  readonly defineVars: typeof defineVars,
  readonly createTheme: typeof createTheme,
} = {
  create,
  props,
  defineVars,
  createTheme,
};
