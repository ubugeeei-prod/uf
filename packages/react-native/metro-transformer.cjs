// @noflow
//
// `@uniflowed/react-native/metro-transformer.cjs`: the Babel transformer Metro
// runs for every module in a native bundle.
//
// Metro loads `transformer.babelTransformerPath` with `require`, in its own
// workers, whatever the app's config is written in — so this is CommonJS, and
// plain JavaScript for the reason `metro.cjs` is: no process that loads it has
// uf's hooks.
//
// For a module uf owns (`isFlowModule`), it runs `uf transform` — Flow's own
// lowering, the React Compiler, the StyleX policy — and hands the result to the
// transformer the project's Metro config already had, so Expo's preset or React
// Native's still does everything it did before: the module system, React
// Refresh, platform inlining. Anything else goes to that transformer untouched.

"use strict";

const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

/**
 * An upstream transformer a person names on purpose. It wins over everything.
 */
const UPSTREAM_OVERRIDE_ENV = "UF_METRO_UPSTREAM_TRANSFORMER";

/**
 * The upstream transformer `withUniflowedMetro()` found in the config it
 * composed.
 *
 * # Why the environment
 *
 * Metro calls a Babel transformer with `{ src, filename, options, plugins }`
 * and nothing from the config it was loaded from — `getBabelTransformArgs` in
 * `metro-transform-worker` builds `options` from the bundle request and five
 * named config values — so there is no key on `transformer` this module could
 * read the upstream from. The environment is the one channel that reaches it:
 * the config is evaluated before Metro starts its workers, and a worker, child
 * process or thread, starts with a copy of the environment it was created from.
 *
 * A name of its own, rather than reusing `UF_METRO_UPSTREAM_TRANSFORMER`,
 * because the two say different things. That one is a person overriding the
 * upstream, and it wins; this one is what the project's config already said,
 * and it is written again every time a config is composed.
 */
const CONFIGURED_UPSTREAM_ENV = "UF_METRO_CONFIGURED_UPSTREAM";

let host = null;
let upstream = null;

async function transform(args) {
  const { src, filename, options } = args;
  const projectRoot = projectRootOf(options);
  const flow = loadHost();
  const absolute = path.isAbsolute(filename) ? filename : path.join(projectRoot, filename);
  let next = src;

  if (flow.isFlowModule(absolute)) {
    findUfBinary(flow);
    const out = await flow.transformFlow(src, absolute, {
      root: projectRoot,
      development: options?.dev === true,
      // Off, because the upstream transformer registers components for React
      // Refresh itself — Expo's preset and React Native's both add
      // `react-refresh/babel` to a development bundle — and a component
      // registered twice is two families to the runtime, and a refresh that
      // remounts instead of preserving state.
      refresh: false,
      sourceMap: true,
    });
    if (out != null) {
      if (out.css != null && out.css !== "") {
        throw new Error(
          `@uniflowed/react-native/metro: ${filename} produced StyleX CSS, but the ` +
            "native Metro contract cannot attach CSS to a React Native bundle yet. " +
            "Use native style props or keep that StyleX module out of the native route.",
        );
      }
      next = out.code;
    }
  }

  // Everything Metro passed, and not a chosen subset of it. `plugins` carries
  // Metro's `functionMapBabelPlugin` and `importLocationsPlugin`, and
  // forwarding `{ src, filename, options }` alone dropped both for every Flow
  // module in the bundle — function names in stack traces among them.
  return loadUpstream(projectRoot).transformer.transform({ ...args, src: next });
}

/**
 * The cache key Metro files every transformed module under.
 *
 * Four identities, because four things decide what a module compiles to: this
 * file, the JavaScript side of `uf transform`, the `uf` binary that does the
 * compiling, and the upstream transformer with its own key. The binary is the
 * one that was missing. A rebuilt or upgraded `uf` changes the output of every
 * Flow module without touching any file this key used to hash, so Metro went on
 * serving the old transforms until somebody thought to pass `--reset-cache` —
 * the defect `@uniflowed/host`'s own cache keys on `ufBinaryIdentity()` to
 * avoid.
 */
