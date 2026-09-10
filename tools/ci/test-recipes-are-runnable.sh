#!/bin/sh
# `recipes-are-runnable.sh` against recipes it can safely make wrong.
#
# The check reads four files nothing else in this repository reads, so what is
# worth testing is that it *notices*. Each case below is one of the five rules,
# planted in a scratch tree in the shape the real recipes are written in, and
# each is a mistake that was in this repository or is one edit away from being
# in it:
#
#   * `uf check` with no `uf install` before it — which is what the GitLab and
#     CircleCI recipes shipped, and which passes rather than failing.
#   * A renamed command and a removed flag, which is what a shipped recipe
#     turns into on its own when the CLI moves under it.
#   * An image with no JavaScript runtime on it, which is what both of those
#     recipes named.
#   * A toolchain cache key with no architecture in it, which is what the
#     GitLab one used.
#   * A command a job cannot read the result of, and no written reason.
#
# And one case for the guard itself: with no binary to ask, it fails rather
# than passing, because a check that quietly stops applying is what every gate
# in this directory exists to prevent.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
script="$repo_root/tools/ci/recipes-are-runnable.sh"

# The binary the check asks. Resolved to an absolute path here: every case runs
# from a scratch tree somewhere else, and a relative `target/release/uf` would
# resolve to nothing there and be indistinguishable from a broken rule.
uf="${UF_BIN:-}"
if [ -z "$uf" ]; then
  for candidate in "$repo_root/target/release/uf" "$repo_root/target/debug/uf"; do
    if [ -x "$candidate" ]; then
      uf="$candidate"
      break
    fi
  done
