// @flow
//
// React Native package integration that does not require the peer runtime.
//
// The root package re-exports `react-native`, so importing it would correctly
// need the app's peer dependency. The Metro half is different twice over: a
// `metro.config.js` imports it while the app is still being assembled, and it
// imports it from plain Node — `expo start`, `react-native start`, Xcode's and
// Gradle's bundle phases — where none of uf's loader hooks are installed. This
// suite runs under those hooks, so the one test that matters most here spawns a
// process without them.

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import { createRequire } from "node:module";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";

import { describe, expect, it } from "@uniflowed/test";

const load = createRequire(import.meta.url);

// Through `require`, the way Metro's own process reaches the helper. uf's loader
// hooks hand every module they transform to Node as an ES module — right for
// Flow, and not what a `.cjs` file is — so an `import` here would test the
// hooks rather than the helper. An `import` from plain Node is the subject of
// "loads in plain Node" below.
const {
  metroResolverMainFields,
  metroSourceExts,
  metroTransformPipeline,
  metroTransformerPath,
  withUniflowedMetro,
} = load("@uniflowed/react-native/metro");

type MetroTransformInput = {
  src: string,
  filename: string,
  options: {
    dev?: boolean,
    projectRoot?: string,
    readonly [key: string]: mixed,
  },
  plugins?: $ReadOnlyArray<mixed>,
};

type MetroTransformResult = {
  metadata: {
    upstream: string,
    uniflowedSource: string,
    plugins: $ReadOnlyArray<string>,
    readonly [key: string]: mixed,
  },
  readonly [key: string]: mixed,
};

type MetroTransformerModule = {
  transform: (input: MetroTransformInput) => Promise<MetroTransformResult>,
  getCacheKey: (options: { readonly [key: string]: mixed }) => string,
};

type MetroProject = {
  root: string,
  upstream: string,
};

const FLOW_COMPONENT =
  "// @flow\n" +
  'import { useState } from "react";\n' +
  "export component NativeCounter(label: string) {\n" +
  "  const [count] = useState(1);\n" +
  "  return <Text>{label}{count}</Text>;\n" +
  "}\n";