function getCacheKey(options) {
  const projectRoot = projectRootOf(options);
  const flow = loadHost();
  const binary = findUfBinary(flow);
  const loaded = loadUpstream(projectRoot);
  const parts = [
    "uniflowed-react-native-metro-transformer-v2",
    fileIdentity(__filename),
    fileIdentity(require.resolve("@uniflowed/host/transform")),
    flow.ufBinaryIdentity(binary) ?? binary,
    loaded.path,
  ];
  if (typeof loaded.transformer.getCacheKey === "function") {
    parts.push(String(loaded.transformer.getCacheKey(options)));
  }
  return parts.join("\0");
}

/**
 * `@uniflowed/host/transform`, loaded synchronously.
 *
 * It is an ES module and this file is not, and `getCacheKey` has to answer
 * synchronously: Metro asks it once, on its main thread, before the first
 * transform. `require` of an ES module is what Node 20.19 and 22.12 added, and
 * on an older Node the error says that rather than failing on a name.
 */
function loadHost() {
  if (host != null) return host;
  try {
    host = require("@uniflowed/host/transform");
  } catch (error) {
    if (error?.code === "ERR_REQUIRE_ESM") {
      throw new Error(
        `@uniflowed/react-native/metro: Node ${process.version} cannot require @uniflowed/host, ` +
          "which is an ES module. Run Metro on Node 20.19, 22.12 or later.",
      );
    }
    throw error;
  }
  return host;
}

/**
 * The `uf` that compiles this bundle, found the way its installer arranged and
 * handed to `@uniflowed/host` through `UF_BINARY`.
 *
 * In order: `UF_BINARY`, which `uf dev` sets to itself so a bundle compiles
 * with exactly the `uf` that started it; `uf` on `PATH`; and the installer's
 * own directory, `UF_BIN_DIR` or `~/.local/bin`. The last is there for the
 * processes that bundle an app without a person's shell in front of them —
 * Xcode's "Bundle React Native code and images" phase runs with a minimal
 * `PATH`, and a `uf` the installer put in `~/.local/bin` is invisible to it.
 *
 * Found once and written back to `UF_BINARY`, so that the binary this key
 * identifies and the binary `@uniflowed/host` spawns cannot be two different
 * files.
 */
function findUfBinary(flow) {
  const named = process.env.UF_BINARY;
  if (named != null && named !== "") return named;
  if (flow.ufBinaryIdentity("uf") != null) return "uf";
  const directory = process.env.UF_BIN_DIR || path.join(os.homedir(), ".local", "bin");
  const installed = path.join(directory, "uf");
  if (flow.ufBinaryIdentity(installed) != null) {
    process.env.UF_BINARY = installed;
    return installed;
  }
  throw new Error(
    "@uniflowed/react-native/metro: cannot find the `uf` binary that compiles Flow for this bundle. " +
      `UF_BINARY is not set, \`uf\` is not on PATH (${process.env.PATH ?? ""}), ` +
      `and there is no executable at ${installed}. ` +
      "Install uf with `curl -fsSL https://setup.uniflowed.dev | sh`, or set UF_BINARY to its path — " +
      "for Xcode's bundle phase, export it from ios/.xcode.env.local.",
  );
}

/**
 * The Babel transformer that runs after uf's, loaded once per worker.
 *
 * An upstream named by a person or by the composed config is the only
 * candidate: if it does not load, that is an error, not an invitation to
 * guess. Only a config that never went through `withUniflowedMetro()` falls
 * back to looking for the transformer the project's own Metro config would
 * have named.
 */
