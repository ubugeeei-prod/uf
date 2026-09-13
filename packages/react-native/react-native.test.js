// @flow
//
// React Native package integration that does not require the peer runtime.
//
// The root package re-exports `react-native`, so importing it would correctly
// need the app's peer dependency. The Metro helper is different: a
// `metro.config.js` needs to be able to import it while the app is still being
// assembled.

import fs from "node:fs";
import { createRequire } from "node:module";
import os from "node:os";
import path from "node:path";

import { describe, expect, it } from "@uniflowed/test";
import {
  metroResolverMainFields,
  metroSourceExts,
  metroTransformPipeline,
  metroTransformerPath,
  withUniflowedMetro,
} from "@uniflowed/react-native/metro";

const load = createRequire(import.meta.url);

type MetroTransformInput = {
  src: string,
  filename: string,
  options: {
    dev?: boolean,
    projectRoot?: string,
    readonly [key: string]: mixed,
  },
};

type MetroTransformResult = {
  metadata: {
    uniflowedSource: string,
    readonly [key: string]: mixed,
  },
  readonly [key: string]: mixed,
};

type MetroTransformerModule = {
  transform: (input: MetroTransformInput) => Promise<MetroTransformResult>,
  getCacheKey: (options: { readonly [key: string]: mixed }) => string,
};

describe("@uniflowed/react-native/metro", () => {
  it("names the source extensions and transform order from the native build contract", () => {
    expect(metroSourceExts).toEqual(["js", "jsx", "mjs", "cjs"]);
    expect(metroResolverMainFields).toEqual(["react-native", "browser", "main"]);
    expect(metroTransformPipeline).toEqual([
      "flow",
      "react-compiler",
      "stylex-css-refusal",
      "metro-babel",
    ]);
    expect(metroTransformerPath.endsWith("metro-transformer.cjs")).toBe(true);
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
    expect(config.transformer).toEqual({
      minifierPath: "metro-minify-terser",
      babelTransformerPath: metroTransformerPath,
    });
    expect(config.resolver).toEqual({
      sourceExts: ["tsx", "js", "jsx", "mjs", "cjs"],
      resolverMainFields: ["expo", "react-native", "browser", "main"],
      unstable_conditionNames: ["react-native"],
    });
  });

  it("refuses to silently replace an existing Metro Babel transformer", () => {
    expect(() =>
      withUniflowedMetro({
        transformer: {
          babelTransformerPath: "/app/custom-transformer.cjs",
        },
      }),
    ).toThrow("transformer.babelTransformerPath is already set");
  });

  it("runs Flow through uf before handing the module to Metro's Babel transformer", async () => {
    await withMetroTransformerProject(async ({ root }) => {
      const transformer = loadMetroTransformer();
      const result = await transformer.transform({
        src:
          "// @flow\n" +
          'import { useState } from "react";\n' +
          "export component NativeCounter(label: string) {\n" +
          "  const [count] = useState(1);\n" +
          "  return <Text>{label}{count}</Text>;\n" +
          "}\n",
        filename: "app/NativeCounter.js",
        options: {
          dev: true,
          projectRoot: root,
        },
      });

      const source = result.metadata.uniflowedSource;
      expect(source).toContain("function NativeCounter(");
      expect(source).toContain("react/compiler-runtime");
      expect(source).not.toContain("component NativeCounter");
      expect(source).not.toContain("label: string");
    });
  });

  it("refuses StyleX CSS instead of dropping it from a native Metro bundle", async () => {
    await withMetroTransformerProject(async ({ root }) => {
      const transformer = loadMetroTransformer();
      await expect(
        transformer.transform({
          src:
            "// @flow\n" +
            'import { stylex } from "@uniflowed/stylex";\n' +
            'const styles = stylex.create({ root: { color: "red" } });\n' +
            "export const root = styles.root;\n",
          filename: "app/styles.js",
          options: {
            dev: false,
            projectRoot: root,
          },
        }),
      ).rejects.toThrow("produced StyleX CSS");
    });
  });

  it("builds a Metro cache key from uf and upstream transformer identities", async () => {
    await withMetroTransformerProject(async () => {
      const transformer = loadMetroTransformer();
      const key = transformer.getCacheKey({ dev: true });

      expect(key).toContain("uniflowed-react-native-metro-transformer-v1");
      expect(key).toContain("upstream-test-transformer");
    });
  });
});

function loadMetroTransformer(): MetroTransformerModule {
  delete load.cache[metroTransformerPath];
  return load(metroTransformerPath);
}

async function withMetroTransformerProject(
  body: ({ root: string, upstream: string }) => Promise<void>,
): Promise<void> {
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "uf-metro-transform-")));
  const previous = process.env.UF_METRO_UPSTREAM_TRANSFORMER;
  try {
    fs.writeFileSync(path.join(root, "uf.config.js"), "export default {};\n");
    const upstream = path.join(root, "upstream-transformer.cjs");
    fs.writeFileSync(
      upstream,
      "module.exports.transform = async ({ src, filename, options }) => ({\n" +
        "  ast: { type: 'Program', sourceType: 'module', body: [] },\n" +
        "  metadata: { uniflowedSource: src, filename, dev: options.dev === true },\n" +
        "});\n" +
        "module.exports.getCacheKey = () => 'upstream-test-transformer';\n",
    );
    process.env.UF_METRO_UPSTREAM_TRANSFORMER = upstream;
    await body({ root, upstream });
  } finally {
    if (previous == null) {
      delete process.env.UF_METRO_UPSTREAM_TRANSFORMER;
    } else {
      process.env.UF_METRO_UPSTREAM_TRANSFORMER = previous;
    }
    fs.rmSync(root, { recursive: true, force: true });
  }
}