describe("@uniflowed/react-native/metro", () => {
  it("names the source extensions and transform order from the native build contract", () => {
    expect(metroSourceExts).toEqual(["js", "jsx", "mjs", "cjs"]);
    expect(metroResolverMainFields).toEqual(["react-native", "browser", "main"]);
    expect(metroTransformPipeline).toEqual([
      "flow",
      "react-compiler",
      "stylex-native-objects",
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
    const { resolveRequest, ...resolverFields } = config.resolver;
    expect(typeof resolveRequest).toBe("function");
    expect(resolverFields).toEqual({
      sourceExts: ["tsx", "js", "jsx", "mjs", "cjs"],
      resolverMainFields: ["expo", "react-native", "browser", "main"],
      unstable_conditionNames: ["react-native"],
    });
  });

  it("loads in plain Node, from a CommonJS config and from an ES module config", async () => {
    await withMetroProject({ overrideUpstream: false }, ({ upstream }) => {
      const helper = path.join(path.dirname(metroTransformerPath), "metro.cjs");
      const report =
        "process.stdout.write(JSON.stringify({" +
        " path: config.transformer.babelTransformerPath," +
        " ours: metroTransformerPath," +
        " upstream: process.env.UF_METRO_CONFIGURED_UPSTREAM," +
        ` transform: typeof require(${JSON.stringify(metroTransformerPath)}).transform,` +
        " }));";
      const composed = `withUniflowedMetro({ transformer: { babelTransformerPath: ${JSON.stringify(upstream)} } })`;
      const commonjs = runPlainNode([
        "-e",
        `const { withUniflowedMetro, metroTransformerPath } = require(${JSON.stringify(helper)});\n` +
          `const config = ${composed};\n` +
          report,
      ]);
      const esm = runPlainNode([
        "--input-type=module",
        "-e",
        'import { createRequire } from "node:module";\n' +
          `import { withUniflowedMetro, metroTransformerPath } from ${JSON.stringify(pathToFileURL(helper).href)};\n` +
          "const require = createRequire(import.meta.url);\n" +
          `const config = ${composed};\n` +
          report,
      ]);

      for (const run of [commonjs, esm]) {
        expect(run.stderr).toBe("");
        expect(run.status).toBe(0);
        expect(JSON.parse(run.stdout)).toEqual({
          path: metroTransformerPath,
          ours: metroTransformerPath,
          upstream,
          transform: "function",
        });
      }
    });
  });

  it("refuses DOM packages at resolution and preserves a project's custom resolver", () => {
    const calls = [];
    const config = withUniflowedMetro({
      resolver: {
        resolveRequest(context, name, platform) {
          calls.push([name, platform]);
          return { type: "sourceFile", filePath: `/app/${name}.js` };
        },
      },
    });
    for (const name of ["@uniflowed/ui", "@uniflowed/web", "@uniflowed/router"]) {
      expect(() => config.resolver.resolveRequest({}, name, "ios")).toThrow("needs a browser DOM");
    }
    const subpaths = ["native", "native-navigation", "http-client", "routing"];
    for (const subpath of subpaths) {
      const name = `@uniflowed/router/${subpath}`;
      expect(config.resolver.resolveRequest({}, name, "ios").filePath).toBe(`/app/${name}.js`);
    }
    expect(calls).toEqual(subpaths.map((subpath) => [`@uniflowed/router/${subpath}`, "ios"]));
  });

  it("restores Metro's omitted hot flag for React Refresh and honours an explicit false", async () => {
    await withMetroProject({ overrideUpstream: true }, async ({ root }) => {
      for (const hot of [undefined, false]) {
        const result = await loadMetroTransformer().transform({
          src: FLOW_COMPONENT,
          filename: "app/NativeCounter.js",
          options: { dev: true, hot, projectRoot: root },
        });
        expect(result.metadata.hot).toBe(hot !== false);
      }
    });
  });

  it("records the uncomposed source-map gap: upstream locations refer to lowered Flow output", async () => {
    await withMetroProject({ overrideUpstream: true }, async ({ root }) => {
      const result = await loadMetroTransformer().transform({
        src: FLOW_COMPONENT,
        filename: "app/NativeCounter.js",
        options: { dev: true, projectRoot: root },
      });
      expect(result.metadata.uniflowedSource).toContain("function NativeCounter(");
      expect(result.metadata.uniflowedSource).not.toContain("component NativeCounter(");
      expect(result.metadata.inputSourceMap).toBe(null);
    });
  });

  it("composes with the transformer a default config already names, and runs it after uf's", async () => {
    await withMetroProject({ overrideUpstream: false }, async ({ root, upstream }) => {
      const config = withUniflowedMetro({
        projectRoot: root,
        transformer: { babelTransformerPath: upstream, unstable_workerThreads: true },
      });

      expect(config.transformer).toEqual({
        babelTransformerPath: metroTransformerPath,
        unstable_workerThreads: true,
      });
      expect(process.env.UF_METRO_CONFIGURED_UPSTREAM).toBe(upstream);

      const result = await loadMetroTransformer().transform({
        src: FLOW_COMPONENT,
        filename: "app/NativeCounter.js",
        options: { dev: true, projectRoot: root },
      });
      expect(result.metadata.upstream).toBe("upstream-test-transformer");
      expect(result.metadata.uniflowedSource).toContain("function NativeCounter(");
    });
  });

  it("treats a config that is already composed as composed once", async () => {
    await withMetroProject({ overrideUpstream: false }, ({ root, upstream }) => {
      const once = withUniflowedMetro({
        projectRoot: root,
        transformer: { babelTransformerPath: upstream },
      });
      const twice = withUniflowedMetro(once);

      expect(twice.transformer.babelTransformerPath).toBe(metroTransformerPath);
      expect(process.env.UF_METRO_CONFIGURED_UPSTREAM).toBe(upstream);
    });
  });

  it("resolves a transformer the config names by package from the project root", async () => {
    await withMetroProject({ overrideUpstream: false }, ({ root }) => {
      writeProjectFile(
        root,
        "node_modules/app-transformer/package.json",
        '{ "name": "app-transformer", "main": "index.js" }\n',
      );
      writeProjectFile(
        root,
        "node_modules/app-transformer/index.js",
        fakeTransformer("app-transformer"),
      );

      withUniflowedMetro({
        projectRoot: root,
        transformer: { babelTransformerPath: "app-transformer" },
      });

      expect(process.env.UF_METRO_CONFIGURED_UPSTREAM).toBe(
        path.join(root, "node_modules/app-transformer/index.js"),
      );
    });
  });

  it("refuses a transformer the config names when it is not installed", async () => {
    await withMetroProject({ overrideUpstream: false }, ({ root }) => {
      expect(() =>
        withUniflowedMetro({
          projectRoot: root,
          transformer: { babelTransformerPath: "missing-transformer" },
        }),
      ).toThrow("names missing-transformer, which does not resolve from");
    });
  });

  it("runs Flow through uf before handing the module to Metro's Babel transformer", async () => {
    await withMetroProject({ overrideUpstream: true }, async ({ root }) => {
      const result = await loadMetroTransformer().transform({
        src: FLOW_COMPONENT,
        filename: "app/NativeCounter.js",
        options: { dev: true, projectRoot: root },
      });

      const source = result.metadata.uniflowedSource;
      expect(source).toContain("function NativeCounter(");
      expect(source).toContain("react/compiler-runtime");
      expect(source).not.toContain("component NativeCounter");
      expect(source).not.toContain("label: string");
    });
  });

  it("forwards the plugins Metro passes to the upstream transformer", async () => {
    await withMetroProject({ overrideUpstream: true }, async ({ root }) => {
      function functionMapBabelPlugin() {}
      function importLocationsPlugin() {}

      const result = await loadMetroTransformer().transform({
        src: FLOW_COMPONENT,
        filename: "app/NativeCounter.js",
        options: { dev: true, projectRoot: root },
        plugins: [functionMapBabelPlugin, importLocationsPlugin],
      });

      expect(result.metadata.plugins).toEqual(["functionMapBabelPlugin", "importLocationsPlugin"]);
    });
  });

  it("finds Expo's transformer beside the installed expo when the config names none", async () => {
    await withMetroProject({ overrideUpstream: false }, async ({ root }) => {
      // npm nests it exactly like this for the SDK 57 template, which is why a
      // lookup from uf's own package could not see it.
      writeProjectFile(root, "node_modules/expo/package.json", '{ "name": "expo" }\n');
      writeProjectFile(
        root,
        "node_modules/expo/node_modules/@expo/metro-config/package.json",
        '{ "name": "@expo/metro-config", "exports": { "./babel-transformer": "./build/babel-transformer.js" } }\n',
      );
      writeProjectFile(
        root,
        "node_modules/expo/node_modules/@expo/metro-config/build/babel-transformer.js",
        fakeTransformer("expo"),
      );
      writeProjectFile(
        root,
        "node_modules/@react-native/metro-babel-transformer/package.json",
        '{ "name": "@react-native/metro-babel-transformer", "main": "index.js" }\n',
      );
      writeProjectFile(
        root,
        "node_modules/@react-native/metro-babel-transformer/index.js",
        fakeTransformer("react-native"),
      );

      const result = await loadMetroTransformer().transform({
        src: FLOW_COMPONENT,
        filename: "app/NativeCounter.js",
        options: { dev: true, projectRoot: root },
      });

      expect(result.metadata.upstream).toBe("expo");
    });
  });

  it("refuses to guess when no React Native transformer is installed", async () => {
    await withMetroProject({ overrideUpstream: false }, async ({ root }) => {
      await expect(
        loadMetroTransformer().transform({
          src: FLOW_COMPONENT,
          filename: "app/NativeCounter.js",
          options: { dev: true, projectRoot: root },
        }),
      ).rejects.toThrow("there is no React Native Babel transformer to run after uf's");
    });
  });

  it("refuses to run uf's own transformer as its upstream", async () => {
    await withMetroProject({ overrideUpstream: false }, async ({ root }) => {
      await withEnv({ UF_METRO_UPSTREAM_TRANSFORMER: metroTransformerPath }, async () => {
        await expect(
          loadMetroTransformer().transform({
            src: FLOW_COMPONENT,
            filename: "app/NativeCounter.js",
            options: { dev: true, projectRoot: root },
          }),
        ).rejects.toThrow("is uf's own Metro transformer");
      });
    });
  });

  it("compiles shared StyleX into native objects before Metro runs", async () => {
    await withMetroProject({ overrideUpstream: true }, async ({ root }) => {
      const result = await loadMetroTransformer().transform({
        src: 'import { stylex } from "@uniflowed/stylex"; const styles = stylex.create({ root: { color: "red", padding: 12 } }); export const root = styles.root;',
        filename: "app/styles.js",
        options: { dev: false, projectRoot: root },
      });
      expect(result.metadata.uniflowedSource).toContain('"$$native":true');
      expect(result.metadata.uniflowedSource).toContain('"padding":12');
    });
  });

  it("names CSS-only declarations refused by the native StyleX compiler", async () => {
    await withMetroProject({ overrideUpstream: true }, async ({ root }) => {
      await expect(
        loadMetroTransformer().transform({
          src: 'import { stylex } from "@uniflowed/stylex"; export const styles = stylex.create({ root: { color: { default: "red", ":hover": "blue" } } });',
          filename: "app/styles.js",
          options: { dev: false, projectRoot: root },
        }),
      ).rejects.toThrow("selector :hover");
    });
  });

  it("builds a Metro cache key that changes when the uf binary does", async () => {
    await withMetroProject({ overrideUpstream: true }, async ({ root }) => {
      const binary = path.join(root, "bin/uf");
      writeProjectFile(root, "bin/uf", "#!/bin/sh\n");
      fs.chmodSync(binary, 0o755);

      await withEnv({ UF_BINARY: binary }, () => {
        const before = loadMetroTransformer().getCacheKey({ projectRoot: root });
        expect(before).toContain("uniflowed-react-native-metro-transformer-v2");
        expect(before).toContain("upstream-test-transformer");
        expect(before).toContain(binary);

        // A rebuilt `uf` is a different file at the same path.
        fs.writeFileSync(binary, "#!/bin/sh\n# rebuilt\n");
        const after = loadMetroTransformer().getCacheKey({ projectRoot: root });
        expect(after).not.toBe(before);
      });
    });
  });

  it("finds uf where the installer put it when it is not on PATH", async () => {
    await withMetroProject({ overrideUpstream: true }, async ({ root }) => {
      const installed = path.join(root, "installed-bin");
      writeProjectFile(root, "installed-bin/uf", "#!/bin/sh\n");
      fs.chmodSync(path.join(installed, "uf"), 0o755);
      fs.mkdirSync(path.join(root, "empty-path"));

      await withEnv(
        { UF_BINARY: undefined, PATH: path.join(root, "empty-path"), UF_BIN_DIR: installed },
        () => {
          const key = loadMetroTransformer().getCacheKey({ projectRoot: root });
          expect(key).toContain(path.join(installed, "uf"));
          expect(process.env.UF_BINARY).toBe(path.join(installed, "uf"));
        },
      );
    });
  });

  it("names every place it looked when it cannot find uf", async () => {
    await withMetroProject({ overrideUpstream: true }, async ({ root }) => {
      fs.mkdirSync(path.join(root, "empty-path"));
      fs.mkdirSync(path.join(root, "empty-bin"));

      await withEnv(
        {
          UF_BINARY: undefined,
          PATH: path.join(root, "empty-path"),
          UF_BIN_DIR: path.join(root, "empty-bin"),
        },
        () => {
          expect(() => loadMetroTransformer().getCacheKey({ projectRoot: root })).toThrow(
            `UF_BINARY is not set, \`uf\` is not on PATH (${path.join(root, "empty-path")}), ` +
              `and there is no executable at ${path.join(root, "empty-bin", "uf")}`,
          );
        },
      );
    });
  });
});

function loadMetroTransformer(): MetroTransformerModule {
  delete load.cache[metroTransformerPath];
  return load(metroTransformerPath);
}

/**
 * Run `args` on the host's own runtime with none of uf's hooks: no `--import`
 * (a spawned process does not inherit this one's), and no `NODE_OPTIONS`.
 */
function runPlainNode(args: $ReadOnlyArray<string>): {
  status: ?number,
  stdout: string,
  stderr: string,
} {
  const node = "node";
  const env = { ...process.env };
  delete env.NODE_OPTIONS;
  const run = spawnSync(node, args, { encoding: "utf8", env });
  return { status: run.status, stdout: String(run.stdout), stderr: String(run.stderr) };
}

async function withMetroProject(
  options: { overrideUpstream: boolean },
  body: (project: MetroProject) => Promise<void> | void,
): Promise<void> {
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "uf-metro-transform-")));
  const upstream = path.join(root, "upstream-transformer.cjs");
  try {
    fs.writeFileSync(path.join(root, "uf.config.js"), "export default {};\n");
    fs.writeFileSync(upstream, fakeTransformer("upstream-test-transformer"));
    await withEnv(
      {
        UF_METRO_UPSTREAM_TRANSFORMER: options.overrideUpstream ? upstream : undefined,
        UF_METRO_CONFIGURED_UPSTREAM: undefined,
      },
      () => body({ root, upstream }),
    );
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
}

