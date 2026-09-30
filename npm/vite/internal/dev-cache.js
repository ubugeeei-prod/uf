// @noflow
//
// A cache scope for the renders `uf dev` answers with.
//
// `index.js` is loaded by Vite before any Flow transform, so it cannot import
// `@uniflowed/server/cache` — that file is Flow. This module is the one
// dynamic import. Production wraps the same renders in
// `runInScope(newScope({ key: [] }))` (`npm/server/fetch.js`): a component
// may call `cacheLife` whether or not a route cache is configured.
// `collectCacheDeclarations` is that scope, and it stores nothing. The scope
// lasts until `render` returns. ubugeeei-prod/uf#1703

/**
 * Run `render` inside the empty cache scope `uf start` uses, and return what
 * it produced.
 *
 * @template T
 * @param {() => Promise<T>} render
 * @returns {Promise<T>}
 */
export async function inDevCacheScope(render) {
  const { collectCacheDeclarations } = await import("@uniflowed/server/cache");
  const declared = await collectCacheDeclarations(render);
  return declared.value;
}
