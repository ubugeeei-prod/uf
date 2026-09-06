// @flow
//
// Internal to `@uniflowed/test`: standing a module in for another one.
//
// `uft.spyOn` replaces a method on an object a test can reach. This replaces a
// *module*, which is the only seam a great deal of a React application has: a
// component that imports a client and calls it at module scope, a page that
// imports `useRouter`, a `"use server"` module a client component calls. The
// mechanism belongs to the loader and lives in `@uniflowed/host`
// (`module-mocks.js`); what lives here is the API, the resolution of a
// specifier to the module it names, and the automatic stand-in.
//
// # When a mock takes effect
//
// **A mock is not hoisted.** Vitest lifts `vi.mock` above the importing file's
// `import` declarations with a Babel pass; uf has no Babel in its pipeline and
// `docs/architecture.md` says so on purpose, so inventing one for this would be
// a compiler pass whose only customer is a test helper. Bun answers the same
// question the same way, and it is the answer this package gives:
//
//   * every `import` declaration in a file runs before the file's first
//     statement, so a static import is *never* affected by a `uft.mock` written
//     below it — it already holds the real module;
//   * `uft.mock` takes effect for every import that begins after the promise it
//     returns settles;
//   * so `await import("./client.js")` — after the mock — is how a test reaches
//     the stand-in.
//
// A synchronous factory is installed before `uft.mock` returns, which means the
// rule holds whether or not the promise is awaited. Awaiting is still the habit
// worth having, because an automatic stand-in has to read the real module first
// and cannot be installed synchronously at all.
//
// # What a mock does not do
//
// A module that has already been evaluated is not re-written. Node's registry
// hands out a namespace that is linked into every importer, and no loader hook
// can reach back into it — Bun's engine can, and having one API mean two things
// on two hosts is worse than having it mean the narrower one on both. So a mock
// affects the next import, not the last one, and `uft.resetModules` is how a
// test gets a module evaluated again.

import {
  actualUrl,
  defineModuleMock,
  installInterception,
  interceptionSupported,
  removeModuleMock,
  resetModuleMocks,
  startModuleEpoch,
} from "@uniflowed/host/module-mocks";
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";

import { UnsupportedError } from "./unsupported.js";
import { frameFile, isInternalFrame } from "./frames.js";
import { fn } from "./spy.js";

/** The shape of a module's exports, as far as a type can say it. */
export type ModuleNamespace = { +[string]: mixed };

/**
 * What a `uft.mock` factory hands back.
 *
 * `Partial<Module>` rather than `Module` because a partial mock is the common
 * case — replace `send`, keep everything else — and rather than an unconstrained
 * object because the whole point of a Flow-first toolchain doing this is that
 * `{ send: 42 }` for a module whose `send` is a function is a type error at the
 * call rather than a `TypeError` three tests later.
 */
export type ModuleFactory<Module> = () => Partial<Module> | Promise<Partial<Module>>;

/** How deep an automatic stand-in follows nested objects. */
const AUTOMOCK_DEPTH = 5;

/**
 * Why module interception is unavailable here, or `null` when it is available.
 *
 * Named as a reason rather than a boolean because the reason is what a reader
 * needs: "this host has no synchronous module hooks" tells them the suite will
 * work on Node, and "not implemented" tells them nothing.
 */
export function moduleMockingUnavailable(): string | null {
  return interceptionSupported() ? null : unsupportedReason(hostName());
}

/** The name of the host this process is. */
export function hostName(): string {
  const host = globalThis as $FlowFixMe;
  if (host.Deno != null) {
    return "Deno";
  }
  if (host.process?.versions?.bun != null) {
    return "Bun";
  }
  return "this host";
}

/** What to tell someone whose host cannot intercept a module. */
export function unsupportedReason(host: string): string {
  return (
    `replacing a module before it is imported needs synchronous module ` +
    `customization hooks (\`node:module\`'s \`registerHooks\`), and ${host} does ` +
    `not provide them; run the suite on Node, or replace what the module ` +
    `hands out with \`uft.spyOn\` or \`@uniflowed/mock\` instead`
  );
}

/** Raise unless this host can intercept a module. */
function requireInterception(binding: string): void {
  const reason = moduleMockingUnavailable();
  if (reason != null) {
    throw new UnsupportedError(binding, reason);
  }
  installInterception();
}

/**
 * The module that called into this one.
 *
 * A specifier is relative to the file it was written in, so that file has to be
 * found, and a stack trace is the only place it is recorded. The frames are
 * `--enable-source-maps`'d back to the author's own file, so what comes back is
 * the path of the source rather than of the transform — which is exactly what a
 * relative specifier in that source should resolve against.
 */
function callerURL(binding: string): string {
  const stack = new Error("uft").stack;
  for (const frame of (stack ?? "").split("\n").slice(1)) {
    if (isInternalFrame(frame)) {
      continue;
    }
    const file = frameFile(frame);
    if (file == null) {
      continue;
    }
    if (file.startsWith("file:")) {
      const url = new URL(file);
      url.search = "";
      url.hash = "";
      return url.href;
    }
    if (file.startsWith("/")) {
      return pathToFileURL(file).href;
    }
  }
  throw new UnsupportedError(
    binding,
    "the calling file could not be read off the stack, so a relative " +
      "specifier has nothing to resolve against; pass an absolute path or a " +
      "`file:` URL",
  );
}

