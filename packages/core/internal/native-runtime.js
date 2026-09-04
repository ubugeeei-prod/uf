// @flow
//
// Internal to `@uniflowed/core`.
//
// This module is deliberately absent from `package.json#exports`, so Node and
// every bundler that honours the exports map refuse to resolve
// `@uniflowed/core/internal/native-runtime`. Sibling modules reach it through a
// relative specifier, which the exports map does not gate.
//
// It exists so the "native runtime required" message is defined in exactly one
// place, and so the phantom carriers behind the package's opaque types share a
// single documented convention.

/**
 * Specifier of the shipped subpath a binding belongs to, for example
 * `@uniflowed/core/effect`.
 */
export type ModuleSpecifier = string;

/**
 * Phantom carrier for an opaque handle the native runtime owns.
 *
 * `Name` is a distinct string literal per handle, which keeps two unrelated
 * handles from unifying even inside the module that defines them.
 */
export type NativeHandle<Name extends string> = { readonly __ufNative: Name };

/**
 * Phantom carrier for an opaque handle with an invariant type parameter.
 *
 * `T` sits behind a writable property, which is precisely what invariance
 * means: `Handle<Dog>` is neither a subtype nor a supertype of `Handle<Animal>`.
 */
export type NativeHandleInvariant<Name extends string, T> = {
  readonly __ufNative: Name,
  __ufValue: T,
};

/**
 * Phantom carrier for an opaque handle with a covariant type parameter.
 *
 * `T` only ever appears in a return position, so `Handle<Dog>` stays assignable
 * to `Handle<Animal>` — the guarantee a `+T` sigil makes to callers.
 */
export type NativeHandleCovariant<Name extends string, out T> = {
  readonly __ufNative: Name,
  readonly __ufValue: () => T,
};

/**
 * Phantom carrier for a handle with two covariant type parameters.
 *
 * Same guarantee as `NativeHandleCovariant`, for a handle that tracks two
 * things at once — a fiber's success and failure types, say.
 */
export type NativeHandleCovariant2<Name extends string, out A, out B> = {
  readonly __ufNative: Name,
  readonly __ufFirst: () => A,
  readonly __ufSecond: () => B,
};

/**
 * Phantom carrier for a handle with three covariant type parameters.
 *
 * An effect tracks what it produces, how it fails, and what it needs; each sits
 * behind a function that returns it, which is what makes all three covariant
 * rather than merely declared so.
 */
export type NativeHandleCovariant3<Name extends string, out A, out B, out C> = {
  readonly __ufNative: Name,
  readonly __ufFirst: () => A,
  readonly __ufSecond: () => B,
  readonly __ufThird: () => C,
};

/**
 * Raised when a `@uniflowed/*` binding is reached outside the uf native runtime.
 *
 * The message names both the subpath and the binding, so a caller sees
 * `@uniflowed/core/effect: effect() requires the uf native runtime` rather than
 * a generic failure.
 */
export class NativeRuntimeRequiredError extends Error {
  /** Subpath the binding belongs to, e.g. `@uniflowed/core/effect`. */
  moduleSpecifier: ModuleSpecifier;
  /** Binding that was reached, e.g. `effect` or `Dialog.Root`. */
  binding: string;

  constructor(moduleSpecifier: ModuleSpecifier, binding: string) {
    super(`${moduleSpecifier}: ${binding}() requires the uf native runtime`);
    this.name = "NativeRuntimeRequiredError";
    this.moduleSpecifier = moduleSpecifier;
    this.binding = binding;
  }
}

/**
 * Raise `NativeRuntimeRequiredError` for one binding.
 *
 * Returns `empty`, Flow's bottom type, so a call site can `return` it from a
 * function of any declared return type without weakening that type to `any` or
 * `mixed`. Every shipped binding raises on *call*; importing a subpath stays
 * free of side effects so bundlers can drop the modules an application never
 * touches.
 */
export function nativeRuntimeRequired(moduleSpecifier: ModuleSpecifier, binding: string): empty {
  throw new NativeRuntimeRequiredError(moduleSpecifier, binding);
}
