#!/bin/sh
# `uf fmt --check` over examples/simple-sns-native, from its own directory.
#
# The root `ignore` list keeps the example out of `fmt:check`: it is its own
# project with its own `fmt` settings (`align: false` there is load-bearing for
# `native:smoke`, #1667). `native-example.sh` checks it, but that lane runs only
# in the release queue (#1683), so this runs at PR time instead.
#
# It needs none of the example's own dependencies — no `npm ci` there. uf
# formats the Flow itself; the non-Flow files go to Biome, which uf looks for in
# the example's `node_modules/.bin` and then on `PATH`. So the repository's own
# Biome (the example asks for the same `^2.5.12`) goes on `PATH`.
set -eu
repo=$(pwd)
binary=${UF_BINARY:-"$repo/target/release/uf"}
PATH="$repo/node_modules/.bin:$PATH"
export PATH
exec "$binary" --cwd examples/simple-sns-native fmt --check
