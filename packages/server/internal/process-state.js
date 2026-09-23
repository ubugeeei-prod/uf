// @flow
//
// Internal to `@uniflowed/server`: the state there is one of per process,
// whichever copy of this package holds it.
//
// A process can hold more than one copy of a module, and a uf application is
// about to hold two of this one on purpose. React Server Components render in
// a module graph resolved under the `react-server` export condition, which is a
// second graph beside the one that renders HTML (ubugeeei-prod/uf#519), and
// each graph that imports `@uniflowed/server` evaluates it once. Before that
// there were already accidental second copies: a host that resolved
// `@uniflowed/server/host` from its own `node_modules` rather than from the
// bundle it serves, which is the trap ubugeeei-prod/uf#389 found and every
// host's header still warns about.
//
// Almost everything in this package is correct duplicated — a function has no
// identity worth sharing. Three things are not, because each is a *place* two
// halves of one request have to agree on:
//
// * the request context `cookies()` and `after()` read (`./context.js`);
// * the cache scope `cacheLife()` declares into (`./cache-store.js`);
// * the process logger a host installs (`../log.js`).
//
// A second `AsyncLocalStorage` is not a second view of the request; it is a
// second request store that nothing ever entered. A page rendered by the other
// graph would find no request, call it "outside a request", and throw — or, for
// the logger, write through a default one onto a stdout that `uf dev` uses as a
// protocol. So those three are kept here, once per process, and every copy asks
// for the one that exists.
//
// # Why the key names a format version
//
// `Symbol.for` is a registry shared by everything in the realm, including a
// copy of `@uniflowed/server` from another *release* that some dependency
// pinned. Sharing a request context across a change to the context's shape
// would be a newer copy reading a field an older one never wrote, which fails
// later and less legibly than not sharing at all. The version in each name is
// bumped when what is stored under it changes shape, so two releases that
// disagree about the shape each keep their own, and two copies of one release
// share one.
//
// # Why not a module-level variable with a comment on it
//
// That is what each of the three was, and a comment does not make a second
// module instance see the first one's variable.

/** Every name this module registers starts with this, so it cannot collide. */
const NAMESPACE = "@uniflowed/server:";

/**
 * The value registered under `name`, created by `create` the first time any
 * copy of this package asks for it.
 *
 * Defined rather than assigned, and non-writable and non-configurable, so a
 * slot cannot be replaced once something holds it: the storage a request was
 * begun in must be the storage every later reader finds, and a copy that
 * swapped it would strand every request already in flight. What is stored is
 * therefore always a container — an `AsyncLocalStorage`, or a box a setter
 * writes into — rather than a value that is meant to change.
 *
 * Not enumerable, because nothing should find these by walking the global
 * object, and there is nothing in them a walk would be entitled to.
 */
export function processWide<T>(name: string, create: () => T): T {
  const key = Symbol.for(`${NAMESPACE}${name}`);
  const existing = Object.getOwnPropertyDescriptor(globalThis, key);
  if (existing != null) {
    // $FlowFixMe[incompatible-type] a slot is only ever written below, by `create` for the same name.
    return existing.value;
  }
  const value = create();
  Object.defineProperty(globalThis, key, {
    value,
    writable: false,
    enumerable: false,
    configurable: false,
  });
  return value;
}
