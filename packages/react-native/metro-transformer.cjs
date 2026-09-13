"use strict";

// `@uniflowed/react-native/metro-transformer`: Metro's Babel transformer hook.
//
// Metro loads `transformer.babelTransformerPath` with CommonJS `require`, even
// when the app config is ESM. The public config helper is ESM; this file is CJS
// so Metro can load it directly.

const fs = require("node:fs");
const path = require("node:path");

const DEFAULT_UPSTREAM_TRANSFORMERS = [
  "@react-native/metro-babel-transformer",
  "metro-react-native-babel-transformer",
  "metro-babel-transformer",
];

let hostPromise = null;
let upstreamTransformer = null;
let upstreamTransformerPath = null;

async function transform({ src, filename, options }) {
  const host = await loadHost();
  const projectRoot =
    typeof options?.projectRoot === "string" ? options.projectRoot : process.cwd();
  const absolute = path.isAbsolute(filename) ? filename : path.join(projectRoot, filename);
  let next = src;

  if (host.isFlowModule(absolute)) {
    const out = await host.transformFlow(src, absolute, {
      root: projectRoot,
      development: options?.dev === true,
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

  return loadUpstreamTransformer().transform({ src: next, filename, options });
}

function getCacheKey(options) {
  const parts = [
    "uniflowed-react-native-metro-transformer-v1",
    fileIdentity(__filename),
    fileIdentity(require.resolve("@uniflowed/host/transform")),
  ];
  const upstream = loadUpstreamTransformer();
  parts.push(upstreamTransformerPath ?? "");
  if (typeof upstream.getCacheKey === "function") {
    parts.push(String(upstream.getCacheKey(options)));
  }
  return parts.join("\0");
}

function loadHost() {
  hostPromise ??= import("@uniflowed/host/transform");
  return hostPromise;
}

function loadUpstreamTransformer() {
  if (upstreamTransformer != null) return upstreamTransformer;

  const candidates = [
    process.env.UF_METRO_UPSTREAM_TRANSFORMER,
    ...DEFAULT_UPSTREAM_TRANSFORMERS,
  ].filter(Boolean);
  const errors = [];
  for (const candidate of candidates) {
    try {
      const loaded = require(candidate);
      const transformer =
        loaded?.__esModule === true && loaded.default != null ? loaded.default : loaded;
      if (typeof transformer?.transform !== "function") {
        throw new Error("module does not export transform()");
      }
      upstreamTransformer = transformer;
      upstreamTransformerPath = require.resolve(candidate);
      return upstreamTransformer;
    } catch (error) {
      errors.push(`${candidate}: ${error.message}`);
    }
  }

  throw new Error(
    "@uniflowed/react-native/metro: could not load a React Native Metro Babel transformer. " +
      "Install @react-native/metro-babel-transformer or set UF_METRO_UPSTREAM_TRANSFORMER.\n" +
      errors.join("\n"),
  );
}

function fileIdentity(file) {
  const stat = fs.statSync(file);
  return `${file}:${stat.size}:${stat.mtimeMs}`;
}

module.exports = {
  transform,
  getCacheKey,
};
