// Loads a project's Metro config the way Metro loads it, and reports what
// `uf dev --target native` has to know before it starts a server: whether
// modules reach uf's transformer, which transformer runs after it, and the
// port Metro will listen on.
//
// Run as `node -e` by `native.rs`, from the project root, with the root in
// `UF_METRO_PROBE_ROOT`. The reply is one line on stdout behind a marker,
// because a Metro config is free to print whatever it likes while it loads.
//
// Everything is resolved from the project rather than from uf: the question
// is what *this project's* Metro will do, and the only honest way to answer it
// is to ask the project's own `metro-config` to load the project's own file.

"use strict";

const fs = require("node:fs");

const MARKER = "UF_METRO_PROBE ";
const root = process.env.UF_METRO_PROBE_ROOT || process.cwd();

function reply(value) {
  process.stdout.write(`\n${MARKER}${JSON.stringify(value)}\n`);
}

function resolveFromProject(request) {
  try {
    return require.resolve(request, { paths: [root] });
  } catch {
    return null;
  }
}

function realPath(file) {
  try {
    return fs.realpathSync(file);
  } catch {
    return file;
  }
}

function describe(error) {
  return String(error?.stack ?? error);
}

async function probe() {
  const loader = resolveFromProject("metro-config");
  if (loader == null) {
    reply({ status: "no-metro", message: `metro-config does not resolve from ${root}` });
    return;
  }
  const metro = require(loader);

  let configFile = null;
  try {
    const found = await metro.resolveConfig(undefined, root);
    configFile = found.isEmpty ? null : found.filepath;
  } catch {
    // Which file was found is a detail for the message; `loadConfig` below is
    // the call that says whether the config loads at all.
  }

  let config;
  try {
    config = await metro.loadConfig({ cwd: root }, {});
  } catch (error) {
    reply({ status: "load-failed", config_file: configFile, message: describe(error) });
    return;
  }

  const babelTransformerPath = config?.transformer?.babelTransformerPath ?? null;
  const uniflowed = resolveFromProject("@uniflowed/react-native/metro-transformer.cjs");
  reply({
    status: "ok",
    config_file: configFile,
    babel_transformer_path: babelTransformerPath,
    uniflowed_transformer: uniflowed,
    composed:
      uniflowed != null &&
      typeof babelTransformerPath === "string" &&
      realPath(babelTransformerPath) === realPath(uniflowed),
    // `withUniflowedMetro()` records the transformer it composed with while the
    // config loads; see `@uniflowed/react-native/metro`.
    upstream: process.env.UF_METRO_CONFIGURED_UPSTREAM ?? null,
    port: typeof config?.server?.port === "number" ? config.server.port : null,
  });
}

probe().catch((error) => {
  reply({ status: "load-failed", config_file: null, message: describe(error) });
});
