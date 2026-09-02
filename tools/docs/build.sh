#!/usr/bin/env sh
set -eu

script_dir="$(CDPATH= cd "$(dirname "$0")" && pwd)"
repo_root="$(CDPATH= cd "$script_dir/../.." && pwd)"
out_dir="$repo_root/docs/dist/docs"
static_dir="$repo_root/docs/static"

rm -rf "$out_dir"
mkdir -p "$out_dir/brand" "$out_dir/install"

cp "$static_dir/index.html" "$out_dir/index.html"
cp "$static_dir/install/index.html" "$out_dir/install/index.html"
cp "$static_dir/docs.css" "$out_dir/docs.css"

for asset in \
  favicon.svg \
  index.js \
  tokens.css \
  tokens.json \
  uf.png \
  uniflowed-logo.png \
  uniflowed-logo.svg \
  uniflowed-mark.png \
  uniflowed-mark.svg \
  uniflowed-wordmark.png \
  uniflowed-wordmark.svg
do
  cp "$repo_root/brand/$asset" "$out_dir/brand/$asset"
done

(
  cd "$repo_root"
  cargo run --release --package uf_cli --bin uf -- --cwd docs build --size-report
)