/**
 * The URL a specifier names, resolved from the file that wrote it.
 *
 * A path is resolved as a URL, which is what an ES module specifier is; a bare
 * specifier goes through the package resolver, so `@uniflowed/router` reaches
 * the same file an `import` of it would.
 */
export function resolveSpecifier(specifier: string, parentURL: string): string {
  if (specifier.startsWith("file:")) {
    return specifier;
  }
  if (isPathSpecifier(specifier)) {
    return new URL(specifier, parentURL).href;
  }
  try {
    return pathToFileURL(createRequire(parentURL).resolve(specifier)).href;
  } catch (error) {
    throw new Error(`uft could not resolve "${specifier}" from ${parentURL}: ${String(error)}`);
  }
}

/**
 * Register a stand-in for the module `specifier` names.
 *
 * With a factory, the factory is what the module exports. Without one, the real
 * module is read and every function it exports becomes a spy that records its
 * calls and returns `undefined` — the automatic form, for a module whose shape
 * a test wants to keep and whose behaviour it wants gone.
 */
export function mock<Module: ModuleNamespace>(
  specifier: string,
  factory?: ModuleFactory<Module>,
): Promise<void> {
  requireInterception("mock");
  const url = resolveSpecifier(specifier, callerURL("mock"));

  if (factory == null) {
    return importURL(actualUrl(url, isPathSpecifier(specifier))).then((actual) => {
      defineModuleMock(url, automock(actual, AUTOMOCK_DEPTH, new Map()));
    });
  }

  const produced = factory();
  if (isThenable(produced)) {
    return (produced: $FlowFixMe).then((namespace) => {
      defineModuleMock(url, exportsOf(specifier, namespace));
    });
  }
  defineModuleMock(url, exportsOf(specifier, produced));
  return Promise.resolve();
}

/** Stop standing in for the module `specifier` names. */
export function unmock(specifier: string): void {
  requireInterception("unmock");
  removeModuleMock(resolveSpecifier(specifier, callerURL("unmock")));
}

/**
 * The real module, whatever is registered for it.
 *
 * A separate instance from the one an ordinary import gets while the module is
 * mocked — there is only one URL per module instance, and the mocked one is
 * occupying the other. One instance per module epoch, so two calls in a row
 * hand back the same module.
 *
 * It reaches past the mock for the module it *names* and for no other: the
 * modules that one imports are resolved the way they would be anywhere else,
 * stand-ins included. That is what makes a partial mock possible — the factory
 * asks for the real module while its own stand-in is being built — and it is
 * also why `importActual` of a module that imports a mocked one still sees the
 * stand-in.
 */
export function importActual<Module: ModuleNamespace>(specifier: string): Promise<Module> {
  requireInterception("importActual");
  const url = resolveSpecifier(specifier, callerURL("importActual"));
  return importURL(actualUrl(url, isPathSpecifier(specifier)));
}

/**
 * The module with every function it exports replaced by a spy.
 *
 * The automatic form of `mock`, without registering anything: what comes back
 * is a stand-in the caller holds, and every other importer of that module still
 * gets whatever it got before.
 */
export function importMock<Module: ModuleNamespace>(specifier: string): Promise<Module> {
  requireInterception("importMock");
  const url = resolveSpecifier(specifier, callerURL("importMock"));
  return importURL(actualUrl(url, isPathSpecifier(specifier))).then(
    (actual) => (automock(actual, AUTOMOCK_DEPTH, new Map()): $FlowFixMe),
  );
}

/**
 * Evaluate modules again on the next import of them.
 *
 * "Modules" means the ones reached by a path — this project's own. A package is
 * left alone: handing a second copy of `@uniflowed/test` to a file would give
 * it a second registry and a second set of spies, and a reset that did that
 * would break far more than it fixed.
 *
 * It does not run a mock's factory again. The factory ran when `uft.mock` was
 * called; register the mock again to get new stand-ins out of it.
 */
export function resetModules(): void {
  requireInterception("resetModules");
  startModuleEpoch();
}

/**
 * Forget every mock and every reset.
 *
 * Called by the worker before each file, because a worker serves many files and
 * a mock that outlived its file would be a suite that passes alone and fails
 * beside another.
 */
export function resetModuleState(): void {
  resetModuleMocks();
}

/**
 * Whether `specifier` names a file rather than a package.
 *
 * The same three prefixes `@uniflowed/host`'s resolve hook tests for, and
 * deliberately not `file:`: a `file:` specifier names the URL it wants, and the
 * one module that writes one — the generated stand-in, reaching back for its
 * values — must land on the copy of the registry that put it there.
 */
function isPathSpecifier(specifier: string): boolean {
  return specifier.startsWith("./") || specifier.startsWith("../") || specifier.startsWith("/");
}

