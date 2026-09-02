#!/bin/sh
# Materialize the `upstream/flow` submodule cheaply.
#
# `uf_flow`'s `upstream-parser` feature builds against Meta's official Flow Rust
# port, which is not published to crates.io. The submodule tracks the whole Flow
# repository (~190 MB), but only `rust_port/` is ever compiled, so this script
# fetches a single shallow commit without blobs and checks out just that
# subtree — roughly 40 MB instead of 190 MB.
#
# Every cargo invocation in this workspace needs the submodule present, because
# Cargo resolves path dependencies even when the feature that uses them is off.
set -eu

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"

submodule_path="upstream/flow"
sparse_subtree="rust_port"

git submodule update --init --depth 1 --filter=blob:none "$submodule_path"
git -C "$submodule_path" sparse-checkout set "$sparse_subtree"

if [ ! -f "$submodule_path/$sparse_subtree/Cargo.toml" ]; then
  echo "upstream sync failed: $submodule_path/$sparse_subtree/Cargo.toml is missing" >&2
  exit 1
fi

printf 'upstream/flow ready at %s\n' "$(git -C "$submodule_path" rev-parse --short HEAD)"
