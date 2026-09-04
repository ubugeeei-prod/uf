// @flow
//
// `@uniflowed/stylex`: uf's style engine, as the thing that runs.
//
// Almost all of StyleX happens at compile time. `uf transform` rewrites every
// `stylex.create({ … })` into a plain object of class names and collects the
// rules into a stylesheet, so by the time this module is loaded there are no
// style values left — only names.
//
// What remains is `props`, and it is here because it cannot be anywhere else:
// its arguments are usually conditional (`active && styles.on`), and a compiler
// cannot fold a value it does not know. Everything else in this module exists
// so that a call the compiler failed to see fails loudly rather than silently
// rendering an application with no styles.
//
// The merge is specified in `crates/uf_stylex/src/props.rs` and modelled there
// at compile time, which is what lets its ordering be tested. This
// implementation and that model have to agree.

import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/stylex";

/**
 * A compiled style namespace.
 *
 * `$$css` marks an object the compiler produced. Every other key is a CSS
 * property mapped to the class name that sets it — or to `null`, which is how
 * a namespace says it deliberately unsets that property.
 */
export type CompiledStyle = {
  readonly $$css: true,
  readonly [property: string]: string | null | true,
};

/** What a call site may pass: a namespace, something falsy, or a list. */
export type StyleArgument = mixed;

/** What `props` hands to an element. */
export type StyleProps = { readonly className?: string };

/**
 * Merge compiled namespaces into a `className`, left to right.
 *
 * The **property** is the unit of merging: a later namespace that sets `color`
 * replaces everything an earlier one said about `color`, its `:hover` value
 * included. That is what a later `color:` in a stylesheet does, and it is why a
 * later namespace cannot leave a stray hover state behind.
 *
 * Falsy arguments are skipped, because `active && styles.on` is the idiom this
 * function exists for, and arrays are flattened so a list built elsewhere can
 * be passed without spreading it.
 *
 * Returns an object rather than a string so the call site stays
 * `<div {...stylex.props(a, b)} />` — the same shape whether or not anything
 * survived.
 */
export function props(...styles: $ReadOnlyArray<StyleArgument>): StyleProps {
  const winners: { [string]: string | null } = {};
  collect(styles, winners);

  let className = "";
  for (const property of Object.keys(winners)) {
    const name = winners[property];
    // `null` is a deliberate unset: the property has an owner, and that owner
    // said there should be no class for it.
    if (name == null) {
      continue;
    }
    className = className === "" ? name : className + " " + name;
  }

  return className === "" ? {} : { className };
}

/**
 * Fold arguments into `winners`, flattening arrays.
 *
 * Insertion order is the order a property was *first* claimed, and assigning
 * over an existing key does not move it — so two namespaces that both set
 * `color` produce one class in the position the first one had. The class list
 * is a function of the properties involved, not of how many namespaces
 * mentioned them.
 */
function collect(
  styles: $ReadOnlyArray<StyleArgument>,
  winners: { [string]: string | null },
): void {
  for (const style of styles) {
    if (style == null || style === false || style === true) {
      continue;
    }
    if (Array.isArray(style)) {
      collect(style, winners);
      continue;
    }
    if (typeof style !== "object") {
      continue;
    }
    const namespace = style as $FlowFixMe;
    for (const property of Object.keys(namespace)) {
      if (property === "$$css") {
        continue;
      }
      const value = namespace[property];
      if (typeof value === "string" || value === null) {
        winners[property] = value;
      }
    }
  }
}

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
export function defineVars<T extends { readonly [string]: string | number }>(tokens: T): T {
  return nativeRuntimeRequired(MODULE, "stylex.defineVars");
}

/**
 * Override a set of tokens for a subtree.
 *
 * Compile-time, for the same reason as `create`.
 */
export function createTheme<T extends { readonly [string]: string | number }>(tokens: T): T {
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
