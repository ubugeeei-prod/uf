#!/bin/sh
set -eu
repo=$(pwd)
binary=$(printenv UF_BINARY || printf '%s/target/release/uf' "$repo")
scratch=$(mktemp -d "/tmp/uf-migration-consumers.XXXXXX")
trap 'rm -rf "$scratch"' EXIT HUP INT TERM
mkdir -p "$scratch/packs"
node tools/ci/pack-native-dependencies.cjs "$scratch/packs"
for kind in flow-babel vite-react create-react-app; do
  app="$scratch/$kind"
  cp -R "crates/uf_cli/tests/fixtures/migrations/$kind" "$app"
  "$binary" --cwd "$app" migrate --dry-run --json > "$scratch/$kind.plan.json"
  test ! -f "$app/uf.config.js"
  "$binary" --cwd "$app" migrate > "$scratch/$kind.report.txt"
  node --input-type=commonjs - "$app" "$scratch/packs" <<'JS'
const fs = require("node:fs");
const [app, packs] = process.argv.slice(2);
const file = app + "/package.json";
const manifest = JSON.parse(fs.readFileSync(file));
const dependencies = JSON.parse(fs.readFileSync(packs + "/dependencies.json"));
for (const [name, value] of Object.entries(dependencies)) {
  if (manifest.devDependencies?.[name]) manifest.devDependencies[name] = value;
  else (manifest.dependencies ??= {})[name] = value;
}
fs.writeFileSync(file, JSON.stringify(manifest));
JS
  "$binary" --cwd "$app" install
  "$binary" --cwd "$app" check
  "$binary" --cwd "$app" lint
  "$binary" --cwd "$app" test
done
echo 'Flow/Babel/Jest/ESLint, Vite React and CRA consumers passed install/check/lint/test'