fi
case "$uf" in
  /*) ;;
  *) uf="$repo_root/$uf" ;;
esac
if [ ! -x "$uf" ]; then
  echo "test-recipes: FAIL: no uf binary at '$uf'." >&2
  echo "  Build one, or set UF_BIN to an absolute path." >&2
  exit 1
fi

work="$(mktemp -d "${TMPDIR:-/tmp}/uf-test-recipes.XXXXXX")"
trap 'rm -rf "$work"' EXIT INT TERM

fail() {
  echo "test-recipes: FAIL: $*" >&2
  exit 1
}

pass() {
  echo "  ok  $*"
}

# A tree with the check in it and one recipe of each shape beside it. The
# shapes matter more than the contents: the check is line-oriented and knows
# how these four files are written.
scratch() {
  root="$work/$1"
  rm -rf "$root"
  mkdir -p "$root/tools/ci" \
    "$root/integrations/github-actions" \
    "$root/integrations/gitlab" \
    "$root/integrations/circleci" \
    "$root/docs/app/guide/ci"
  cp "$script" "$root/tools/ci/recipes-are-runnable.sh"

  # The action installs the toolchain and runs no uf command of its own, and
  # its description is prose that names commands — which is exactly the text a
  # reader of these files must not mistake for a recipe.
  cat > "$root/integrations/github-actions/action.yml" <<'ACTION'
name: Set up uf
description: >-
  Install the uf toolchain so a workflow can run uf check and uf test.
  uf publishes macOS and Linux binaries only.
runs:
  using: composite
  steps:
    - name: Restore
      uses: actions/cache/restore@v4
      with:
        key: uf-${{ runner.os }}-${{ runner.arch }}-${{ inputs.version }}
ACTION

  cat > "$root/integrations/gitlab/uf.gitlab-ci.yml" <<'GITLAB'
variables:
  UF_VERSION: "latest"

.uf:
  image: node:24-bookworm-slim
  cache:
    - key: "uf-${UF_VERSION}-${CI_RUNNER_EXECUTABLE_ARCH}"
      paths:
        - .uf-toolchain/

uf:check:
  extends: .uf
  script:
    - uf install --frozen-lockfile
    - uf check
GITLAB

  cat > "$root/integrations/circleci/orb.yml" <<'ORB'
executors:
  default:
    docker:
      - image: cimg/node:22.11

jobs:
  run:
    parameters:
      command:
        type: string
        default: uf check
    steps:
      - restore_cache:
          keys:
            - uf-v1-{{ arch }}-<< parameters.version >>
      - run: uf install --frozen-lockfile
      - run:
          command: << parameters.command >>
ORB

  cat > "$root/docs/app/guide/ci/\$page.mdx" <<'GUIDE'
---
title: "uf in CI"
---

Run uf check in a pipeline.

```yaml
jobs:
  check:
    steps:
      - run: uf install
      - run: uf check
```
GUIDE

  echo "$root"
}

check() {
  ( cd "$1" && UF_BIN="$uf" sh tools/ci/recipes-are-runnable.sh ) >"$work/out" 2>&1
}

# 1. A tree in which every rule holds.
root="$(scratch clean)"
check "$root" || {
  cat "$work/out" >&2
  fail "rejected a tree in which every rule holds"
}
pass "passes when the recipes are runnable"

# 2. And the prose in the action's description was not read as a recipe. If it
#    had been, `uf publishes` would have been rule 1's problem above.
if grep -q 'publishes' "$work/out"; then
  fail "read an action description as a command"
fi
pass "prose that names commands is not read as one"

# 3. Rule 3: the mistake the GitLab and CircleCI recipes shipped.
root="$(scratch no-install)"
sed '/uf install/d' "$root/integrations/gitlab/uf.gitlab-ci.yml" > "$work/edited"
cp "$work/edited" "$root/integrations/gitlab/uf.gitlab-ci.yml"
if check "$root"; then
  fail "accepted a \`uf check\` with no \`uf install\` before it"
fi
grep -q 'no `uf install` before it' "$work/out" ||
  fail "failed for some other reason than the missing install"
pass "rejects a command that reads node_modules with nothing installed"

# 4. And it is the *order* it objects to, not the presence of the word: an
#    install after the check is not an install the check ran with.
root="$(scratch install-after)"
cat > "$root/integrations/gitlab/uf.gitlab-ci.yml" <<'GITLAB'
.uf:
  image: node:24-bookworm-slim

uf:check:
  extends: .uf
  script:
    - uf check
    - uf install --frozen-lockfile
GITLAB
if check "$root"; then
  fail "accepted an install that runs after the command it is for"
fi
pass "rejects an install that comes after the command"

# 5. And an install in a different job is a different job's install.
root="$(scratch install-elsewhere)"
cat > "$root/integrations/gitlab/uf.gitlab-ci.yml" <<'GITLAB'
.uf:
  image: node:24-bookworm-slim

uf:install:
  extends: .uf
  script:
    - uf install --frozen-lockfile

uf:check:
  extends: .uf
  script:
    - uf check
GITLAB
if check "$root"; then
  fail "accepted an install from another job as this one's"
fi
pass "rejects an install that belongs to another job"

# 6. Rule 1: a command the binary does not have. This is the staleness the
#    whole file is for — a recipe outliving the name it was written against.
root="$(scratch renamed)"
sed 's/- uf check$/- uf typecheck/' "$root/integrations/gitlab/uf.gitlab-ci.yml" > "$work/edited"
cp "$work/edited" "$root/integrations/gitlab/uf.gitlab-ci.yml"
if check "$root"; then
  fail "accepted a command this uf has no name for"
fi
grep -q 'no command for' "$work/out" || fail "failed for some other reason than the name"
pass "rejects a command the binary does not have"

# 7. Rule 1 again, for a flag. Same failure, one level down, and the more
#    likely of the two.
root="$(scratch unknown-flag)"
sed 's/- uf check$/- uf check --format json/' "$root/integrations/gitlab/uf.gitlab-ci.yml" > "$work/edited"
cp "$work/edited" "$root/integrations/gitlab/uf.gitlab-ci.yml"
if check "$root"; then
  fail "accepted a flag \`uf check\` does not take"
fi
grep -q 'does not accept it' "$work/out" || fail "failed for some other reason than the flag"
pass "rejects a flag the command does not accept"

# 8. Rule 4: an image that cannot run what the recipe runs.
root="$(scratch no-host)"
sed 's|image: node:24-bookworm-slim|image: debian:stable-slim|' \
  "$root/integrations/gitlab/uf.gitlab-ci.yml" > "$work/edited"
cp "$work/edited" "$root/integrations/gitlab/uf.gitlab-ci.yml"
if check "$root"; then
  fail "accepted an image with no JavaScript runtime on it"
fi
grep -q 'needing a JavaScript host' "$work/out" || fail "failed for some other reason than the image"
pass "rejects an image that cannot run what the recipe runs"

# 9. Rule 5: a toolchain cache key with no architecture in it.
root="$(scratch key-without-arch)"
sed 's/uf-${UF_VERSION}-${CI_RUNNER_EXECUTABLE_ARCH}/uf-${UF_VERSION}/' \
  "$root/integrations/gitlab/uf.gitlab-ci.yml" > "$work/edited"
cp "$work/edited" "$root/integrations/gitlab/uf.gitlab-ci.yml"
if check "$root"; then
  fail "accepted a toolchain cache key naming no architecture"
fi
grep -q 'naming no architecture' "$work/out" || fail "failed for some other reason than the key"
pass "rejects a toolchain cache key with no architecture"

# 10. Rule 2: a command whose result a job cannot read, with no written reason.
root="$(scratch unreadable)"
sed 's/- uf check$/- uf clean/' "$root/integrations/gitlab/uf.gitlab-ci.yml" > "$work/edited"
cp "$work/edited" "$root/integrations/gitlab/uf.gitlab-ci.yml"
if check "$root"; then
  fail "accepted a command with no --json and no written exemption"
fi
grep -q 'offers no --json' "$work/out" || fail "failed for some other reason than the missing --json"
pass "rejects a command a job would have to grep prose to read"

# 11. The guard on the guard. With no binary to ask, this check cannot apply
#     rules 1 and 2 at all, and the answer to that is to fail rather than to
#     report the three it can still apply as a pass.
root="$(scratch no-binary)"
if ( cd "$root" && UF_BIN=/nonexistent/uf sh tools/ci/recipes-are-runnable.sh ) >"$work/out" 2>&1; then
  fail "passed with no uf binary to ask"
fi
grep -q 'no uf binary to ask' "$work/out" || fail "failed for some other reason than the missing binary"
pass "fails rather than passing when there is no binary to ask"

echo "test-recipes-are-runnable: ok"
