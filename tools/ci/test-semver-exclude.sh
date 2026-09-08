#!/bin/sh
# `semver-exclude.sh` against workspaces it can safely make wrong.
#
# The list of crates that cannot be compared used to be written down, with a
# comment saying it "grows as more crates print from or read the parser's
# syntax tree". #633 is what a list somebody has to remember looks like when
# they do not: `uf_i18n` arrived with a path dependency on `uf_flow`, was in
# nobody's list, and `Cargo Semver Checks` then failed on `cargo update` for
# *every* pull request afterwards — including the one cutting a release. The
# job that guards against a silent break became the break.
#
# So the closure is computed, and computing it is a thing that can be wrong in
# both directions. Too narrow and the release is blocked again; too wide and
# crates leave the gate quietly, which is worse because nothing goes red. Each
# case below plants one shape and says which side of that line it is on.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
script="$repo_root/tools/ci/semver-exclude.sh"

work="$(mktemp -d "${TMPDIR:-/tmp}/uf-test-semver-exclude.XXXXXX")"
trap 'rm -rf "$work"' EXIT INT TERM

fail() {
  echo "test-semver-exclude: FAIL: $*" >&2
  [ -n "${out:-}" ] && echo "  it printed: $out" >&2
  exit 1
}

pass() {
  echo "  ok  $*"
}

root=""
out=""

# A crate with the dependencies named after `--`, each as a path dependency.
# `--dev` moves the ones after it into `[dev-dependencies]`.
crate() {
  name="$1"
  shift
  mkdir -p "$root/crates/$name"
  {
    echo '[package]'
    echo "name = \"$name\""
    echo 'version = "0.0.0"'
    echo ''
    echo '[dependencies]'
    section=deps
    for dep in "$@"; do
      if [ "$dep" = "--dev" ]; then
        echo ''
        echo '[dev-dependencies]'
        section=dev
        continue
      fi
      echo "$dep = { path = \"../$dep\" }"
    done
  } > "$root/crates/$name/Cargo.toml"
}

# A workspace in a git repository, because the check asks git what existed at
# the baseline.
scratch() {
  root="$work/$1"
  rm -rf "$root"
  mkdir -p "$root/crates"
  mkdir -p "$root/tools/ci"
  cp "$script" "$root/tools/ci/semver-exclude.sh"
  ( cd "$root" && git init --quiet && git config user.email uf@example.com \
      && git config user.name uf )
}

commit_baseline() {
  ( cd "$root" && git add -A && git commit --quiet -m baseline )
}

run() {
  out="$( cd "$root" && sh tools/ci/semver-exclude.sh "$1" )"
}

# Is $1 one of the comma-separated names in $out?
excluded() {
  case ",$out," in
    *",$1,"*) return 0 ;;
  esac
  return 1
}

# 1. The roots themselves, and a crate that names one directly.
scratch direct
crate uf_flow
crate uf_check
crate uf_fmt uf_flow
crate uf_std
commit_baseline
run HEAD
excluded uf_flow || fail "uf_flow is a root and was not excluded"
excluded uf_check || fail "uf_check is a root and was not excluded"
excluded uf_fmt || fail "a crate depending on uf_flow was not excluded"
excluded uf_std && fail "uf_std reaches nothing and must stay in the gate"
pass "excludes the roots and what depends on them, and nothing else"

# 2. Transitively, which is the half a written list gets wrong first: nobody
#    adding a dependency on `uf_fmt` thinks of themselves as adding one on the
#    submodule.
scratch transitive
crate uf_flow
crate uf_check
crate uf_fmt uf_flow
crate uf_lint uf_fmt
crate uf_cli uf_lint
crate uf_std
commit_baseline
run HEAD
excluded uf_lint || fail "a crate two hops from uf_flow was not excluded"
excluded uf_cli || fail "a crate three hops from uf_flow was not excluded"
excluded uf_std && fail "uf_std still reaches nothing"
pass "follows the chain however long it is"

# 3. And not through `[dev-dependencies]`. `cargo update` resolves the baseline
#    without them, so a crate that only tests against the parser builds fine —
#    excluding it would take it out of the gate for nothing.
scratch dev-only
crate uf_flow
crate uf_check
crate uf_lib uf_std --dev uf_flow
crate uf_std
commit_baseline
run HEAD
excluded uf_lib && fail "a dev-only dependency on uf_flow must not exclude a crate"
pass "a dev-dependency on the parser is not a reason to leave the gate"

# 4. The other reason, which is temporary: a crate that did not exist at the
#    baseline has nothing to be compared against, and cargo-semver-checks ends
#    the whole run rather than skipping it.
scratch new-crate
crate uf_flow
crate uf_check
crate uf_std
commit_baseline
crate uf_brand_new
run HEAD
excluded uf_brand_new || fail "a crate absent from the baseline was not excluded"
excluded uf_std && fail "a crate present at the baseline stays in the gate"
pass "excludes a crate the baseline has never seen"

# 5. A cycle. Cargo forbids one, but the closure is a fixed point and a fixed
#    point that does not terminate is a job that hangs rather than fails, which
#    is the worst way for this to be wrong.
scratch cycle
crate uf_flow
crate uf_check
crate uf_a uf_b
crate uf_b uf_a
crate uf_std
commit_baseline
run HEAD
excluded uf_a && fail "a cycle that reaches nothing must not be excluded"
pass "terminates on a cycle rather than hanging"

echo "test-semver-exclude: ok"
