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
  og.png \
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
  # `UF_BIN` is set by anything that has already built the toolchain — CI
  # builds it once and shares it, and rebuilding here would cost minutes for
  # a binary that is already on disk.
  # `build#docs` rather than `--cwd docs`: the workspace selector is what a
  # person types, so it is what this runs, and a break in it breaks here first.
  if [ -n "${UF_BIN:-}" ]; then
    "$UF_BIN" build#docs --size-report
  else
    cargo run --release --package uf_cli --bin uf -- build#docs --size-report
  fi
  # The search index is read from what the build wrote — the ids the markdown
  # pipeline rendered, not the ones the sources imply — so it runs after the
  # build and writes beside it. `docs/app/_design/search.js` says what is in it.
  # The Flow loader compiles through `uf transform`, so it is told which uf:
  # the one that just built the site, not whichever is first on `PATH`.
  UF_BINARY="${UF_BIN:-$repo_root/target/release/uf}" UF_PROJECT_ROOT=. \
    node --import @uniflowed/host/register tools/docs/search-index.js docs/dist/docs
)
