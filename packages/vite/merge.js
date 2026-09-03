// Plain JavaScript: executed by the host that runs Vite, before any transform.
//
// Merging a project's own Vite configuration over the one uf generates.
//
// This exists because of a specific failure. uf's config re-declared Vite's
// options one at a time — `host`, `port`, `strictPort`, `outDir`, `sourcemap`
// — and the driver copied them across by hand. Anything Vite could do that uf
// had not enumerated was unreachable until uf shipped a release naming it.
// That is `react-scripts`: an integrated tool becoming the chokepoint every
// upgrade in the ecosystem has to pass through.
//
// So `vite` in `uf.config.js` is Vite's own configuration, merged over uf's,
// and uf makes no attempt to understand it. An option added to Vite tomorrow
// works in a uf project tomorrow.
//
// # What uf still decides
//
// Three things are uf's rather than the project's, and the merge protects
// them:
//
//   * `plugins`, which are concatenated rather than replaced — dropping uf's
//     Flow transform would leave a project whose source no longer compiles,
//     which is not a thing anyone means to configure.
//   * `configFile`, because a `vite.config.ts` beside `uf.config.js` is two
//     files disagreeing about one project.
//   * `root`, which is the project uf resolved.
//
// Everything else is the project's to set, including options uf sets itself:
// a default is a convenience, not an architecture.

/** Keys uf owns outright, whatever the project's Vite config says. */
const RESERVED = ["root", "configFile"];

/**
 * Deep-merge `overrides` onto `base`.
 *
 * Plain objects merge key by key; arrays and everything else replace, which is
 * what a caller setting `build.rollupOptions.input` means. `plugins` is the
 * exception and is handled by the caller, because concatenating is right there
 * and replacing is right everywhere else.
 */
export function mergeConfig(base, overrides) {
  if (overrides == null) return base;
  const merged = { ...base };
  for (const key of Object.keys(overrides)) {
    const value = overrides[key];
    if (value === undefined) continue;
    merged[key] = isPlainObject(value) && isPlainObject(base[key])
      ? mergeConfig(base[key], value)
      : value;
  }
  return merged;
}

/**
 * uf's generated config, with the project's Vite config merged over it.
 *
 * @param {object} generated what uf built from the semantics it owns
 * @param {object | undefined} overrides `vite` from `uf.config.js`
 */
export function withProjectConfig(generated, overrides) {
  if (overrides == null) return generated;

  const owned = {};
  for (const key of RESERVED) owned[key] = generated[key];

  const merged = mergeConfig(generated, { ...overrides, ...owned });

  // uf's plugins first, then the project's. First because the Flow transform
  // has to see a module before anything that expects JavaScript does.
  merged.plugins = [
    ...(generated.plugins ?? []),
    ...(Array.isArray(overrides.plugins) ? overrides.plugins : []),
  ];
  return merged;
}

function isPlainObject(value) {
  return (
    typeof value === "object" &&
    value !== null &&
    !Array.isArray(value) &&
    // A plugin, a logger or a URL is a value to replace, not a shape to merge.
    (Object.getPrototypeOf(value) === Object.prototype ||
      Object.getPrototypeOf(value) === null)
  );
}
