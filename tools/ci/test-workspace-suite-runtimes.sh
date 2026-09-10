#!/usr/bin/env sh
# `workspace-suite-runtimes.sh`, against pipelines with a defect planted in
# them. A check nobody has watched fail is a check nobody knows the shape of.
set -eu

root="$(CDPATH= cd "$(dirname "$0")/../.." && pwd)"
check="$root/tools/ci/workspace-suite-runtimes.sh"

work="${TMPDIR:-/tmp}/uf-suite-runtimes-test.$$"
rm -rf "$work"
mkdir -p "$work/.github/workflows" "$work/crates/uf_cli/tests" "$work/tools/ci"
trap 'rm -rf "$work"' EXIT
cp "$check" "$work/tools/ci/"

# The three test files the check keys off. Their contents do not matter; the
# check asks whether they exist, because a runtime is required exactly when
# something starts it.
: > "$work/crates/uf_cli/tests/bun_host.rs"
: > "$work/crates/uf_cli/tests/deno_host.rs"
: > "$work/crates/uf_cli/tests/permissions.rs"

failures=0

# `case` "$1" describes the defect; the rest is the workflow to write.
plant() {
  cat > "$work/.github/workflows/pipeline.yml"
}

expect() {
  want="$1"
  why="$2"
  if ( cd "$work" && sh tools/ci/workspace-suite-runtimes.sh >"$work/out" 2>&1 ); then
    got=0
  else
    got=1
  fi
  if [ "$got" != "$want" ]; then
    echo "FAIL: $why (wanted exit $want, got $got)" >&2
    sed 's/^/    /' "$work/out" >&2
    failures=$((failures + 1))
  fi
}

names() {
  if ! grep -q "$1" "$work/out"; then
    echo "FAIL: the message does not name '$1'" >&2
    sed 's/^/    /' "$work/out" >&2
    failures=$((failures + 1))
  fi
}

echo "a job with all three runtimes passes"
plant <<'YAML'
name: Pipeline
on: [push]
jobs:
  suite:
    runs-on: ubuntu-latest
    steps:
      - uses: oven-sh/setup-bun@v2
      - uses: denoland/setup-deno@v2
      - uses: actions/setup-node@v7
      - run: cargo test --workspace
YAML
expect 0 "all three present"

echo "a missing runtime is named, with the file that starts it"
plant <<'YAML'
name: Pipeline
on: [push]
jobs:
  suite:
    runs-on: ubuntu-latest
    steps:
      - uses: oven-sh/setup-bun@v2
      - uses: actions/setup-node@v7
      - run: cargo test --workspace
YAML
expect 1 "deno missing"
names "without deno"
names "crates/uf_cli/tests/deno_host.rs"

echo "the suite reached through uf run rust:test counts"
plant <<'YAML'
name: Pipeline
on: [push]
jobs:
  suite:
    runs-on: ubuntu-latest
    steps:
      - uses: oven-sh/setup-bun@v2
      - run: ./target/release/uf run rust:test
YAML
expect 1 "rust:test is the same command"

echo "the suite reached through uf run ci counts"
plant <<'YAML'
name: Pipeline
on: [push]
jobs:
  suite:
    runs-on: ubuntu-latest
    steps:
      - run: ./target/release/uf run ci
YAML
expect 1 "ci depends on rust:test"

echo "uf run ci:gate is not uf run ci"
plant <<'YAML'
name: Pipeline
on: [push]
jobs:
  gate:
    runs-on: ubuntu-latest
    steps:
      - run: ./target/release/uf run ci:gate
      - run: ./target/release/uf run ci:gate:test
  suite:
    runs-on: ubuntu-latest
    steps:
      - uses: oven-sh/setup-bun@v2
      - uses: denoland/setup-deno@v2
      - uses: actions/setup-node@v7
      - run: cargo test --workspace
YAML
expect 0 "a task whose name merely begins with ci is not the suite"

echo "a comment naming the command is prose, not a step"
plant <<'YAML'
name: Pipeline
on: [push]
jobs:
  docs:
    runs-on: ubuntu-latest
    steps:
      # A check that is in the pipeline and not in `uf run ci` is a check a
      # contributor cannot run before pushing.
      - run: echo nothing
  suite:
    runs-on: ubuntu-latest
    steps:
      - uses: oven-sh/setup-bun@v2
      - uses: denoland/setup-deno@v2
      - uses: actions/setup-node@v7
      - run: cargo test --workspace
YAML
expect 0 "a comment is not a command"

echo "a runtime with no test file is not required"
rm "$work/crates/uf_cli/tests/deno_host.rs"
plant <<'YAML'
name: Pipeline
on: [push]
jobs:
  suite:
    runs-on: ubuntu-latest
    steps:
      - uses: oven-sh/setup-bun@v2
      - uses: actions/setup-node@v7
      - run: cargo test --workspace
YAML
expect 0 "nothing starts deno, so nothing needs it"
: > "$work/crates/uf_cli/tests/deno_host.rs"

echo "a pipeline that runs no suite is refused rather than passing silently"
plant <<'YAML'
name: Pipeline
on: [push]
jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - run: cargo clippy --workspace --all-targets --all-features -- -D warnings
YAML
expect 1 "a check that checks nothing has stopped being a check"
names "stopped checking anything"

if [ "$failures" -ne 0 ]; then
  echo "$failures case(s) failed" >&2
  exit 1
fi
echo "workspace-suite-runtimes: every case behaved"
