#!/bin/sh
# `trust-npm.sh` against npms that behave differently, without touching npm.
#
# This script is the whole of the release bootstrap and it is run by hand,
# once, by one person, on one machine — so every mistake in it costs a round
# trip and blocks a release. It has cost three:
#
#   * it passed `--allow-publish`, which one npm requires and another rejects
#     as an unknown flag;
#   * a guard was added that built its own argument list, so the guard passed
#     while the bind it guarded could not run;
#   * the shared argument list then passed `"${2:-}"`, an empty positional
#     that only the real bind hits because the dry run always has one:
#     `npm error Unknown positional argument:`.
#
# Each was a shape of `npm trust github` that no stub was strict enough to
# see. The stubs here parse their arguments the way npm does and refuse
# anything they were not given, which is what makes them worth running.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
cd "$repo_root"

script="tools/release/trust-npm.sh"
work="$(mktemp -d "${TMPDIR:-/tmp}/uf-test-trust-npm.XXXXXX")"
trap 'rm -rf "$work"' EXIT INT TERM

names="$(grep -cvE '^[[:space:]]*(#|$)' tools/release/published-packages.txt)"

fail() {
  echo "test-trust-npm: FAIL: $*" >&2
  exit 1
}

pass() {
  echo "  ok  $*"
}

# An npm that parses like npm: named flags take their values, an unknown flag
# is an error, and a second positional — or an empty one — is an error.
#
# $1 names the flavour: `permissive` accepts `--allow-publish`, `strict`
# requires it, `ancient` has no such subcommand.
make_npm() {
  flavour="$1"
  mkdir -p "${work}/${flavour}"
  cat > "${work}/${flavour}/npm" <<EOF
#!/bin/sh
flavour="${flavour}"
EOF
  cat >> "${work}/${flavour}/npm" <<'EOF'
case "$1" in
  --version) echo "11.99.0"; exit 0 ;;
  whoami) echo "tester"; exit 0 ;;
  trust) ;;
  *) exit 0 ;;
esac
shift
if [ "${1:-}" = "list" ]; then
  case "$flavour" in
    conflicting) echo "  file: some-other.yml"; echo "  repository: someone/else"; exit 0 ;;
    existing) echo "  file: publish.yml"; echo "  repository: ubugeeei-prod/uf"; exit 0 ;;
    *) exit 1 ;;
  esac
fi
[ "${1:-}" = "github" ] || { echo "npm error Unknown subcommand: ${1:-}" >&2; exit 1; }
shift
if [ "${1:-}" = "--help" ]; then
  case "$flavour" in
    ancient) echo "npm trust github [package]" ;;
    permissive) echo "  --file (required)"; echo "  --repository|--repo" ;;
    *)       echo "  --file (required)"; echo "  --repository|--repo"
             echo "  --allow-publish"; echo "  --allow-stage-publish" ;;
  esac
  exit 0
fi
package=""; file=""; repository=""; permission=""; dry=""
while [ $# -gt 0 ]; do
  case "$1" in
    --file) file="${2:-}"; shift 2 ;;
    --repository|--repo) repository="${2:-}"; shift 2 ;;
    --allow-publish|--allow-stage-publish)
      case "$flavour" in
        permissive) echo "npm error Unknown flag: $1" >&2; exit 1 ;;
        *) permission="$1"; shift ;;
      esac ;;
    --yes|-y) shift ;;
    --dry-run) dry=1; shift ;;
    --*) echo "npm error Unknown flag: $1" >&2; exit 1 ;;
    "") echo "npm error Unknown positional argument: " >&2; exit 1 ;;
    *) if [ -z "$package" ]; then package="$1"; shift
       else echo "npm error Unknown positional argument: $1" >&2; exit 1; fi ;;
  esac
done
[ -n "$package" ] || { echo "npm error a package is required" >&2; exit 1; }
[ -n "$file" ] || { echo "npm error --file is required" >&2; exit 1; }
[ -n "$repository" ] || { echo "npm error --repository is required" >&2; exit 1; }
if [ "$flavour" = strict ] && [ -z "$permission" ]; then
  echo "npm error At least one permission flag is required (--allow-publish, --allow-stage-publish)" >&2
  exit 1
