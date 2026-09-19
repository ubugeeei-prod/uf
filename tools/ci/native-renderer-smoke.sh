#!/bin/sh
set -eu
repo=$(pwd)
binary=${UF_BINARY:-"$repo/target/release/uf"}
scratch=$(mktemp -d "${TMPDIR:-/tmp}/uf-native-renderer.XXXXXX")
trap 'rm -rf "$scratch"' EXIT HUP INT TERM
mkdir -p "$scratch/packs" "$scratch/app"
node tools/ci/pack-native-dependencies.cjs "$scratch/packs"
cp -R crates/uf_cli/tests/fixtures/native-navigation/. "$scratch/app/"
node --input-type=commonjs - "$scratch" <<'JS'
const fs = require('node:fs');
const root = process.argv[2];
const file = `${root}/app/package.json`;
const manifest = JSON.parse(fs.readFileSync(file));
Object.assign(manifest.dependencies, JSON.parse(fs.readFileSync(`${root}/packs/dependencies.json`)));
fs.writeFileSync(file, JSON.stringify(manifest));
JS
(cd "$scratch/app" && npm install --ignore-scripts --no-audit --no-fund)
"$binary" --cwd "$scratch/app" test --threads 1
"$binary" --cwd "$scratch/app" test --threads 2
