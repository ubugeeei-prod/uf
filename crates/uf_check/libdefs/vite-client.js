/**
 * @fileoverview The client half of Vite, typed for Flow.
 *
 * A TypeScript project gets this from `vite/client`, referenced out of a
 * `vite-env.d.ts`. There is no Flow equivalent — not from Vite, which ships
 * TypeScript, and not from Flow, whose `core.js` types `import.meta` as
 * `{ [key: string]: unknown, url?: string }` and stops there. So the three
 * Vite APIs a real application actually uses were type errors:
 *
 *     Cannot call import.meta.glob because unknown [1] is not a function.
 *     Cannot call import.meta.hot.accept because property accept is missing.
 *     Cannot get import.meta.env.MODE because property MODE is missing.
 *
 * uf is the only project that would ship this file. Vite has no reason to
 * write Flow; Flow has no reason to know about Vite; a single project cannot
 * do it for itself, because Flow has no `/// <reference types>` and a libdef
 * is something the checker has to be told about. uf is the checker and the
 * bundler at once, so here it is.
 *
 * # How this replaces Flow's own definition
 *
 * `import.meta` is typed by looking up the name `Import$Meta` (see
 * `flow_typing_statement`'s `Expression::ImportMeta` case), and `core.js`
 * defines it as a plain type alias. Library definitions are merged in reverse
 * declaration order, so a later file's `Import$Meta` shadows an earlier one's
 * — this file is declared last in `crates/uf_check/src/upstream/
 * environments.rs`, which is the whole extension point.
 *
 * Everything `core.js` promised is still promised: `url` is still an optional
 * string, and the open `[key: string]: unknown` indexer is still there, so a
 * project reading some `import.meta` member uf has not heard of gets `unknown`
 * rather than a new error. Named members win over the indexer, so the three
 * below are typed properly.
 *
 * # One thing that does not work, and what to write instead
 *
 *     if (import.meta.hot) {
 *       import.meta.hot.accept(...);  // Error: property accept is missing.
 *     }
 *
 * That is the idiom every Vite guide shows, and Flow cannot check it. Its
 * refinements are keyed by a *lookup* — a base identifier, `this` or `super`,
 * plus a path of property projections (`flow_env_builder`'s
 * `Lookup::of_expression`). `import.meta` is a meta-property, not an
 * identifier, so `import.meta.hot` has no key, so testing it refines nothing.
 * That is upstream Flow's, and uf does not patch upstream Flow.
 *
 * Both of these do work, and both are shorter:
 *
 *     import.meta.hot?.accept(...);          // optional chaining needs no refinement
 *     const hot = import.meta.hot;           // a local identifier does have a key
 *     if (hot) hot.accept(...);
 *
 * # The dialect
 *
 * `readonly` rather than the `+` variance sigil, and overloads written as
 * repeated signatures, because that is what the vendored `lib/*.js` use and a
 * libdef that disagrees with its neighbours is a libdef nobody can edit.
 */

/**
 * A module namespace object, as `import.meta.glob` and `import.meta.hot` hand
 * one back.
 *
 * The exports are `unknown` rather than `any` because nobody — not Vite, not
 * uf — knows what is in a module matched by a glob until somebody says so.
 * Saying so is what the type argument on `glob` is for.
 */
type ModuleNamespace = { readonly [key: string]: unknown };

/**
 * The environment Vite substitutes at build time.
 *
 * The five named members are Vite's own and are always defined. Everything
 * else is a variable somebody put in a `.env` file, and it is `string | void`
 * rather than `string`, because the honest answer to "is `VITE_API_URL` set?"
 * is that nobody knows until the build runs — which is exactly the question a
 * type ought to make somebody ask.
 */
type ImportMetaEnv = {
  /** `"development"`, `"production"`, or whatever `--mode` said. */
  readonly MODE: string,
  /** The `base` the app is served under, always with a trailing slash. */
  readonly BASE_URL: string,
  /** True in a build. Not the negation of `DEV` in every mode. */
  readonly PROD: boolean,
  /** True in the dev server. */
  readonly DEV: boolean,
  /** True when this module is running on the server. */
  readonly SSR: boolean,
  /**
   * Anything else somebody put in a `.env` file.
   *
   * A type alias rather than an `interface` because an interface's indexer
   * does not answer a read of a property it did not name: `env.VITE_API_URL`
   * against an interface is `prop-missing`, and against this is
   * `string | void`. The five above keep their own types — a named member wins
   * over the indexer, which is why `PROD` stays a boolean here.
   */
  readonly [key: string]: string | void,
};

