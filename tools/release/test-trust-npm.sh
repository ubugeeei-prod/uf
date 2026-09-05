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
[ "${1:-}" = "github" ] || { echo "npm error Unknown subcommand: ${1:-}" >&2; exit 1; }
shift
if [ "${1:-}" = "--help" ]; then
  case "$flavour" in
    ancient) echo "npm trust github [package]" ;;
    strict)  echo "  --file (required)"; echo "  --repository|--repo"
             echo "  --allow-publish"; echo "  --allow-stage-publish" ;;
    *)       echo "  --file (required)"; echo "  --repository|--repo" ;;
  esac
  exit 0
fi
[ "${1:-}" = "list" ] && exit 1
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
  bound="$(grep -c '^bound ' "${work}/${flavour}.log" || true)"
  [ "$bound" = "$names" ] \
    || fail "${flavour}: bound ${bound} of ${names}:
$(cat "${work}/${flavour}.log")"
  pass "${flavour} npm binds all ${names} names"
done

# 3. An npm without the subcommand stops, binds nothing, and says what to do.
if run ancient; then
  fail "ancient: the script should have stopped:
$(cat "${work}/ancient.log")"
fi
grep -q '^bound ' "${work}/ancient.log" && fail "ancient: it bound something"
grep -q 'npm install -g npm@' "${work}/ancient.log" \
  || fail "ancient: no upgrade instruction:
$(cat "${work}/ancient.log")"
pass "an npm without the subcommand stops and names the upgrade"

echo "test-trust-npm: ok"
