#!/bin/sh
# `uf fmt --check` over every example, each from its own directory.
#
# Every `examples/*` is its own project, formatted by its own config. Most
# take uf's defaults (`align` on), so they show what uf produces out of the
# box; `simple-sns-native` keeps `align: false`, which `native:smoke` relies on
# (#1667). The root's `fmt.ignore` keeps them out of the root run, which would
# print them with the repository's `align: false` instead. They are still
# linted, checked and tested from the root. The native example's own
# `native-example.sh` lane runs only in the release queue (#1683), so this is
# what checks its formatting at PR time.
#
# It needs none of the examples' own dependencies — no `npm ci` in them. uf
# formats the Flow itself; the non-Flow files go to Biome, which uf looks for
# in the example's `node_modules/.bin` and then on `PATH`. So the repository's
# own Biome goes on `PATH` (the native example pins the same `^2.5.12`).
set -eu
repo=$(pwd)
binary=${UF_BINARY:-"$repo/target/release/uf"}
PATH="$repo/node_modules/.bin:$PATH"
export PATH
status=0
for config in examples/*/uf.config.js; do
  "$binary" --cwd "${config%/uf.config.js}" fmt --check || status=1
done
exit "$status"
