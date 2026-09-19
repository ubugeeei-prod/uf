#!/bin/sh
set -eu

root=$(pwd)
uf=${UF_BINARY:-"$root/target/release/uf"}
mkdir -p "$root/.uf" "$root/output/browser-tests"
scratch=$(mktemp -d "$root/.uf/browser-tests.XXXXXX")
trap 'rm -rf "$scratch"' EXIT HUP INT TERM
cp -R crates/uf_cli/tests/fixtures/rsc-test-app "$scratch/rsc"
cp -R crates/uf_cli/tests/fixtures/browser-interactions "$scratch/browser"
rm -rf "$scratch/rsc/.uf" "$scratch/rsc/node_modules" "$scratch/browser/.uf" "$scratch/browser/__screenshots__"

"$uf" --cwd "$scratch/rsc" test
"$uf" --cwd "$scratch/browser" test --browser interaction.test.js
log="$root/output/browser-tests/visual.log"
if "$uf" --cwd "$scratch/browser" test --browser visual.test.js > "$log" 2>&1; then
  echo 'a missing visual baseline unexpectedly passed' >&2
  exit 1
fi
rg -q 'missing screenshot baseline' "$log"
"$uf" --cwd "$scratch/browser" test --browser visual.test.js -u
"$uf" --cwd "$scratch/browser" test --browser visual.test.js
sed 's/#245b8b/#c62c54/g' "$scratch/browser/visual.html" > "$scratch/browser/changed.html"
mv "$scratch/browser/changed.html" "$scratch/browser/visual.html"
if "$uf" --cwd "$scratch/browser" test --browser visual.test.js > "$log" 2>&1; then
  echo 'a changed visual baseline unexpectedly passed' >&2
  exit 1
fi
rg -q 'pixels differ' "$log"
test -s "$scratch/browser/__screenshots__/solid-shape.diff.png"
cp "$scratch/browser/__screenshots__/solid-shape.diff.png" "$root/output/browser-tests/solid-shape.diff.png"
"$uf" --cwd "$scratch/browser" test --browser visual.test.js -u
"$uf" --cwd "$scratch/browser" test --browser visual.test.js
echo 'RSC, trusted browser input, and visual pass/fail/update checks passed'