/**
 * Set or remove environment variables for the length of `body`, and put back
 * exactly what was there afterwards — including a variable that was absent.
 */
async function withEnv(
  values: { readonly [name: string]: string | void },
  body: () => Promise<void> | void,
): Promise<void> {
  const previous: Map<string, string | void> = new Map();
  for (const name of Object.keys(values)) {
    previous.set(name, process.env[name]);
    const value = values[name];
    if (value == null) {
      delete process.env[name];
    } else {
      process.env[name] = value;
    }
  }
  try {
    await body();
  } finally {
    for (const [name, value] of previous) {
      if (value == null) {
        delete process.env[name];
      } else {
        process.env[name] = value;
      }
    }
  }
}

/** A Babel transformer that reports what it was handed instead of transforming. */
function fakeTransformer(name: string): string {
  return (
    "module.exports.transform = async ({ src, filename, options, plugins }) => ({\n" +
    "  ast: { type: 'Program', sourceType: 'module', body: [] },\n" +
    "  metadata: {\n" +
    `    upstream: ${JSON.stringify(name)},\n` +
    "    uniflowedSource: src,\n" +
    "    filename,\n" +
    "    dev: options.dev === true,\n" +
    "    hot: options.hot,\n" +
    "    inputSourceMap: options.inputSourceMap ?? null,\n" +
    "    plugins: (plugins ?? []).map((plugin) => plugin.name),\n" +
    "  },\n" +
    "});\n" +
    `module.exports.getCacheKey = () => ${JSON.stringify(name)};\n`
  );
}

function writeProjectFile(root: string, relative: string, contents: string): void {
  const file = path.join(root, relative);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, contents);
}
