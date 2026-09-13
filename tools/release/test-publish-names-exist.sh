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

if (NPM_MISSING="temporal vite"; run missing); then
  fail "missing names reported success:
$(cat "${work}/missing.log")"
fi
grep -q "missing     @uniflowed/temporal" "${work}/missing.log" \
  || fail "temporal was not reported missing:
$(cat "${work}/missing.log")"
grep -q "missing     @uniflowed/vite" "${work}/missing.log" \
  || fail "vite was not reported missing:
$(cat "${work}/missing.log")"
grep -q "before any package is published" "${work}/missing.log" \
  || fail "failure did not explain why it is before publish:
$(cat "${work}/missing.log")"
pass "every missing name is reported before publish"

echo "test-publish-names-exist: ok"
