// @noflow
import { createRequire } from "node:module";
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

/** CommonJS hidden behind uf's excluded Flow packages still needs prebundling. */
export function relayDependencies(root, server = false) {
  const require = createRequire(path.join(root, "package.json"));
  const entries = server
    ? ["relay-runtime", "relay-runtime/experimental", "react-relay/rsc_EXPERIMENTAL.js"]
    : ["relay-runtime", "react-relay", "react-relay/rsc-client_EXPERIMENTAL.js"];
  return entries.filter((entry) => {
    try {
      require.resolve(entry);
      return true;
    } catch (error) {
      if (error.code === "MODULE_NOT_FOUND" || error.code === "ERR_PACKAGE_PATH_NOT_EXPORTED")
        return false;
      throw error;
    }
  });
}

/** Resolve config from the application, independently of the process's cwd. */
export async function relayConfig(root) {
  for (const name of ["relay.config.json", "relay.config.js", "relay.config.mjs"]) {
    const filename = path.join(root, name);
    if (!fs.existsSync(filename)) continue;
    const config = name.endsWith(".json")
      ? JSON.parse(fs.readFileSync(filename, "utf8"))
      : (await import(pathToFileURL(filename).href)).default;
    return artifactOptions(root, config);
  }
  const filename = path.join(root, "package.json");
  const config = fs.existsSync(filename)
    ? JSON.parse(fs.readFileSync(filename, "utf8")).relay
    : null;
  return artifactOptions(root, config);
}

function artifactOptions(root, config) {
  if (config?.projects != null) {
    throw new Error("uf's Relay integration currently needs a single-project Relay config.");
  }
  return {
    artifactDirectory:
      config?.artifactDirectory == null ? null : path.resolve(root, config.artifactDirectory),
    eagerEsModules: true,
    jsModuleFormat: "commonjs",
    codegenCommand: "uf run relay",
  };
}

/** Flow has already lowered components/hooks; Relay owns the GraphQL transform. */
export async function transformRelay(code, filename, map, options) {
  if (
    !code.includes("graphql") ||
    !/["'](?:@uniflowed\/relay|react-relay|relay-runtime)["']/.test(code)
  )
    return null;
  const [{ transformAsync }, { default: relay }] = await Promise.all([
    import("@babel/core"),
    import("babel-plugin-relay"),
  ]);
  return transformAsync(code, {
    filename,
    babelrc: false,
    configFile: false,
    sourceMaps: true,
    inputSourceMap: map ?? undefined,
    plugins: [[relay, options]],
  });
}
