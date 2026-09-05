#!/bin/sh
# Print the crates `cargo semver-checks` cannot compare, comma-separated.
#
# There are two reasons a crate cannot be compared, and only one of them is
# permanent.
#
# It names the `upstream/flow` submodule through a path dependency. The
# baseline is built from a copy of the crate outside the workspace, where a
# relative path no longer resolves — which is not something a crate can fix,
# since a path dependency is relative by definition. The list below is the
# closure of the crates reaching `uf_flow` or `uf_check`, whether or not the
# feature using it is enabled, and it grows as more crates print from or read
# the parser's syntax tree. See docs/architecture.md.
#
# Or it did not exist at the baseline revision. A new crate has nothing to be
# compared against, and cargo-semver-checks ends the whole run rather than
# skipping it — so every pull request that adds a crate fails this job for a
# reason that is not a semver break. That half is computed rather than
# written down, so a crate rejoins the gate by itself once the baseline has
# caught up, instead of sitting in a list nobody revisits.
set -eu

baseline=${1:?usage: semver-exclude.sh <baseline-rev>}

submodule_path='uf_check uf_cli uf_doc uf_flow uf_fmt uf_lint uf_transform'

new=''
for manifest in crates/*/Cargo.toml; do
  # The package name rather than the directory name: the two agree today and
  # an exclude list built on the wrong one fails silently, by excluding
  # nothing.
  name=$(sed -n 's/^name = "\(.*\)"$/\1/p' "$manifest" | head -1)
  [ -n "$name" ] || continue
  git cat-file -e "$baseline:$manifest" 2>/dev/null || new="$new $name"
done

printf '%s\n' "$submodule_path $new" | tr ' ' '\n' | sed '/^$/d' | sort -u | paste -sd, -
