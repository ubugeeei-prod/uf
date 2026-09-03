#!/usr/bin/env sh
set -eu

script_dir="$(CDPATH= cd "$(dirname "$0")" && pwd)"
repo_root="$(CDPATH= cd "$script_dir/../.." && pwd)"
public_dir="$repo_root/docs/public/brand"

# The brand assets are shared with the README and the release pages, so they
# live at the repository root rather than inside the site. Staging them into
# Vite's public directory — rather than copying them over the build afterwards
# — means `uf dev` serves them too, and the build cannot wipe them: it empties
# its own output directory before it writes, which is why copying them there
# first never worked.
rm -rf "$public_dir"
mkdir -p "$public_dir"

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
  cp "$repo_root/brand/$asset" "$public_dir/$asset"
done

(
  cd "$repo_root"
  # The docs are a uf project: `@uniflowed/*` resolve to this repository's
  # packages through the npm workspace, and Vite runs on Node.js.
  if [ ! -d node_modules ]; then
    npm ci --no-audit --no-fund
  fi
  cargo run --release --package uf_cli --bin uf -- --cwd docs build --size-report
)
