// @flow
//
// `@uniflowed/react-native/metro`: the Metro half of uf's native target.
//
// `uf build --target native` writes the same extension list into the build
// manifest's target contract. This module gives a React Native application a
// package subpath it can import from `metro.config.js`, so the contract is not
// only something a completed build reports after the fact.

export type MetroResolverConfig = {
  readonly sourceExts?: $ReadOnlyArray<string>,
  readonly resolverMainFields?: $ReadOnlyArray<string>,
  readonly [key: string]: mixed,
};

export type MetroConfig = {
  readonly resolver?: MetroResolverConfig,
  readonly [key: string]: mixed,
};

export const metroSourceExts: $ReadOnlyArray<string> = Object.freeze(["js", "jsx", "mjs", "cjs"]);
export const metroResolverMainFields: $ReadOnlyArray<string> = Object.freeze([
  "react-native",
  "browser",
  "main",
]);

/**
 * Add uf's Flow-first JavaScript extensions to a Metro config.
 *
 * Metro already owns platform suffix resolution (`.ios.js`, `.android.js`,
 * `.native.js`). uf's route scanner and build manifest use the same suffixes,
 * so this helper only names the source extensions and resolver main fields a
 * Metro app needs to consume `@uniflowed/*` Flow packages without dropping any
 * app-specific extensions the project already configured.
 */
export function withUniflowedMetro(config?: MetroConfig = {} as MetroConfig): MetroConfig {
  const resolver = config.resolver ?? {};
  return {
    ...config,
    resolver: {
      ...resolver,
      sourceExts: mergeUnique(resolver.sourceExts, metroSourceExts),
      resolverMainFields: mergeUnique(resolver.resolverMainFields, metroResolverMainFields),
    },
  };
}

function mergeUnique(
  existing: void | $ReadOnlyArray<string>,
  required: $ReadOnlyArray<string>,
): $ReadOnlyArray<string> {
  const merged: Array<string> = [];
  const seen: Set<string> = new Set();
  for (const value of [...(existing ?? []), ...required]) {
    if (seen.has(value)) continue;
    seen.add(value);
    merged.push(value);
  }
  return merged;
}
