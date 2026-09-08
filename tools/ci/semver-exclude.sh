#!/bin/sh
# Print the crates `cargo semver-checks` cannot compare, comma-separated.
#
# There are two reasons a crate cannot be compared, and only one of them is
# permanent.
#
# It names the `upstream/flow` submodule through a path dependency. The
# baseline is built from a copy of the crate outside the workspace, where a
# relative path no longer resolves — which is not something a crate can fix,
# since a path dependency is relative by definition. That is the closure of
# the crates reaching `uf_flow` or `uf_check`, whether or not the feature
# using it is enabled. See docs/architecture.md.
#
# The closure is **computed**, and it used to be a list somebody added to.
# That list went stale the moment `uf_i18n` was added: it reaches `uf_flow`,
# it was new so the second half below excluded it, and the release after that
# the baseline caught up and it rejoined a gate it can never pass. The job
# then failed on every pull request, at `uf_i18n` — which is alphabetically
# first among the crates the list was missing, so `uf_lib` and `uf_router`
# were latent behind it and would have been next.
#
# A hand-maintained list of what to check is the same shape as the bug
# ubugeeei-prod/uf#590 fixed for the CI gate and ubugeeei-prod/uf#425 for
# `uf explain`. This reads the manifests instead.
#
# Or it did not exist at the baseline revision. A new crate has nothing to be
# compared against, and cargo-semver-checks ends the whole run rather than
# skipping it — so every pull request that adds a crate fails this job for a
# reason that is not a semver break. That half is computed rather than
# written down, so a crate rejoins the gate by itself once the baseline has
# caught up, instead of sitting in a list nobody revisits.
set -eu

baseline=${1:?usage: semver-exclude.sh <baseline-rev>}

# The two crates that name the submodule directly. Everything else joins by
# depending on one of them, at any depth.
seed='uf_flow uf_check'

names=$(sed -n 's/^name = "\(.*\)"$/\1/p' crates/*/Cargo.toml)

submodule_path=$seed
while : ; do
  added=''
  for manifest in crates/*/Cargo.toml; do
    name=$(sed -n 's/^name = "\(.*\)"$/\1/p' "$manifest" | head -1)
    [ -n "$name" ] || continue
    # Already in?
    case " $submodule_path " in *" $name "*) continue ;; esac
    for excluded in $submodule_path; do
      # A path dependency on an excluded crate, in any dependency table.
      if grep -qE "^$excluded = \{[^}]*path = " "$manifest"; then
        added="$added $name"
        break
      fi
    done
  done
  [ -n "$added" ] || break
  submodule_path="$submodule_path$added"
done

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
