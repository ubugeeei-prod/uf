#!/bin/sh
# Check out the React Compiler's own fixture corpus at the pinned commit.
#
# `crates/uf_transform/tests/react_compiler_conformance/` runs uf's compile
# path over every fixture here — the Flow parser, `babel.rs`, `scope.rs`, the
# official `react_compiler` crate and `print.rs` — and compares each result with
# the `.expect.md` snapshot `babel-plugin-react-compiler` wrote for it.
#
# Only the fixture directory is materialized: a blob-less fetch one commit
# deep, with a sparse checkout of that one subtree. The fixtures are about
# 3 MB; the repository they live in is hundreds.
#
# The checkout sits in a directory named for the commit, so a pin that moved
# can never be measured against the fixtures of the old one, and checkouts of
# any other commit are removed.
#
# Nothing here is edited by hand, and nothing is patched: a fixture that uf
# fails is a failure to record, not a file to change.
#
# Idempotent, so it is safe as a `dependsOn`.
set -eu

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"

pin=tools/react-compiler/pin.txt

pinned() {
  awk -v key="$1" '$1 == key { print $2 }' "$pin"
}

repository=$(pinned repository)
commit=$(pinned commit)
fixtures=$(pinned fixtures)
if [ -z "$repository" ] || [ -z "$commit" ] || [ -z "$fixtures" ]; then
  echo "react-compiler: $pin must name a repository, a commit and a fixtures path" >&2
  exit 1
fi

root=tests/fixtures/react-compiler
dest=$root/$commit

mkdir -p "$root"
for checkout in "$root"/*; do
  [ -d "$checkout" ] || continue # the unmatched glob
  [ "$checkout" = "$dest" ] && continue
  printf 'react-compiler: removing %s, which is not the pinned commit\n' "$checkout"
  rm -rf "$checkout"
done

# `git -C` on a directory without its own `.git` would find this repository
# instead and answer with uf's HEAD, so the check looks for the marker first.
current=none
if [ -e "$dest/.git" ]; then
  current=$(git -C "$dest" rev-parse HEAD 2>/dev/null || echo none)
fi

if [ "$current" != "$commit" ] || [ ! -d "$dest/$fixtures" ]; then
  rm -rf "$dest"
  mkdir -p "$dest"
  git -C "$dest" init --quiet
  git -C "$dest" remote add origin "$repository"
  # A non-cone pattern, written before the first checkout: `sparse-checkout
  # set` wants a HEAD to update, and this repository has none until the fetch.
  # One anchored directory pattern selects that subtree and nothing else.
  git -C "$dest" config core.sparseCheckout true
  git -C "$dest" config core.sparseCheckoutCone false
  mkdir -p "$dest/.git/info"
  printf '/%s/\n' "$fixtures" >"$dest/.git/info/sparse-checkout"
  printf 'react-compiler: fetching %s at %.12s\n' "$repository" "$commit"
  git -C "$dest" fetch --quiet --depth 1 --filter=blob:none origin "$commit"
  git -C "$dest" checkout --quiet --detach FETCH_HEAD
fi

if [ ! -d "$dest/$fixtures" ]; then
  echo "react-compiler: $dest/$fixtures is missing after the checkout" >&2
  exit 1
fi

count=$(find "$dest/$fixtures" -type f \( -name '*.js' -o -name '*.jsx' -o -name '*.ts' -o -name '*.tsx' \) | wc -l | tr -d ' ')
printf 'react-compiler: %s fixtures ready at %s\n' "$count" "$dest/$fixtures"