/** `import()`, in one place, so the marker parameter is never spelled twice. */
function importURL<Module>(url: string): Promise<Module> {
  return (import(url): $FlowFixMe);
}

/** Whether `value` is a promise, or near enough for `then` to be meant. */
function isThenable(value: mixed): boolean {
  return (
    value != null &&
    (typeof value === "object" || typeof value === "function") &&
    typeof (value: $FlowFixMe).then === "function"
  );
}

/**
 * The exports a factory produced, as a plain object.
 *
 * Copied rather than kept, so that the export list the loader writes out is
 * fixed at the moment the mock is registered: a factory that hands back an
 * object it goes on adding keys to would otherwise produce a module whose
 * exports depend on when it was first imported.
 */
function exportsOf(specifier: string, produced: mixed): { [string]: mixed } {
  if (produced == null || (typeof produced !== "object" && typeof produced !== "function")) {
    throw new TypeError(
      `uft.mock("${specifier}"): the factory must return the module's exports as an ` +
        `object, and it returned ${produced === null ? "null" : typeof produced}`,
    );
  }
  return { ...(produced: $FlowFixMe) };
}

/**
 * `value` with its behaviour taken out and its shape left in.
 *
 * The rules, which are the ones a reader has to be able to predict:
 *
 *   * a function becomes a spy that records its calls and returns `undefined`,
 *     keeping its name and its prototype's methods — so a class stays `new`-able
 *     and its methods are spies too;
 *   * an array becomes an empty array, because a fixture list a test did not
 *     write is a fixture list a test should not depend on;
 *   * a plain object is followed, key by key, to [`AUTOMOCK_DEPTH`];
 *   * a getter is kept as a getter rather than followed: reading one here would
 *     run code the module did not expect to run yet, and would fix an answer
 *     the getter exists to recompute;
 *   * everything else — a string, a number, a `Map`, a `Date`, a `RegExp` — is
 *     kept, because there is nothing in it to call.
 */
function automock(value: mixed, depth: number, seen: Map<mixed, mixed>): mixed {
  if (value == null || depth <= 0) {
    return value;
  }
  const already = seen.get(value);
  if (already !== undefined) {
    return already;
  }

  if (typeof value === "function") {
    const spy = fn().mockName((value: $FlowFixMe).name ?? "spy");
    seen.set(value, spy);
    copyProperties(value, spy, depth, seen, false);
    const prototype = (value: $FlowFixMe).prototype;
    if (prototype != null && typeof prototype === "object") {
      // Every own name, not only the enumerable ones: a class's methods are
      // non-enumerable own properties of its prototype, so `Object.keys` finds
      // none of them and the stand-in of a class would have no methods at all.
      copyProperties(prototype, spy.prototype, depth, seen, true);
    }
    return spy;
  }

  if (typeof value !== "object") {
    return value;
  }
  if (Array.isArray(value)) {
    const empty: Array<mixed> = [];
    seen.set(value, empty);
    return empty;
  }
  if (!isPlainish(value)) {
    return value;
  }

  const copy: { [string]: mixed } = {};
  seen.set(value, copy);
  copyProperties(value, copy, depth, seen, false);
  return copy;
}

/**
 * Automock the properties of `from` onto `onto`.
 *
 * `hidden` is what separates a prototype from everything else: a class's
 * methods are own but not enumerable, so a prototype needs every own name,
 * while an object or a function's statics want the enumerable ones — the same
 * set a spread or a module namespace would show.
 */
function copyProperties(
  from: mixed,
  onto: mixed,
  depth: number,
  seen: Map<mixed, mixed>,
  hidden: boolean,
): void {
  const source = (from: $FlowFixMe);
  const target = (onto: $FlowFixMe);
  const names = hidden ? Object.getOwnPropertyNames(source) : Object.keys(source);
  for (const name of names) {
    // `constructor` on a prototype points back at the function being mocked,
    // and rewriting it would replace the spy with a spy of the spy.
    if (name === "constructor") {
      continue;
    }
    const descriptor = Object.getOwnPropertyDescriptor(source, name);
    if (descriptor == null) {
      continue;
    }
    if (!Object.hasOwn(descriptor, "value")) {
      Object.defineProperty(target, name, descriptor);
      continue;
    }
    target[name] = automock(descriptor.value, depth - 1, seen);
  }
}

/**
 * Whether an object is one to look inside.
 *
 * A module namespace, an object literal and a `null`-prototype object are; a
 * `Map`, a `Date`, a `RegExp` and anything else with behaviour of its own are
 * not — replacing their methods with spies would produce something that claims
 * to be a `Date` and cannot tell the time.
 */
function isPlainish(value: mixed): boolean {
  const prototype = Object.getPrototypeOf(value);
  return prototype === null || prototype === Object.prototype || isNamespace(value);
}

/** Whether `value` is a module namespace object. */
function isNamespace(value: mixed): boolean {
  return (value: $FlowFixMe)[Symbol.toStringTag] === "Module";
}
