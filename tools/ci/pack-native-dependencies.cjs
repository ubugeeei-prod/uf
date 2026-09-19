// Install the tested source, including its dependency closure, before publication.
const fs = require("node:fs");
const path = require("node:path");
const { execFileSync } = require("node:child_process");

const destination = process.argv[2];
const packed = {};
function pack(name) {
  if (packed[name]) return;
  const directory = `packages/${name.slice("@uniflowed/".length)}`;
  const manifest = JSON.parse(fs.readFileSync(`${directory}/package.json`, "utf8"));
  const archive = execFileSync(
    "npm",
    [
      "pack",
      "--workspace",
      directory,
      "--ignore-scripts",
      "--silent",
      "--pack-destination",
      destination,
    ],
    { encoding: "utf8" },
  ).trim();
  packed[name] = `file:${path.join(destination, archive)}`;
  for (const dependency of Object.keys(manifest.dependencies ?? {})) {
    if (dependency.startsWith("@uniflowed/")) pack(dependency);
  }
}
for (const name of ["react-native", "vite", "router", "test", "stylex", "react-native-testing"])
  pack(`@uniflowed/${name}`);
fs.writeFileSync(path.join(destination, "dependencies.json"), JSON.stringify(packed));
