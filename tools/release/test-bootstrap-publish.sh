#!/bin/sh
# `bootstrap-publish.sh` against an npm stub, without touching the registry.
#
# The bootstrap is the one local release step that creates package names. A
# package in `pending-packages.txt` is not safe to move to the published list
# until the name exists and trusted publishing is bound, so the bootstrap has
# to read both release manifests. This test pins that: the packages waiting in
# the pending room are the ones the real script must publish first.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
cd "$repo_root"

script="tools/release/bootstrap-publish.sh"
work="$(mktemp -d "${TMPDIR:-/tmp}/uf-test-bootstrap-publish.XXXXXX")"
trap 'rm -rf "$work"' EXIT INT TERM

fail() {
  echo "test-bootstrap-publish: FAIL: $*" >&2
  exit 1
}

pass() {
  echo "  ok  $*"
}

make_npm() {
  world="$1"
  mkdir -p "${work}/${world}"
  cat > "${work}/${world}/npm" <<'EOF'
#!/bin/sh
case "$1" in
  whoami)
    [ -n "${NPM_NO_LOGIN:-}" ] && exit 1
    echo "tester"
    exit 0 ;;
  view)
    [ -n "${NPM_ALL_PRESENT:-}" ] && { echo "$2"; exit 0; }
    case "$2" in
      @uniflowed/temporal | @uniflowed/std) exit 1 ;;
      *) echo "$2"; exit 0 ;;
    esac ;;
  publish) ;;
  *) echo "npm stub: unexpected command $1" >&2; exit 1 ;;
esac

name="$(node -p "require('./package.json').name")"
mode=publish
for argument in "$@"; do
  [ "$argument" = "--dry-run" ] && mode=dry-run
done
echo "${mode} ${name} $*" >> "$NPM_LOG"

if [ "$mode" = dry-run ] && [ "${NPM_REFUSE_DRY_RUN:-}" = "$name" ]; then
  echo "npm error dry run refused ${name}" >&2
  exit 1
fi

echo "+ ${name}"
EOF
  chmod +x "${work}/${world}/npm"
}

run() {
  world="$1"
  label="$2"
  shift 2
  make_npm "$world"
  : >"${work}/${label}.npm"
  NPM_ALL_PRESENT="${NPM_ALL_PRESENT:-}" \
    NPM_REFUSE_DRY_RUN="${NPM_REFUSE_DRY_RUN:-}" \
    NPM_NO_LOGIN="${NPM_NO_LOGIN:-}" \
    NPM_LOG="${work}/${label}.npm" PATH="${work}/${world}:$PATH" \
    sh "$script" "$@" >"${work}/${label}.log" 2>&1
}

# 1. The two names currently in the pending room are missing from the registry,
#    so bootstrap publishes them even though they are not in the published list.
run missing-pending pending --yes || fail "pending names were not bootstrapped:
$(cat "${work}/pending.log")"
grep -q '^dry-run @uniflowed/temporal ' "${work}/pending.npm" \
  || fail "temporal was not dry-run packed:
$(cat "${work}/pending.npm")"
grep -q '^publish @uniflowed/temporal ' "${work}/pending.npm" \
  || fail "temporal was not published:
$(cat "${work}/pending.npm")"
grep -q '^dry-run @uniflowed/std ' "${work}/pending.npm" \
  || fail "std was not dry-run packed:
$(cat "${work}/pending.npm")"
grep -q '^publish @uniflowed/std ' "${work}/pending.npm" \
  || fail "std was not published:
$(cat "${work}/pending.npm")"
pass "pending names are bootstrapped"

# 2. A registry that already has every name is a no-op.
(NPM_ALL_PRESENT=1; run all-present settled --yes) \
  || fail "a fully bootstrapped registry was refused:
$(cat "${work}/settled.log")"
[ ! -s "${work}/settled.npm" ] \
  || fail "settled registry still published something:
$(cat "${work}/settled.npm")"
grep -q "nothing to do" "${work}/settled.log" \
  || fail "settled registry did not report a no-op:
$(cat "${work}/settled.log")"
pass "already-created names are left alone"

# 3. The dry run phase covers every missing name before any real publish, so a
#    bad tarball cannot leave the registry half-created.
if (NPM_REFUSE_DRY_RUN=@uniflowed/std; run missing-pending dry-fails --yes); then
  fail "a dry-run failure reported success:
$(cat "${work}/dry-fails.log")"
fi
grep -q '^publish ' "${work}/dry-fails.npm" \
  && fail "real publish ran after a dry-run failure:
$(cat "${work}/dry-fails.npm")"
pass "a dry-run failure stops before real publish"

# 4. No npm session, no names created.
if (NPM_NO_LOGIN=1; run missing-pending no-login --yes); then
  fail "not logged in reported success:
$(cat "${work}/no-login.log")"
fi
grep -q "not logged in" "${work}/no-login.log" \
  || fail "not logged in did not say why:
$(cat "${work}/no-login.log")"
[ ! -s "${work}/no-login.npm" ] \
  || fail "not logged in still published something:
$(cat "${work}/no-login.npm")"
pass "a missing npm session stops before publish"

echo "test-bootstrap-publish: ok"
