#!/bin/sh
# Materialize the upstream sources uf builds against.
#
# Without arguments: `upstream/flow`, which every cargo invocation needs.
# With `--integrations`: also the repositories in `tools/upstream/repos.txt`,
# which nothing in the cargo graph depends on yet.
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

# `rust_port` holds the crates, but several of them reach outside it with
# `include_str!`: `flow_flowlib` embeds Flow's own library definitions from
# `lib/`, `prelude/`, and `tslib/`. Leaving those out builds fine until the day
# someone enables the type checker, then fails with an unhelpful missing-file
# error from a macro. Check them out up front.
#
# `evals/flow-typed/environment` is where Flow keeps the DOM, BOM and Node
# globals. They are not in `lib/` — that holds only `core.js` and `react.js` —
# and without them a type checker knows what a `Map` is and not what a
# `document` is. `uf_check` embeds them from there.
sparse_subtrees="rust_port lib prelude tslib evals/flow-typed/environment"

git submodule update --init --depth 1 --filter=blob:none "$submodule_path"
# shellcheck disable=SC2086 # deliberate word splitting: one argument per subtree
git -C "$submodule_path" sparse-checkout set $sparse_subtrees

for required in \
  "rust_port/Cargo.toml" \
  "lib/core.js" \
  "lib/react.js" \
  "prelude/prelude.js" \
  "evals/flow-typed/environment/dom.js" \
  "evals/flow-typed/environment/bom.js" \
  "evals/flow-typed/environment/node.js"
do
  if [ ! -f "$submodule_path/$required" ]; then
    echo "upstream sync failed: $submodule_path/$required is missing" >&2
    exit 1
  fi
done

if [ ! -d "$submodule_path/tslib" ]; then
  echo "upstream sync failed: $submodule_path/tslib is missing" >&2
  exit 1
fi

printf 'upstream/flow ready at %s\n' "$(git -C "$submodule_path" rev-parse --short HEAD)"

# The rest of `upstream/` is pinned by commit in `tools/upstream/repos.txt`
# rather than tracked as submodules, for the reason that file gives: nothing in
# the cargo graph depends on them, and this script runs in every CI job.
#
# `--integrations` fetches them. Each is filtered to the subtrees uf actually
# reads — React's compiler crates, Relay's compiler crates, React Native's
# codegen and Libraries — because the three repositories together are about a
# gigabyte and uf reads perhaps eighty megabytes of it.
[ "${1:-}" = "--integrations" ] || exit 0

manifest=tools/upstream/repos.txt

while IFS='|' read -r name url commit subtrees; do
  name=$(echo "$name" | tr -d '[:space:]')
  case "$name" in '' | '#'*) continue ;; esac
  url=$(echo "$url" | tr -d '[:space:]')
  commit=$(echo "$commit" | tr -d '[:space:]')
  subtrees=$(echo "$subtrees" | sed 's/^ *//; s/ *$//')

  dest=upstream/$name
  git_dir=$repo_root/.git/upstream/$name

  if [ ! -e "$dest/.git" ]; then
    rm -rf "$git_dir"
    mkdir -p "$(dirname "$git_dir")"
    git init --quiet --separate-git-dir "$git_dir" "$dest"
    git -C "$dest" remote add origin "$url"
    git -C "$dest" config core.sparseCheckout true
  fi

  if [ "$(git -C "$dest" rev-parse HEAD 2>/dev/null || echo none)" != "$commit" ]; then
    printf 'upstream: fetching %s at %.12s\n' "$name" "$commit"
    git -C "$dest" fetch --quiet --depth 1 --filter=blob:none origin "$commit"
    # shellcheck disable=SC2086 # deliberate word splitting: one argument per subtree
    git -C "$dest" sparse-checkout set $subtrees
    git -C "$dest" checkout --quiet --detach "$commit"
  fi

  for subtree in $subtrees; do
    if [ ! -e "$dest/$subtree" ]; then
      echo "upstream sync failed: $dest/$subtree is missing" >&2
      exit 1
    fi
  done

  printf 'upstream/%s ready at %s\n' "$name" "$(git -C "$dest" rev-parse --short HEAD)"
done < "$manifest"