fi
[ -n "$dry" ] && exit 0
case "$flavour" in
  unpublished)
    echo "npm error code E404" >&2
    echo "npm error 404 Not Found - POST https://registry.npmjs.org/-/package/${package}/trust" >&2
    exit 1 ;;
  existing | conflicting)
    echo "npm error code E409" >&2
    echo "npm error 409 Conflict - a trusted publisher configuration that a token could also match already exists for this package" >&2
    exit 1 ;;
esac
echo "bound $package"
EOF
  chmod +x "${work}/${flavour}/npm"
}

run() {
  make_npm "$1"
  PATH="${work}/$1:$PATH" sh "$script" >"${work}/$1.log" 2>&1
}

# 1 and 2. Both flag sets bind every name. The script asks the help which one
#          this npm speaks rather than assuming either.
for flavour in strict permissive; do
  run "$flavour" || fail "${flavour}: the script failed:
$(cat "${work}/${flavour}.log")"
  # The script's own line, not the stub's: the stub's output goes into the
  # file the script captures so it can tell E409 from a real failure.
  bound="$(grep -c '^trust-npm: bound ' "${work}/${flavour}.log" || true)"
  [ "$bound" = "$names" ] \
    || fail "${flavour}: bound ${bound} of ${names}:
$(cat "${work}/${flavour}.log")"
  pass "${flavour} npm binds all ${names} names"
done

# 3. A name that already has this repository's configuration answers E409,
#    which is "already done" and not a failure. `set -eu` used to stop the
#    whole run on the first one.
run existing || fail "existing: E409 should not stop the run:
$(cat "${work}/existing.log")"
grep -q "already points at publish.yml" "${work}/existing.log" \
  || fail "existing: it did not say the configuration is the right one:
$(cat "${work}/existing.log")"
grep -q "0 bound, ${names} already configured" "${work}/existing.log" \
  || fail "existing: wrong summary:
$(cat "${work}/existing.log")"
pass "a name that is already configured for this workflow is reported, not fatal"

# 4. And one whose configuration names something else is a problem, because a
#    publish from this repository will be refused for it.
if run conflicting; then
  fail "conflicting: a configuration for another workflow should fail:
$(cat "${work}/conflicting.log")"
fi
grep -q "npm trust revoke" "${work}/conflicting.log" \
  || fail "conflicting: no way out was named:
$(cat "${work}/conflicting.log")"
pass "a configuration pointing somewhere else fails and names the fix"

# 5. A name the registry does not have answers E404. `npm trust` binds a name
#    the registry already has; it cannot create one. That is a different job,
#    named, rather than the end of this one.
if run unpublished; then
  fail "unpublished: E404 should end in a non-zero exit:
$(cat "${work}/unpublished.log")"
fi
grep -q "is not on the registry yet" "${work}/unpublished.log" \
  || fail "unpublished: it did not say which names:
$(cat "${work}/unpublished.log")"
grep -q "bootstrap-publish.sh" "${work}/unpublished.log" \
  || fail "unpublished: it did not name the way out:
$(cat "${work}/unpublished.log")"
# All of them, not the first one: `set -eu` used to stop on the first.
missing="$(grep -c 'is not on the registry yet' "${work}/unpublished.log")"
[ "$missing" = "$names" ] \
  || fail "unpublished: reported ${missing} of ${names}, so it stopped early:
$(cat "${work}/unpublished.log")"
pass "every unpublished name is reported, and the bootstrap is named"

# 6. An npm without the subcommand stops, binds nothing, and says what to do.
if run ancient; then
  fail "ancient: the script should have stopped:
$(cat "${work}/ancient.log")"
fi
grep -q '^trust-npm: bound ' "${work}/ancient.log" && fail "ancient: it bound something"
grep -q 'npm install -g npm@' "${work}/ancient.log" \
  || fail "ancient: no upgrade instruction:
$(cat "${work}/ancient.log")"
pass "an npm without the subcommand stops and names the upgrade"

echo "test-trust-npm: ok"