function loadUpstream(projectRoot) {
  if (upstream != null) return upstream;
  const override = process.env[UPSTREAM_OVERRIDE_ENV];
  const configured = process.env[CONFIGURED_UPSTREAM_ENV];
  let candidates;
  if (override != null && override !== "") {
    candidates = [{ request: override, from: projectRoot, why: UPSTREAM_OVERRIDE_ENV }];
  } else if (configured != null && configured !== "") {
    candidates = [
      {
        request: configured,
        from: projectRoot,
        why: "transformer.babelTransformerPath, composed by withUniflowedMetro()",
      },
    ];
  } else {
    candidates = installedCandidates(projectRoot);
  }

  const tried = [];
  for (const candidate of candidates) {
    let resolved;
    try {
      resolved = require.resolve(candidate.request, { paths: [candidate.from] });
    } catch {
      tried.push(`${candidate.request} (${candidate.why}): not found from ${candidate.from}`);
      continue;
    }
    const loaded = require(resolved);
    const transformer =
      loaded?.__esModule === true && loaded.default != null ? loaded.default : loaded;
    if (typeof transformer?.transform !== "function") {
      tried.push(`${resolved} (${candidate.why}): does not export transform()`);
      continue;
    }
    if (transformer.isUniflowedMetroTransformer === true) {
      throw new Error(
        `@uniflowed/react-native/metro: ${resolved} is uf's own Metro transformer, named as the ` +
          "transformer to run after uf's. Two copies of @uniflowed/react-native have composed the " +
          "same config; install one.",
      );
    }
    upstream = { transformer, path: resolved };
    return upstream;
  }

  throw new Error(
    "@uniflowed/react-native/metro: there is no React Native Babel transformer to run after uf's. " +
      "Compose the project's Metro config so the transformer it already has keeps running — " +
      "`module.exports = withUniflowedMetro(getDefaultConfig(__dirname))`, with getDefaultConfig " +
      "from expo/metro-config or @react-native/metro-config — or set UF_METRO_UPSTREAM_TRANSFORMER " +
      "to a transformer's path.\n" +
      tried.map((line) => `  ${line}`).join("\n"),
  );
}

/**
 * The transformers a project's own Metro config would have named, most likely
 * first.
 *
 * Looked up from the project, not from this package. A package manager is free
 * to nest `@expo/metro-config` inside `expo` — npm does, for the SDK 57
 * template — and a `require` from here sees only what was hoisted, which is why
 * this list could not find Expo's transformer in a freshly created Expo app.
 *
 * Expo's first when the project has Expo, because in an Expo project it is the
 * one `getDefaultConfig()` names and React Native's would bundle without
 * `babel-preset-expo`. `metro-babel-transformer`, which this list used to end
 * with, is gone from it: it applies no React Native preset, so reaching it meant
 * React Native's own Flow source arriving at Babel with nothing to strip it — a
 * failure much further from its cause than this refusal.
 */
function installedCandidates(projectRoot) {
  const candidates = [];
  const expo = packageDirectory("expo", projectRoot);
  if (expo != null) {
    candidates.push({
      request: "@expo/metro-config/babel-transformer",
      from: expo,
      why: "Expo's transformer, beside the installed expo",
    });
  }
  candidates.push({
    request: "@react-native/metro-babel-transformer",
    from: projectRoot,
    why: "React Native's transformer",
  });
  const reactNative = packageDirectory("react-native", projectRoot);
  if (reactNative != null) {
    candidates.push({
      request: "@react-native/metro-babel-transformer",
      from: reactNative,
      why: "React Native's transformer, beside the installed react-native",
    });
  }
  candidates.push({
    request: "metro-react-native-babel-transformer",
    from: projectRoot,
    why: "React Native's transformer before 0.73",
  });
  return candidates;
}

/**
 * The directory `name` is installed in, looked up the way Node looks up a
 * package — `node_modules` in the starting directory and then in each parent.
 *
 * Walked rather than resolved, because `require.resolve("expo/package.json")`
 * depends on the package's `exports` allowing it, and the question here is only
 * whether the package is installed.
 */
function packageDirectory(name, from) {
  let directory = from;
  for (;;) {
    const candidate = path.join(directory, "node_modules", name);
    if (fs.existsSync(path.join(candidate, "package.json"))) return candidate;
    const parent = path.dirname(directory);
    if (parent === directory) return null;
    directory = parent;
  }
}

function projectRootOf(options) {
  return typeof options?.projectRoot === "string" ? options.projectRoot : process.cwd();
}

function fileIdentity(file) {
  const stat = fs.statSync(file);
  return `${file}:${stat.size}:${stat.mtimeMs}`;
}

module.exports = {
  transform,
  getCacheKey,
  // Read by `loadUpstream`, so a config composed by two copies of this package
  // is refused instead of recursing into itself.
  isUniflowedMetroTransformer: true,
  // The name `metro.cjs` writes the composed upstream under, kept in one place.
  configuredUpstreamEnv: CONFIGURED_UPSTREAM_ENV,
};
