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

export type MetroTransformerConfig = {
  readonly babelTransformerPath?: string,
  readonly [key: string]: mixed,
};

export type MetroConfig = {
  readonly resolver?: MetroResolverConfig,
  readonly transformer?: MetroTransformerConfig,
  readonly [key: string]: mixed,
};

export const metroSourceExts: $ReadOnlyArray<string> = Object.freeze(["js", "jsx", "mjs", "cjs"]);
export const metroResolverMainFields: $ReadOnlyArray<string> = Object.freeze([
  "react-native",
  "browser",
  "main",
]);
export const metroTransformPipeline: $ReadOnlyArray<string> = Object.freeze([
  "flow",
  "react-compiler",
  "stylex-css-refusal",
  "metro-babel",
]);
export const metroTransformerPath: string = filePathFromFileUrl(
  new URL("./metro-transformer.cjs", import.meta.url),
);

/**
 * Add uf's Flow-first JavaScript extensions and transformer to a Metro config.
 *
 * Metro already owns platform suffix resolution (`.ios.js`, `.android.js`,
 * `.native.js`). uf's route scanner and build manifest use the same suffixes,
 * so this helper names the source extensions, resolver main fields and the
 * single Metro Babel transformer that runs uf's Flow pipeline before Metro's
 * React Native Babel pass.
 */
export function withUniflowedMetro(config?: MetroConfig = {} as MetroConfig): MetroConfig {
  const resolver: MetroResolverConfig = config.resolver ?? {};
  const transformer: MetroTransformerConfig = config.transformer ?? {};
  const existing = transformer.babelTransformerPath;
  if (existing != null && existing !== metroTransformerPath) {
    throw new Error(
      "@uniflowed/react-native/metro: transformer.babelTransformerPath is already set. " +
        "Metro accepts one Babel transformer, so compose that transformer with " +
        "@uniflowed/react-native/metro-transformer.cjs instead of silently replacing it.",
    );
  }
  return {
    ...config,
    resolver: {
      ...resolver,
      sourceExts: mergeUnique(resolver.sourceExts, metroSourceExts),
      resolverMainFields: mergeUnique(resolver.resolverMainFields, metroResolverMainFields),
    },
    transformer: {
      ...transformer,
      babelTransformerPath: metroTransformerPath,
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

function filePathFromFileUrl(fileUrl: URL): string {
  const pathname = decodeURIComponent(fileUrl.pathname);
  if (fileUrl.hostname !== "") return `//${fileUrl.hostname}${pathname}`;
  if (/^\/[A-Za-z]:\//.test(pathname)) return pathname.slice(1);
  return pathname;
}
