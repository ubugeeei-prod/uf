// @flow
//
// React Native package integration that does not require the peer runtime.
//
// The root package re-exports `react-native`, so importing it would correctly
// need the app's peer dependency. The Metro helper is different: a
// `metro.config.js` needs to be able to import it while the app is still being
// assembled.

import { describe, expect, it } from "@uniflowed/test";
import {
  metroResolverMainFields,
  metroSourceExts,
  withUniflowedMetro,
} from "@uniflowed/react-native/metro";

describe("@uniflowed/react-native/metro", () => {
  it("names the source extensions from the native build contract", () => {
    expect(metroSourceExts).toEqual(["js", "jsx", "mjs", "cjs"]);
    expect(metroResolverMainFields).toEqual(["react-native", "browser", "main"]);
  });

  it("extends a Metro config without dropping project fields", () => {
    const config = withUniflowedMetro({
      cacheVersion: "app",
      resolver: {
        sourceExts: ["tsx", "js"],
        resolverMainFields: ["expo", "react-native"],
        unstable_conditionNames: ["react-native"],
      },
      transformer: {
        minifierPath: "metro-minify-terser",
      },
    });

    expect(config.cacheVersion).toBe("app");
    expect(config.transformer).toEqual({ minifierPath: "metro-minify-terser" });
    expect(config.resolver).toEqual({
      sourceExts: ["tsx", "js", "jsx", "mjs", "cjs"],
      resolverMainFields: ["expo", "react-native", "browser", "main"],
      unstable_conditionNames: ["react-native"],
    });
  });
});
