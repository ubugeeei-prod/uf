// @noflow
//
// `@uniflowed/react-native/metro`: the Metro half of uf's native target.
//
// # Why this module is CommonJS rather than Flow
//
// Every other module in `@uniflowed/*` is Flow, because `uf` is the process
// that loads it and `uf` installs the hooks that make Flow run. This one is
// loaded by somebody else's process. Metro evaluates `metro.config.js` in
// plain Node — under `expo start`, `react-native start`, the bundle phases
// Xcode and Gradle run, and on EAS Build's machines — and none of those has
// heard of uf's loader. Until ubugeeei-prod/uf#988 this file was Flow, and
// outside `uf test` importing it was `SyntaxError: Unexpected token 'export'`.
// The suite never saw that, because the suite is the one place the hooks are
// always installed.
//
// CommonJS rather than an ES module, because every template that writes a
// Metro config writes CommonJS — `require("expo/metro-config")`,
// `require("@react-native/metro-config")` — and an ES module config can still
// `import { withUniflowedMetro } from "@uniflowed/react-native/metro"`: Node
// reads a CommonJS module's named exports. The other direction, `require` of
// an ES module, needs Node 20.19 or 22.12.
//
// `uf build --target native` writes the same extension list and pipeline into
// the build manifest's target contract, so what a finished build reports and
// what a project's Metro config does are one answer rather than two.

"use strict";

const path = require("node:path");

const { configuredUpstreamEnv } = require("./metro-transformer.cjs");

/** Source extensions uf's native target needs, added to the project's own. */
const metroSourceExts = Object.freeze(["js", "jsx", "mjs", "cjs"]);

/** The `package.json` fields Metro reads for an entry point, in order. */
const metroResolverMainFields = Object.freeze(["react-native", "browser", "main"]);

/** What a module goes through, in order, on its way into a native bundle. */
const metroTransformPipeline = Object.freeze([
  "flow",
  "react-compiler",
  "stylex-native-objects",
  "metro-babel",
]);

/** The Babel transformer Metro runs, as the absolute path Metro wants. */
const metroTransformerPath = path.join(__dirname, "metro-transformer.cjs");

const nativePackages = Object.freeze([
  "brand",
  "cell",
  "core",
  "effect",
  "fetch",
  "form",
  "graphql",
  "hooks",
  "i18n",
  "immer",
  "query",
  "react",
  "react-native",
  "relay",
  "state",
  "stylex",
  "temporal",
  "validator",
]);
const domPackages = new Set([
  "ui",
  "web",
  "browser",
  "react-testing",
  "story",
  "vrt",
  "pwa",
  "motion",
]);

function nativeResolution(previous) {
  return (context, moduleName, platform) => {
    const name = moduleName.startsWith("@uniflowed/") ? moduleName.split("/")[1] : null;
    if (
      domPackages.has(name) ||
      (name === "router" &&
        moduleName !== "@uniflowed/router/native" &&
        moduleName !== "@uniflowed/router/native-navigation" &&
        moduleName !== "@uniflowed/router/http-client" &&
        moduleName !== "@uniflowed/router/routing")
    ) {
      throw new Error(
        `${moduleName} needs a browser DOM and cannot be bundled for ${platform}. ` +
          `Native packages: ${nativePackages.map((item) => `@uniflowed/${item}`).join(", ")}, ` +
          "@uniflowed/router/native, @uniflowed/router/native-navigation, " +
          "@uniflowed/router/http-client and @uniflowed/router/routing. Keep DOM components in $page.web.js.",
      );
    }
    return (previous ?? context.resolveRequest)(context, moduleName, platform);
  };
}