/**
 * What Vite hands a callback when this module is replaced. `void` when the new
 * copy threw, which is why every one of these is checked before it is used.
 */
type HotModuleCallback = (module: ModuleNamespace | void) => mixed;

/** What Vite hands a listener registered with `on`. */
type HotEventCallback = (payload: unknown) => mixed;

/**
 * The hot-module-replacement handle, which exists only in a dev server.
 *
 * `import.meta.hot` is `undefined` in a build — Vite replaces the expression
 * and drops the block — so this is an optional member on `import.meta` and
 * reaching through it needs `?.`. See the note at the top of this file about
 * why `if (import.meta.hot)` is not the way to get there.
 *
 * `decline` is deliberately absent: upstream deprecated it and it is a no-op.
 *
 * Every callback here returns `mixed` rather than `void`, following
 * `EventHandler` in Flow's own `dom.js`. Vite discards what these return, and
 * a `void` return in Flow — unlike in TypeScript — rejects the value, so
 * `hot.accept((m) => setModule(m))` would be an error about a program that is
 * correct. `mixed` accepts nothing a `void` would have caught: there is no
 * caller to be wrong about the value.
 */
interface ImportMetaHot {
  /** Survives a hot update, so a module can carry state across one. */
  readonly data: { [key: string]: unknown };

  /** Accept an update to this module and re-run it. */
  accept(): void;
  /** Accept an update to this module and handle the new copy. */
  accept(callback: HotModuleCallback): void;
  /** Accept an update to one dependency. */
  accept(dependency: string, callback: HotModuleCallback): void;
  /** Accept an update to several dependencies, in the order given. */
  accept(
    dependencies: ReadonlyArray<string>,
    callback: (modules: ReadonlyArray<ModuleNamespace | void>) => mixed,
  ): void;

  /** Clean up before this module is replaced. */
  dispose(callback: (data: { [key: string]: unknown }) => mixed): void;
  /** Clean up when this module is dropped rather than replaced. */
  prune(callback: (data: { [key: string]: unknown }) => mixed): void;
  /** Refuse this update and hand it to the importers, with a reason. */
  invalidate(message?: string): void;

  /** Listen for an HMR event from the server. */
  on(event: string, callback: HotEventCallback): void;
  /** Stop listening. The callback must be the one that was passed to `on`. */
  off(event: string, callback: HotEventCallback): void;
  /** Send a custom event to the server. */
  send(event: string, data?: unknown): void;
}

/** What `import.meta.glob` accepts besides the pattern. */
interface ImportGlobOptions {
  /** Import one named export from each match instead of the namespace. */
  readonly import?: string;
  /** Appended to each specifier, so `?raw` and friends work through a glob. */
  readonly query?: string | { readonly [key: string]: string | number | boolean };
  /** Resolve the patterns against this directory rather than the importer. */
  readonly base?: string;
  /** Match paths case-sensitively. Added in Vite 8.1. */
  readonly caseSensitive?: boolean;
}

/** `import.meta.glob(...)`, the lazy default. */
interface ImportGlobLazyOptions extends ImportGlobOptions {
  /** Import every match up front. `true` changes what `glob` returns. */
  readonly eager?: false | void;
}

/** `import.meta.glob(..., { eager: true })`. */
interface ImportGlobEagerOptions extends ImportGlobOptions {
  readonly eager: true;
}

/**
 * `import.meta.glob`, in both of its shapes.
 *
 * Two signatures rather than one with an optional flag, because the return
 * type depends on the *value* of `eager`, and an overload is how this dialect
 * says that. Lazy is written first: it is the default, and a call with no
 * options has to land on it.
 *
 * `T` defaults to a namespace of `unknown` exports rather than to `any`. A
 * call that wants more says so — `import.meta.glob<PageModule>("./pages/*.js")`
 * — which is the whole reason this is generic.
 */
interface ImportGlobFunction {
  <T = ModuleNamespace>(
    pattern: string | ReadonlyArray<string>,
    options?: ImportGlobLazyOptions,
  ): { [key: string]: () => Promise<T> };
  <T = ModuleNamespace>(
    pattern: string | ReadonlyArray<string>,
    options: ImportGlobEagerOptions,
  ): { [key: string]: T };
}

type Import$Meta = {
  [key: string]: unknown,
  url?: string,
  readonly env: ImportMetaEnv,
  readonly hot?: ImportMetaHot,
  readonly glob: ImportGlobFunction,
};
