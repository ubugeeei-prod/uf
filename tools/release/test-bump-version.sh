#!/bin/sh
# `bump-version.sh` against a repository it can safely rewrite.
#
# The bump is run by hand, once per release, on a tree that is about to become
# a tag — so a mistake in it is committed before anyone sees it. Two have been:
#
#   * the changelog is written by `uf release <bump>`, which reads the version
#     off the workspace it is about to leave. Run the bump first and the
#     changelog is headed one version too far, which nothing downstream
#     notices;
#   * re-running a bump for the version the tree already carries — how a
#     release that failed halfway is resumed — reported "workspace version not
#     found in Cargo.toml" about a file that plainly has it, because the script
#     read "the replacement changed nothing" as "there was nothing to change".
#
# Both are about the second run, not the first, which is why the release that
# went right did not catch them. So the script is run here against a scratch
# repository: the same script, at the same path relative to a root it may
# rewrite, with `cargo` and `npm` stubbed out — what is under test is which
# files carry the version, not the two lockfile refreshes at the end.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
script="$repo_root/tools/release/bump-version.sh"

work="$(mktemp -d "${TMPDIR:-/tmp}/uf-test-bump-version.XXXXXX")"
trap 'rm -rf "$work"' EXIT INT TERM

fail() {
  echo "test-bump-version: FAIL: $*" >&2
  exit 1
}

pass() {
  echo "  ok  $*"
}

# `cargo metadata` and `npm install --package-lock-only` end the script. Both
# want a real workspace and a network; neither decides anything this test is
# about. Stubs that record their arguments keep them in the assertions anyway:
# a bump that stops refreshing the lockfiles is a bump that leaves `--locked`
# and `npm ci` red.
mkdir -p "$work/bin"
for tool in cargo npm; do
  cat > "$work/bin/$tool" <<STUB
#!/bin/sh
echo "$tool \$*" >> "\$STUB_LOG"
STUB
  chmod +x "$work/bin/$tool"
done
PATH="$work/bin:$PATH"
export PATH

# A repository shaped like this one: a workspace version every crate inherits,
# packages that pin each other exactly, and a docs manifest that depends on the
# packages without being one.
scratch() {
  root="$work/$1"
  version="$2"
  rm -rf "$root"
  mkdir -p "$root/tools/release" "$root/packages/core" "$root/packages/cli" "$root/docs"
  cp "$script" "$root/tools/release/bump-version.sh"
  cat > "$root/Cargo.toml" <<TOML
[workspace]
members = ["crates/*"]

[workspace.package]
version = "$version"
edition = "2024"
TOML
  cat > "$root/packages/core/package.json" <<JSON
{
  "name": "@uniflowed/core",
  "version": "$version",
  "dependencies": { "@uniflowed/std": "$version" },
  "peerDependencies": { "react": "^19.0.0" }
}
JSON
  cat > "$root/packages/cli/package.json" <<JSON
{
  "name": "@uniflowed/cli",
  "version": "$version",
  "devDependencies": { "@uniflowed/core": "$version" },
  "optionalDependencies": { "@uniflowed/cli-darwin-arm64": "$version" }
}
JSON
  cat > "$root/docs/package.json" <<JSON
{
  "name": "docs",
  "private": true,
  "version": "0.0.0",
  "dependencies": { "@uniflowed/core": "$version", "vite": "^7.0.0" }
}
JSON
  printf '# Changelog\n\n## uf@%s\n\n_2026-01-01_\n' "$version" > "$root/CHANGELOG.md"
}

# The version a manifest declares, and the version it pins a sibling at.
declares() { node -e 'process.stdout.write(String(require(process.argv[1]).version))' "$1"; }
pins() { node -e 'const m=require(process.argv[1]);for(const f of ["dependencies","peerDependencies","devDependencies","optionalDependencies"])if(m[f]?.[process.argv[2]])process.stdout.write(m[f][process.argv[2]])' "$1" "$2"; }

run() {
  root="$work/$1"
  shift
  STUB_LOG="$root/stub.log"
  export STUB_LOG
  : > "$STUB_LOG"
  set +e
  out="$("$root/tools/release/bump-version.sh" "$@" 2>&1)"
  status=$?
  set -e
}

