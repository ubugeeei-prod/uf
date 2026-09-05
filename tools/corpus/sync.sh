#!/bin/sh
# Check out the upstream Flow corpus under `tests/fixtures/git`.
#
# React, Metro, Relay and React Native, shallow — about 240 MB, which is why
# this is not part of `uf run setup`. `crates/uf_fmt/tests/upstream_corpus.rs`
# runs the formatter's three guarantees over every Flow module in them and
# skips cleanly when they are not here.
#
# Idempotent, so it is safe as a `dependsOn`.
set -eu

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"

git submodule update --init --depth 1 -- tests/fixtures/git

for fixture in tests/fixtures/git/*/; do
  [ -d "$fixture" ] || continue
  printf 'corpus: %s ready\n' "${fixture%/}"
done
