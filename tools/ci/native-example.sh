#!/bin/sh
set -eu
repo=$(pwd)
binary=${UF_BINARY:-"$repo/target/release/uf"}
export UF_BINARY="$binary"
# Exercise the checkout instructions as well as the packed consumer below.
(cd examples/simple-sns-native && npm ci --ignore-scripts --no-audit --no-fund)
"$binary" --cwd examples/simple-sns-native fmt --check
"$binary" --cwd examples/simple-sns-native lint
"$binary" --cwd examples/simple-sns-native build --target ios
scratch=$(mktemp -d "${TMPDIR:-/tmp}/uf-native-example.XXXXXX")
trap 'rm -rf "$scratch"' EXIT HUP INT TERM
mkdir -p "$scratch/packs"
node tools/ci/pack-native-dependencies.cjs "$scratch/packs"
node --input-type=commonjs - "$scratch" <<'JS'
const fs = require('node:fs');
const path = require('node:path');
const root = process.argv[2];
fs.cpSync('examples/simple-sns-native', path.join(root, 'app'), {
  recursive: true,
  filter: source => !['node_modules', '.uf', '.expo', 'dist', 'router.js', 'router.native.js', 'router.ios.js', 'router.android.js'].includes(path.basename(source)),
});
const file = path.join(root, 'app/package.json');
const manifest = JSON.parse(fs.readFileSync(file));
Object.assign(manifest.dependencies, JSON.parse(fs.readFileSync(path.join(root, 'packs/dependencies.json'))));
fs.writeFileSync(file, JSON.stringify(manifest));
JS
(cd "$scratch/app" && npm install --ignore-scripts --no-audit --no-fund)
export UF_BINARY="$binary"
"$binary" --cwd "$scratch/app" test --threads 1
"$binary" --cwd "$scratch/app" build --target ios
"$binary" --cwd "$scratch/app" build --target android
test -s "$scratch/app/dist/native/ios/main.jsbundle"
test -s "$scratch/app/dist/native/android/main.jsbundle"
echo 'Commonplace native interactions and iOS/Android bundles passed'
