#!/bin/sh
# Two installed consumers, outside the workspace, using their own Metro and React.
set -eu
repo=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
binary=${1:-"$repo/target/release/uf"}
binary=$(CDPATH='' cd -- "$(dirname -- "$binary")" && pwd)/$(basename -- "$binary")
scratch=$(mktemp -d "${TMPDIR:-/tmp}/uf-native-smoke.XXXXXX")
trap 'rm -rf "$scratch"' EXIT HUP INT TERM
export UF_BINARY="$binary"
mkdir -p "$scratch/packs"
cd "$repo"
node tools/ci/pack-native-dependencies.cjs "$scratch/packs"
for provider in expo react-native; do
  app="$scratch/uf-smoke-$provider"
  if [ "$provider" = expo ]; then
    npm exec --yes --package=create-expo-app@4.0.0 -- create-expo-app "$app" --template expo-template-blank@57.0.26 --no-install
  fi
  mkdir -p "$app/app" "$app/assets"
  node --input-type=commonjs - "$app" "$scratch/packs" "$provider" <<'NODE'
const fs = require('node:fs');
const path = require('node:path');
const [app, packs, provider] = process.argv.slice(2);
const template = fs.existsSync(path.join(app, 'package.json')) ? JSON.parse(fs.readFileSync(path.join(app, 'package.json'), 'utf8')) : {};
const dependencies = {
  ...template.dependencies,
  react: '19.2.3', 'react-dom': '19.2.3', 'react-native-web': '0.21.2',
  'react-native': provider === 'expo' ? '0.86.3' : '0.87.1',
};
if (provider === 'expo') Object.assign(dependencies, { expo: '57.0.24', '@expo/vector-icons': '15.0.3' });
else Object.assign(dependencies, {
  '@react-native-community/cli': '20.2.0',
  '@react-native/metro-config': '0.87.1', '@react-native/babel-preset': '0.87.1',
});
Object.assign(dependencies, JSON.parse(fs.readFileSync(path.join(packs, 'dependencies.json'), 'utf8')));
fs.writeFileSync(path.join(app, 'package.json'), JSON.stringify({ ...template, name: `uf-smoke-${provider}`, private: true, main: 'index.js', dependencies }));
const from = provider === 'expo' ? 'expo/metro-config' : '@react-native/metro-config';
fs.writeFileSync(path.join(app, 'metro.config.cjs'), `// @noflow\nconst {getDefaultConfig} = require('${from}');\nconst {withUniflowedMetro} = require('@uniflowed/react-native/metro');\nmodule.exports = withUniflowedMetro({...getDefaultConfig(__dirname), maxWorkers: 2});\n`);
fs.writeFileSync(path.join(app, 'babel.config.cjs'), `// @noflow\nmodule.exports = { presets: ['${provider === 'expo' ? 'babel-preset-expo' : 'module:@react-native/babel-preset'}'] };\n`);
fs.writeFileSync(path.join(app, 'app.json'), JSON.stringify(provider === 'expo' ? { expo: { name: 'UfNativeSmoke', slug: 'uf-native-smoke', android: { package: 'dev.uniflowed.smoke' } } } : { name: 'UfNativeSmoke' }));
fs.writeFileSync(path.join(app, 'uf.config.js'), `export default { app: { targets: ['web', 'react-native'], rsc: false }, build: { outDir: 'dist' } };\n`);
if (provider === 'expo') {
  fs.writeFileSync(path.join(app, 'adoption.test.js'), `import { it, expect } from '@uniflowed/test';\nit('runs Flow in the adopted Expo app', () => { const answer: number = 42; expect(answer).toBe(42); });\n`);
  fs.writeFileSync(path.join(app, 'template-scripts.json'), JSON.stringify(template.scripts));
}
NODE
  cd "$app"
  npm install --ignore-scripts --no-audit --no-fund
  if [ "$provider" = expo ]; then
    "$binary" install --frozen-lockfile
    "$binary" check
    "$binary" lint
    "$binary" test
    node --input-type=commonjs -e 'const fs=require("node:fs"),assert=require("node:assert/strict"); assert.deepEqual(JSON.parse(fs.readFileSync("package.json")).scripts,JSON.parse(fs.readFileSync("template-scripts.json")));'
  fi
  node --input-type=commonjs - "$app" <<'NODE'
