#!/bin/sh
# Check out the Flow corpus under `tests/fixtures/git`.
#
# `crates/uf_fmt/tests/upstream_corpus.rs` runs the formatter's three
# guarantees over every Flow module here and skips cleanly when nothing is
# checked out. This is ~1 GB of other people's code, which is why it is not
# part of `uf run setup`.
#
# `tools/corpus/repos.txt` is the whole list: one line per repository, fetched
# one commit deep. React, Metro, Relay and React Native were submodules until
# ubugeeei-prod/uf#137 and are ordinary entries now — a checkout that still
# has them as submodules keeps working, because a submodule's `.git` file and
# the pin it is already at are exactly what the loop below looks for.
#
# Idempotent, so it is safe as a `dependsOn`.
set -eu

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"

manifest=tools/corpus/repos.txt

# A git directory outside the checkout, the way submodules keep theirs in
# `.git/modules`. A nested `.git` inside a fixture is one more thing every
# tool that walks these trees has to know to skip.
while read -r name url commit; do
  case "$name" in '' | '#'*) continue ;; esac

  dest=tests/fixtures/git/$name
  git_dir=$repo_root/.git/corpus/$name

  if [ ! -e "$dest/.git" ]; then
    rm -rf "$git_dir"
    # `git init --separate-git-dir` does not create the parent of the git
    # directory it is handed, and `.git/corpus` does not exist until the
    # first entry has been synced — so on a checkout that has never run this,
    # the first entry died on `Invalid path`. `tools/upstream/sync.sh` has
    # always had this line; this script never did, and every machine that had
    # already synced once was past the only run that would have shown it.
    mkdir -p "$(dirname "$git_dir")"
    git init --quiet --separate-git-dir "$git_dir" "$dest"
    git -C "$dest" remote add origin "$url"
  fi

  # Re-fetch only when the pin moved. `rev-parse` on an empty repository
  # fails, which is the first-run case and wants the fetch too.
  if [ "$(git -C "$dest" rev-parse HEAD 2>/dev/null || echo none)" != "$commit" ]; then
    printf 'corpus: fetching %s at %.12s\n' "$name" "$commit"
    git -C "$dest" fetch --quiet --depth 1 origin "$commit"
    git -C "$dest" checkout --quiet --detach FETCH_HEAD
  fi
done < "$manifest"

for fixture in tests/fixtures/git/*/; do
  [ -d "$fixture" ] || continue
  printf 'corpus: %s ready\n' "${fixture%/}"
done
