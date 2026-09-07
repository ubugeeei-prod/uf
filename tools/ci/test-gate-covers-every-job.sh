#!/bin/sh
# `gate-covers-every-job.sh` against pipelines it can safely make wrong.
#
# The check exists because the `CI` gate's `needs:` is a list somebody has to
# add to, and this repository has already been bitten twice by a list like
# that: `Flow lint` became a job in #404 and was in nobody's required set, and
# `uf run ci` had drifted eight tasks behind the pipeline before anybody
# counted. A guard against that failure is only worth the line it occupies if
# it *notices*, and a guard that reports success over a file it could not parse
# is the original bug wearing a green tick.
#
# So each way the gate can rot is planted here in a scratch pipeline, and the
# check has to fail on it — and fail *for that reason*, which is why every case
# looks at the message and not only at the exit status. Two of the cases go the
# other way and plant a shape that is *correct*, because a check that fails on
# everything is no more use than one that fails on nothing.
#
# The pipeline is a small, real-shaped `ci.yml`: eight jobs, six of them named
# for contexts branch protection requires, one deliberately outside the gate,
# and a `security.yml` beside it for `Zizmor`, which is required and does not
# live in `ci.yml` at all.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
script="$repo_root/tools/ci/gate-covers-every-job.sh"

work="$(mktemp -d "${TMPDIR:-/tmp}/uf-test-gate-covers.XXXXXX")"
trap 'rm -rf "$work"' EXIT INT TERM

fail() {
  echo "test-gate-covers-every-job: FAIL: $*" >&2
  if [ -n "${out:-}" ] && [ -f "$out" ]; then
    echo "--- the check said ---" >&2
    cat "$out" >&2
    echo "----------------------" >&2
  fi
  exit 1
}

pass() {
  echo "  ok  $*"
}

# Set by `scratch`, which is called plainly rather than through a command
# substitution: a subshell would build the fixture and then throw away the
# paths to it.
root=""
ci_yml=""
security_yml=""
out=""

# A pipeline shaped like this repository's, in a directory of its own.
scratch() {
  root="$work/$1"
  rm -rf "$root"
  mkdir -p "$root/tools/ci" "$root/.github/workflows"
  cp "$script" "$root/tools/ci/gate-covers-every-job.sh"
  ci_yml="$root/.github/workflows/ci.yml"
  security_yml="$root/.github/workflows/security.yml"
  out="$root/output"

  cat > "$ci_yml" <<'YAML'
name: CI

on:
  pull_request:
  push:
    branches:
      - main

permissions:
  contents: read

jobs:
  toolchain:
    name: Toolchain
    runs-on: ubuntu-latest
    steps:
      - run: true

  format:
    name: Format
    needs: toolchain
    runs-on: ubuntu-latest
    steps:
      - run: true

  clippy:
    name: Clippy
    needs: toolchain
    runs-on: ubuntu-latest
    steps:
      - run: true

  test:
    name: Test
    needs: toolchain
    runs-on: ubuntu-latest
    steps:
      - run: true

  bench-compile:
    name: Bench Compile
    needs: toolchain
    runs-on: ubuntu-latest
    steps:
      - run: true

  metadata:
    name: Metadata
    needs: toolchain
    runs-on: ubuntu-latest
    steps:
      - run: true

  # `ci-gate excludes moving-nightly` — it builds on the moving channel to say
  # early when the pin will have to move, so it is expected to go red.
  moving-nightly:
    name: Moving Nightly
    needs: toolchain
    runs-on: ubuntu-latest
    steps:
      - run: true

  ci:
    name: CI
    if: always()
    needs:
      - toolchain
      - format
      - clippy
      - test
      - bench-compile
      - metadata
    runs-on: ubuntu-latest
    steps:
      - run: true
YAML

  cat > "$security_yml" <<'YAML'
name: Security

on:
  pull_request:

permissions: {}

jobs:
  zizmor:
    name: Zizmor
    runs-on: ubuntu-latest
    steps:
      - run: true
YAML

}

# Edits to the fixtures. Through a temporary file rather than `sed -i`, which
# spells its argument differently on the two platforms this repository is
# written on and run on — and in `awk` wherever a line is added, because BSD
# `sed` does not read `\n` in a replacement as a newline and GNU `sed` does.
edit() {
  file="$1"
  shift
  sed "$@" "$file" > "$file.edited"
  mv "$file.edited" "$file"
}

insert_after() {
  awk -v anchor="$2" -v added="$3" '{ print } $0 == anchor { print added }' \
    "$1" > "$1.edited"
  mv "$1.edited" "$1"
}

replace_line() {
  awk -v old="$2" -v new="$3" '{ print ($0 == old) ? new : $0 }' \
    "$1" > "$1.edited"
  mv "$1.edited" "$1"
}

check() {
  ( cd "$1" && sh tools/ci/gate-covers-every-job.sh > "$out" 2>&1 )
}

# The message has to name the thing that is wrong. A check whose complaint does
# not say which job it is about sends the reader back to the file to guess.
said() {
  grep -q -- "$1" "$out" || fail "the message never mentions '$1'"
}

# 1. A pipeline whose gate names every job it should.
scratch clean
check "$root" || fail "rejected a pipeline whose gate covers every job"
said "gate-covers-every-job: ok"
pass "passes when the gate names every job"

