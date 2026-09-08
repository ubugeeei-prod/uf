#!/bin/sh
# Print the crates `cargo semver-checks` cannot compare, comma-separated.
#
# There are two reasons a crate cannot be compared, and only one of them is
# permanent.
#
# It reaches the `upstream/flow` submodule through a path dependency. The
# baseline is built from a copy of the crate outside the workspace, where a
# relative path no longer resolves — which is not something a crate can fix,
# since a path dependency is relative by definition. That is every crate whose
# path dependencies reach `uf_flow` or `uf_check`, whether or not the feature
# using it is enabled. See docs/architecture.md.
#
# The closure is **computed**, not listed. It used to be a written list with a
# comment saying it "grows as more crates print from or read the parser's
# syntax tree" — which is a list somebody has to remember, and #633 is what
# happens when they do not: `uf_i18n` arrived depending on `uf_flow`, was in
# nobody's list, and this job then failed on `cargo update` for *every* pull
# request afterwards, including the one cutting a release. A gate that a new
# crate breaks for everybody is worse than the break it was guarding against.
#
# Or it did not exist at the baseline revision. A new crate has nothing to be
# compared against, and cargo-semver-checks ends the whole run rather than
# skipping it — so every pull request that adds a crate fails this job for a
# reason that is not a semver break. That half is computed rather than
# written down, so a crate rejoins the gate by itself once the baseline has
# caught up, instead of sitting in a list nobody revisits.
set -eu

baseline=${1:?usage: semver-exclude.sh <baseline-rev>}

# The roots: the two crates that name the submodule themselves.
roots='uf_flow uf_check'

# One line per path dependency, `<crate> <dependency>`, read out of the
# manifests. Every path dependency in this workspace is written on one line as
# `name = { path = "../name" … }`, and the name before the `=` is what cargo
# resolves, so that is what is read.
#
# `[dev-dependencies]` are skipped, and that is the whole reason this walks
# sections rather than grepping the file. `uf_lib` and `uf_router` reach
# `uf_flow` only through a dev-dependency, and the baseline builds fine with
# those — excluding them would take two crates out of the gate for nothing,
# which is the opposite of what this file is for.
edges=$(for manifest in crates/*/Cargo.toml; do
  awk '
    /^\[/ { section = $0; next }
    section == "[dev-dependencies]" { next }
    /^name = "/ && section == "[package]" {
      gsub(/^name = "|"$/, "", $0); crate = $0; next
    }
    /^[A-Za-z0-9_-]+ *= *\{ *path *= *"\.\./ {
      dep = $1
      if (crate != "") { print crate, dep }
    }
  ' "$manifest"
done)

# Transitive closure by fixed point: a crate is in when a crate it depends on
# is. Bounded by the number of crates, because each round either adds one or
# stops.
submodule_path="$roots"
while :; do
  grown=$(printf '%s\n' "$edges" | while read -r crate dep; do
    [ -n "$crate" ] || continue
    for member in $submodule_path; do
      if [ "$dep" = "$member" ]; then
        printf '%s\n' "$crate"
        break
      fi
    done
  done)
  next=$(printf '%s %s\n' "$submodule_path" "$grown" | tr ' ' '\n' | sed '/^$/d' | sort -u | tr '\n' ' ')
  [ "$next" = "$(printf '%s ' $submodule_path)" ] && break
  submodule_path="$next"
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
