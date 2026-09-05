// @flow
//
// `@uniflowed/stylex/props`: the merge, and the only part of StyleX that runs.
//
// `uf transform` rewrites every `stylex.create({ … })` and
// `stylex.createTheme(tokens, { … })` into a plain object of class names, so by
// the time this module is loaded there are no style values left. What is left
// is the merge, and it is here because it cannot be anywhere else: a call site
// writes `active && styles.on`, and a compiler cannot fold a value it does not
// know.
//
// # This file has a twin
//
// The same merge is modelled in `crates/uf_stylex/src/props.rs`, at compile
// time, which is what lets its ordering be tested against the sheet the
// compiler emits. The two have to agree, and they agree on the two things that
// are observable:
//
// * **which classes survive.** The *property* is the unit of merging — a later
//   namespace that sets `color` replaces everything an earlier one said about
//   `color`, its `:hover` value included. That is what a later `color:` in a
//   stylesheet does, and it is why a later namespace cannot leave a stray
//   hover state behind.
// * **that every class of a surviving property survives.** A property written
//   with states compiles to a *map* — `{ "default": "x1", ":hover": "x2" }` —
//   and both classes belong in the class attribute. A runtime that kept only
//   the first, or skipped the map because it is not a string, would silently
//   drop every conditional style in an application while every test that used
//   plain values kept passing.
//
// They deliberately do not agree on the *order* of the class attribute, and
// nothing depends on it: the compiler sorts globally by sheet position because
// it has the priorities to hand, this file emits in the order properties were
// first claimed because it does not. Which rule wins is decided by the sheet,
// never by the attribute, so the two orders render identically. Within one
// property the orders do match, because the compiler writes that property's
// states in sheet order and this file reads them in the order it finds them.

/** The class names one property sets, keyed by the state each applies in. */
export type CompiledClasses = { readonly [state: string]: string | null };

/**
 * A compiled style namespace.
 *
 * `$$css` marks an object the compiler produced. Every other key is a CSS
 * property — or a custom property, for a theme — mapped to the class name that
 * sets it, to a map of class names when the property has states, or to `null`,
 * which is how a namespace says it deliberately unsets that property.
 */
export type CompiledStyle = {
  readonly $$css: true,
  readonly [property: string]: string | null | true | CompiledClasses,
};

/** What a call site may pass: a namespace, something falsy, or a list. */
export type StyleArgument = mixed;

/** What `props` hands to an element. */
export type StyleProps = { readonly className?: string };

/** What one property contributed, once the merge has picked a winner. */
type Winner = string | null | CompiledClasses;

/**
 * Merge compiled namespaces into a `className`, left to right.
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
  const winners: { [string]: Winner } = {};
  collect(styles, winners);

  let className = "";
  for (const property of Object.keys(winners)) {
    const winner = winners[property];
    // `null` is a deliberate unset: the property has an owner, and that owner
    // said there should be no class for it.
    if (winner == null) {
      continue;
    }
    if (typeof winner === "string") {
      className = join(className, winner);
      continue;
    }
    // A property with states. Every one of its classes belongs in the
    // attribute: the base rule and the `:hover` rule are separate rules in the
    // sheet, and dropping either is a style that never applies.
    for (const state of Object.keys(winner)) {
      const name = winner[state];
      if (name != null) {
        className = join(className, name);
      }
    }
  }

  return className === "" ? {} : { className };
}

/**
 * Fold arguments into `winners`, flattening arrays.
 *
 * Insertion order is the order a property was *first* claimed, and assigning
 * over an existing key does not move it — so two namespaces that both set
 * `color` produce one entry in the position the first one had. The class list
 * is a function of the properties involved, not of how many namespaces
 * mentioned them.
 */
function collect(styles: $ReadOnlyArray<StyleArgument>, winners: { [string]: Winner }): void {
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
    // The one place an untyped value enters. This object was written by `uf
    // transform`, and its shape is the compiler's promise rather than something
    // Flow can see from a call site; everything below reads it through
    // `CompiledStyle`, so the trust boundary is this line and no wider.
    const namespace: CompiledStyle = style as $FlowFixMe;
    for (const property of Object.keys(namespace)) {
      const value = namespace[property];
      // `$$css` is the marker, not a property. Skipping it by value as well as
      // by name means a plain object that happens to carry `true` somewhere
      // cannot put `true` into a class attribute.
      if (property === "$$css" || value === true) {
        continue;
      }
      winners[property] = value;
    }
  }
}

/** Append one class name to a space-separated attribute. */
function join(className: string, name: string): string {
  return className === "" ? name : className + " " + name;
}
