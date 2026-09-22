// @noflow
// Test an unpublished release with the packages in this checkout. Install
// with npm_config_install_links=true to copy packages as publication does.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const app = path.resolve(process.argv[2]);
const manifestPath = path.join(app, "package.json");
const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
const fields = ["dependencies", "devDependencies", "peerDependencies", "optionalDependencies"];
const packages = new Map();
for (const entry of fs.readdirSync(path.join(root, "packages"))) {
  const directory = path.join(root, "packages", entry);
  const file = path.join(directory, "package.json");
  if (fs.existsSync(file)) {
    const pkg = JSON.parse(fs.readFileSync(file, "utf8"));
    packages.set(pkg.name, { directory, pkg });
  }
}
const pending = fields.flatMap((field) => Object.keys(manifest[field] ?? {}));
const visited = new Set();
for (const name of pending) {
  if (!name.startsWith("@uniflowed/") || visited.has(name)) continue;
  visited.add(name);
  const entry = packages.get(name);
  if (entry == null) throw new Error(`No workspace package for ${name}`);
  const specifier = `file:${entry.directory.replaceAll("\\", "/")}`;
  let declared = false;
  for (const field of fields) {
    if (manifest[field]?.[name] != null) {
      manifest[field][name] = specifier;
      declared = true;
    }
  }
  if (!declared) (manifest.dependencies ??= {})[name] = specifier;
  for (const field of ["dependencies", "peerDependencies", "optionalDependencies"]) {
    pending.push(...Object.keys(entry.pkg[field] ?? {}));
  }
}
fs.writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
console.log(`Prepared ${visited.size} workspace packages into the fixture manifest`);