# 2. The failure this check exists for: a job is added to `ci.yml` and nobody
#    adds it to the gate. Under branch protection that job's result cannot
#    block a merge, and `Toolchain` failing skips it into a pass.
scratch uncovered
cat >> "$ci_yml" <<'YAML'

  brand-new:
    name: Brand New
    needs: toolchain
    runs-on: ubuntu-latest
    steps:
      - run: true
YAML
if check "$root"; then
  fail "accepted a job that is in neither the gate's needs nor an exclusion"
fi
said "brand-new"
pass "rejects a new job the gate does not name"

# 3. And the fix it asks for: the job goes in the list.
insert_after "$ci_yml" "      - metadata" "      - brand-new"
check "$root" || fail "still rejected the job after it was added to the gate's needs"
pass "accepts the same job once the gate needs it"

# 4. The other fix: a job that must not gate a merge is excluded in writing.
scratch excluded
cat >> "$ci_yml" <<'YAML'

  # `ci-gate excludes brand-new` — it talks to a service that is down as often
  # as it is up, and a merge cannot wait on that.
  brand-new:
    name: Brand New
    needs: toolchain
    runs-on: ubuntu-latest
    steps:
      - run: true
YAML
check "$root" || fail "rejected a job excluded by a marker that names it and says why"
said "excluded   brand-new"
pass "accepts a job excluded by a marker that gives a reason"

# 5. But not a waiver with nothing behind it. A marker with no sentence after
#    it is a job removed from the gate by somebody who did not have to argue
#    for it, which is how a gate empties out one job at a time.
scratch unargued
cat >> "$ci_yml" <<'YAML'

  # ci-gate excludes brand-new
  brand-new:
    name: Brand New
    needs: toolchain
    runs-on: ubuntu-latest
    steps:
      - run: true
YAML
if check "$root"; then
  fail "accepted an exclusion with no reason given"
fi
said "gives no reason"
pass "rejects an exclusion with no reason"

# 6. A marker naming a job that is not there — a rename that moved the job and
#    left the waiver behind, which would silently excuse the next job to take
#    that name.
scratch stale-marker
edit "$ci_yml" -e 's/ci-gate excludes moving-nightly/ci-gate excludes nightly-moving/'
if check "$root"; then
  fail "accepted a marker naming a job that does not exist"
fi
said "nightly-moving"
pass "rejects a marker that names no job"

# 7. The gate without `if: always()`. This is the bug itself, in the one job
#    that is supposed to be immune to it: a gate that needs `toolchain` and is
#    not `always()` skips when `toolchain` fails, and a skipped required check
#    is a pass.
scratch not-always
edit "$ci_yml" -e '/^    if: always()$/d'
if check "$root"; then
  fail "accepted a gate that is not \`if: always()\`"
fi
said "always()"
pass "rejects a gate that can skip with everything else"

# 8. A `needs:` naming a job that has been deleted. GitHub does not run such a
#    workflow at all — it is not a soft failure — so every required context
#    goes missing at once and every pull request blocks. See #197.
scratch dangling
replace_line "$ci_yml" "      - bench-compile" "      - bench-compiles"
if check "$root"; then
  fail "accepted a gate that needs a job which does not exist"
fi
said "bench-compiles"
pass "rejects a dangling needs"

# 9. A required context renamed. Branch protection names the check, not the
#    job, so renaming `Format` leaves a required context nothing reports and
#    the job it used to name free to fail.
scratch renamed
replace_line "$ci_yml" "    name: Format" "    name: Formatting"
if check "$root"; then
  fail "accepted a workflow in which no job reports as a required context"
fi
said "Format"
pass "rejects a renamed required context"

# 10. A required context that is not in `ci.yml` at all. `Zizmor` cannot skip
#     today because it depends on nothing; the moment it does, it inherits the
#     whole of #367 and needs a gate of its own.
scratch zizmor-needs
insert_after "$security_yml" "    name: Zizmor" "    needs: something-else"
if check "$root"; then
  fail "accepted a required context outside ci.yml that can now skip"
fi
said "Zizmor"
pass "rejects a required context that gains a dependency outside the gate"

# 11. The same hole by its other route: a condition. A job whose `if:` is false
#     reports `skipped`, and GitHub reads that as satisfied exactly as it reads
#     a skipped dependency.
scratch conditional
insert_after "$ci_yml" "    name: Test" "    if: github.event_name == 'push'"
if check "$root"; then
  fail "accepted a required job that carries a condition"
fi
said "Test"
pass "rejects a required job made conditional"

# 12. But not `if: always()`, which is the one condition that cannot make a job
#     skip — it is what the gate itself carries, and a job that guarded against
#     a failed dependency this way would be closing the same hole rather than
#     opening it.
scratch unconditional
insert_after "$ci_yml" "    name: Test" "    if: always()"
check "$root" || fail "rejected a required job whose condition is \`always()\`"
pass "accepts a required job that is \`if: always()\`"

# 13. And the gate renamed out from under the setting that requires it.
scratch gate-renamed
replace_line "$ci_yml" "    name: CI" "    name: Everything"
if check "$root"; then
  fail "accepted a gate that no longer reports as the required context"
fi
said "Everything"
pass "rejects a gate renamed away from its required context"

echo "test-gate-covers-every-job: ok"