# --- a bump rewrites every file that carries the version ---------------------
scratch happy 0.1.0
printf '\n## uf@0.2.0\n\n_2026-01-02_\n' >> "$work/happy/CHANGELOG.md"
run happy 0.2.0
[ "$status" -eq 0 ] || fail "a bump with a changelog section exited $status: $out"
grep -q '^version = "0.2.0"$' "$work/happy/Cargo.toml" || fail "Cargo.toml keeps the old workspace version"
[ "$(declares "$work/happy/packages/core/package.json")" = "0.2.0" ] || fail "packages/core keeps its old version"
[ "$(declares "$work/happy/packages/cli/package.json")" = "0.2.0" ] || fail "packages/cli keeps its old version"
pass "the workspace and every package declare the new version"

for field in "packages/core/package.json @uniflowed/std" \
             "packages/cli/package.json @uniflowed/core" \
             "packages/cli/package.json @uniflowed/cli-darwin-arm64" \
             "docs/package.json @uniflowed/core"; do
  set -- $field
  [ "$(pins "$work/happy/$1" "$2")" = "0.2.0" ] || fail "$1 still pins $2 at the old version"
done
pass "every \`@uniflowed/*\` pin moves, in all four dependency fields"

[ "$(declares "$work/happy/docs/package.json")" = "0.0.0" ] || fail "docs/package.json is not shipped and must keep its own version"
grep -q '"vite": "\^7.0.0"' "$work/happy/docs/package.json" || fail "a dependency outside the scope was rewritten"
pass "a manifest that is not shipped keeps its version, and foreign pins are left alone"

grep -q '^cargo metadata' "$work/happy/stub.log" || fail "Cargo.lock was not refreshed"
grep -q '^npm install .*--package-lock-only' "$work/happy/stub.log" || fail "package-lock.json was not refreshed"
pass "both lockfiles are refreshed"

# --- the changelog is written first, and the script says so ------------------
scratch order 0.1.0
run order 0.2.0
[ "$status" -eq 2 ] || fail "a bump past the changelog exited $status, not 2"
case "$out" in
  *CHANGELOG.md*) ;;
  *) fail "the refusal does not name CHANGELOG.md: $out" ;;
esac
grep -q '^version = "0.1.0"$' "$work/order/Cargo.toml" || fail "the refusal still rewrote Cargo.toml"
[ "$(declares "$work/order/packages/core/package.json")" = "0.1.0" ] || fail "the refusal still rewrote a package"
pass "a bump to a version the changelog does not have is refused, and changes nothing"

# --- a bump can be run again ------------------------------------------------
scratch resume 0.2.0
run resume 0.2.0
[ "$status" -eq 0 ] || fail "re-running a completed bump exited $status: $out"
grep -q '^version = "0.2.0"$' "$work/resume/Cargo.toml" || fail "the re-run moved the workspace version"
[ "$(declares "$work/resume/packages/core/package.json")" = "0.2.0" ] || fail "the re-run moved a package version"
pass "a bump to the version the tree already carries is a no-op that succeeds"

# --- and a Cargo.toml that really has no version still says so ---------------
scratch missing 0.1.0
printf '\n## uf@0.2.0\n' >> "$work/missing/CHANGELOG.md"
cat > "$work/missing/Cargo.toml" <<'TOML'
[workspace]
members = ["crates/*"]
TOML
run missing 0.2.0
[ "$status" -ne 0 ] || fail "a Cargo.toml with no workspace version was accepted"
case "$out" in
  *"workspace version not found"*) ;;
  *) fail "the failure does not say the version is missing: $out" ;;
esac
pass "a Cargo.toml with no version is still an error, not a silent success"

# --- the argument is checked before anything is touched ----------------------
scratch args 0.1.0
run args
[ "$status" -eq 2 ] || fail "a missing version exited $status, not 2"
run args "alpha.5"
[ "$status" -eq 2 ] || fail "\`alpha.5\` was accepted as a semantic version"
grep -q '^version = "0.1.0"$' "$work/args/Cargo.toml" || fail "a rejected argument still rewrote Cargo.toml"
pass "a missing or malformed version is refused before any file is written"

echo "test-bump-version: all checks passed"