/**
 * Add uf's source extensions, resolver fields and transformer to a Metro config.
 *
 * Metro already owns platform suffix resolution (`.ios.js`, `.android.js`,
 * `.native.js`), and uf's route scanner uses the same suffixes, so nothing
 * about that is declared again here.
 *
 * # Composing with the transformer that is already there
 *
 * Metro accepts one `babelTransformerPath`, and every real config already has
 * one: `getDefaultConfig()` names Expo's in `expo/metro-config` and React
 * Native's in `@react-native/metro-config`. This function used to refuse such a
 * config, which left a hand-written object as the only input it accepted —
 * `withUniflowedMetro(getDefaultConfig(__dirname))` threw in every project that
 * had ever been generated.
 *
 * Replacing that transformer would be worse than refusing it. Expo's is how
 * `babel-preset-expo`, Expo Router and React Refresh reach a module, and a
 * custom one — an SVG transformer, say — is a decision the project made. So the
 * transformer that was there keeps running, after uf's and on uf's output, and
 * uf's takes its place in the config.
 *
 * Metro gives a Babel transformer nothing from the config it came from, so the
 * one being composed travels to Metro's workers in the environment; see
 * `configuredUpstreamEnv` in `metro-transformer.cjs` for why that is the only
 * channel there is. A transformer the config names by package is resolved here,
 * from the project root, where Metro would have resolved it — so a transformer
 * that is not installed is an error while the config loads, not in a worker
 * halfway through the first bundle.
 *
 * Composing twice is composing once: a config whose transformer is already
 * uf's is returned with nothing recorded, so wrapping a config a library has
 * already wrapped cannot make uf's transformer its own upstream.
 *
 * @param {object} [config] a Metro config — usually `getDefaultConfig(__dirname)`
 * @returns {object} the same config, with uf's extensions, fields and transformer
 */
function withUniflowedMetro(config = {}) {
  const resolver = config.resolver ?? {};
  const transformer = config.transformer ?? {};
  const existing = transformer.babelTransformerPath;
  if (existing != null && existing !== metroTransformerPath) {
    process.env[configuredUpstreamEnv] = resolveTransformer(existing, config.projectRoot);
  }
  return {
    ...config,
    resolver: {
      ...resolver,
      sourceExts: mergeUnique(resolver.sourceExts, metroSourceExts),
      resolverMainFields: mergeUnique(resolver.resolverMainFields, metroResolverMainFields),
      resolveRequest: nativeResolution(resolver.resolveRequest),
    },
    transformer: {
      ...transformer,
      babelTransformerPath: metroTransformerPath,
    },
  };
}

/**
 * The absolute path of the transformer a config names.
 *
 * An absolute path is taken as given, because that is what Metro does with it.
 * A package name or a relative path is resolved from the project root, which is
 * what `getDefaultConfig(projectRoot)` records on the config, and from the
 * working directory for a config that does not say.
 */
function resolveTransformer(specifier, projectRoot) {
  if (typeof specifier !== "string" || specifier === "") {
    throw new Error(
      `@uniflowed/react-native/metro: transformer.babelTransformerPath is ${JSON.stringify(specifier)}, ` +
        "and the path of a Babel transformer module was expected.",
    );
  }
  if (path.isAbsolute(specifier)) return specifier;
  const from = typeof projectRoot === "string" ? projectRoot : process.cwd();
  try {
    return require.resolve(specifier, { paths: [from] });
  } catch (error) {
    throw new Error(
      `@uniflowed/react-native/metro: transformer.babelTransformerPath names ${specifier}, ` +
        `which does not resolve from ${from}: ${String(error.message).split("\n")[0]}. ` +
        "Install it, or name it by its absolute path.",
    );
  }
}

function mergeUnique(existing, required) {
  const merged = [];
  const seen = new Set();
  for (const value of [...(existing ?? []), ...required]) {
    if (seen.has(value)) continue;
    seen.add(value);
    merged.push(value);
  }
  return merged;
}

module.exports = {
  metroResolverMainFields,
  metroSourceExts,
  metroTransformPipeline,
  metroTransformerPath,
  nativePackages,
  withUniflowedMetro,
};