const fs = require('node:fs');
const path = require('node:path');
const app = process.argv[2];
fs.writeFileSync(path.join(app, 'index.js'), `import { AppRegistry } from '@uniflowed/react-native';\nimport Page from './app/$page.js';\nAppRegistry.registerComponent('UfNativeSmoke', () => Page);\n`);
fs.writeFileSync(path.join(app, 'app/$page.js'), `import { Text, View, Image } from '@uniflowed/react-native';\nimport font from '../assets/font.ttf';\nimport icon from '../assets/icon.png';\nexport default component Page() { return <View><Text nativeID={String(font)}>native-shared-page</Text><Image source={typeof icon === 'number' ? icon : { uri: icon.src }} /></View>; }\n`);
fs.writeFileSync(path.join(app, 'app.js'), `import { routerView } from '@uniflowed/router'; export default routerView('./app');\n`);
fs.writeFileSync(path.join(app, 'app/$layout.web.js'), `export default component Layout(children: React.Node) { return <html><body>{children}</body></html>; }\n`);
const png = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+ip1sAAAAASUVORK5CYII=', 'base64');
for (const suffix of ['', '@2x', '@3x']) fs.writeFileSync(path.join(app, `assets/icon${suffix}.png`), png);
NODE
  if [ "$provider" = expo ]; then
    cp node_modules/@expo/vector-icons/build/vendor/react-native-vector-icons/Fonts/FontAwesome.ttf assets/font.ttf
  else
    cp "$scratch/uf-smoke-expo/assets/font.ttf" assets/font.ttf
  fi
  "$binary" build --target ios
  "$binary" build --target android
  node --input-type=commonjs <<'NODE'
const fs = require('node:fs');
const assert = require('node:assert/strict');
const manifest = JSON.parse(fs.readFileSync('.uf/build/meta/uf-build-manifest.json', 'utf8'));
assert.equal(manifest.target, 'android');
assert(manifest.outputs.some(file => /font.*\.ttf$/.test(file.path)));
assert(manifest.outputs.some(file => file.path.includes('drawable-xxhdpi')));
assert(fs.existsSync('dist/native/ios/main.jsbundle'));
assert(fs.readFileSync('dist/native/android/main.jsbundle', 'utf8').includes('native-shared-page'));
NODE
  if [ "$provider" = expo ]; then
    CI=1 node_modules/.bin/expo prebuild --platform android --no-install
    node --input-type=commonjs -e 'const fs=require("node:fs"),assert=require("node:assert/strict"); assert(fs.readFileSync("android/app/build.gradle","utf8").includes("export:embed"));'
    # Execute the generated native project's real JS bundle phase, including
    # Hermes compilation and asset copying, with uf inherited by Gradle.
    (cd android && ./gradlew :app:createBundleReleaseJsAndAssets --no-daemon --console=plain)
    test -s android/app/build/generated/assets/react/release/index.android.bundle
    node_modules/.bin/expo export --platform ios --output-dir expo-export
    node_modules/.bin/expo export:embed --platform ios --entry-file index.js --dev true --minify false --bundle-output dev.bundle --assets-dest dev-assets
  else
    node_modules/.bin/react-native bundle --platform ios --entry-file index.js --dev true --minify false --bundle-output dev.bundle --assets-dest dev-assets
  fi
  node --input-type=commonjs -e 'const fs=require("node:fs"),assert=require("node:assert/strict"); const bundle=fs.readFileSync("dev.bundle","utf8"); assert(bundle.includes("native-shared-page")); assert(/\$RefreshReg\$\([^,\n]+,\s*"Page"\)/.test(bundle), "the Flow component has no Refresh registration");'
  # The same page goes through the app's react-native-web dependency in Vite.
  "$binary" build --target web
  node --input-type=commonjs -e 'const fs=require("node:fs"),assert=require("node:assert/strict"); assert(fs.readFileSync("dist/index.html","utf8").includes("native-shared-page"));'
  node "$repo/tools/ci/native-dev-smoke.cjs" "$binary" "$app" "$provider"
done
