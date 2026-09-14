#!/bin/sh
# `publish-names-exist.sh` against an npm stub, without touching the registry.
#
# The real publish job cannot read trusted-publisher bindings, but it can avoid
# the easiest half-send: a name in `published-packages.txt` that npm has never
# seen. This pins the all-names-before-any-publish guard.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
cd "$repo_root"

script="tools/release/publish-names-exist.sh"
work="$(mktemp -d "${TMPDIR:-/tmp}/uf-test-publish-names-exist.XXXXXX")"
trap 'rm -rf "$work"' EXIT INT TERM

fail() {
  echo "test-publish-names-exist: FAIL: $*" >&2
  exit 1
}

pass() {
  echo "  ok  $*"
}

make_npm() {
  mkdir -p "${work}/bin"
  cat >"${work}/bin/npm" <<'EOF'
#!/bin/sh
case "$1" in
  view) ;;
  *) echo "npm stub: unexpected command $1" >&2; exit 1 ;;
esac

for package in ${NPM_MISSING:-}; do
  if [ "$2" = "@uniflowed/${package}" ]; then
    exit 1
  fi
done

echo "$2"
EOF
  chmod +x "${work}/bin/npm"
}

run() {
  label="$1"
  make_npm
  NPM_MISSING="${NPM_MISSING:-}" PATH="${work}/bin:$PATH" \
    sh "$script" >"${work}/${label}.log" 2>&1
}

run all-present || fail "a registry with every name was refused:
$(cat "${work}/all-present.log")"
grep -q "every listed package name exists" "${work}/all-present.log" \
  || fail "success did not say every name exists:
$(cat "${work}/all-present.log")"
pass "all names present passes"

# The two names the stub withholds are read from the list rather than written
# here. The script walks the real `published-packages.txt`, so a name spelled
# into this test is a claim about that file's contents: this test named
# `temporal`, and failed on #952 — the change that rightly took `temporal` off
# the list — while the guard it pins was working. The first and last entries
# keep the two in list order, which is the order the commands print them in.
listed="$(grep -vE '^[[:space:]]*(#|$)' tools/release/published-packages.txt)"
first="$(printf '%s\n' "$listed" | sed -n '1p')"
last="$(printf '%s\n' "$listed" | sed -n '$p')"
[ -n "$first" ] && [ "$first" != "$last" ] \
  || fail "tools/release/published-packages.txt needs at least two names for this test"

if (NPM_MISSING="$first $last"; run missing); then
  fail "missing names reported success:
$(cat "${work}/missing.log")"
fi
grep -q "missing     @uniflowed/${first}\$" "${work}/missing.log" \
  || fail "${first} was not reported missing:
$(cat "${work}/missing.log")"
grep -q "missing     @uniflowed/${last}\$" "${work}/missing.log" \
  || fail "${last} was not reported missing:
$(cat "${work}/missing.log")"
grep -q "before any package is published" "${work}/missing.log" \
  || fail "failure did not explain why it is before publish:
$(cat "${work}/missing.log")"
grep -q "bootstrap-publish.sh --package ${first} --package ${last}" "${work}/missing.log" \
  || fail "failure did not print the targeted bootstrap:
$(cat "${work}/missing.log")"
grep -q "trust-npm.sh --package ${first} --package ${last}" "${work}/missing.log" \
  || fail "failure did not print the targeted trust command:
$(cat "${work}/missing.log")"
pass "every missing name is reported before publish"

echo "test-publish-names-exist: ok"
