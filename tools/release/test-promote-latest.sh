#!/bin/sh
# `promote-latest.sh` against registries that answer differently, without
# touching npm.
#
# The script moves a dist-tag on seventeen published packages, by hand, once
# per release, and every mistake in it is visible to everyone who types
# `npm install @uniflowed/…` afterwards. Four of the ways it could be wrong
# cannot be seen by running it against the registry as it is today:
#
#   * moving `latest` onto a prerelease when a stable release exists. Today no
#     `@uniflowed/*` package has one, so the guard that must never fire also
#     never runs;
#   * ordering versions as strings, which puts `0.0.0-alpha.9` after
#     `0.0.0-alpha.10` and would move every `latest` backwards on the first
#     release past nine — the release after next;
#   * skipping a name that has no `latest` at all, which is what
#     `bootstrap-publish.sh` leaves a brand new name in and what every
#     currently published name has grown out of;
#   * reading a 404 body as a packument, which reports a name that is not on
#     npm as one whose `latest` is fine.
#
# So the registry is stubbed. The stubs answer the way npm's does — `curl -f`
# exits non-zero on a missing name, `npm dist-tag add` refuses a spec with no
# version in it — and the npm stub records every call, so a run that should
# touch nothing can be checked for having touched nothing.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
cd "$repo_root"

script="tools/release/promote-latest.sh"
work="$(mktemp -d "${TMPDIR:-/tmp}/uf-test-promote-latest.XXXXXX")"
trap 'rm -rf "$work"' EXIT INT TERM

names="$(grep -cvE '^[[:space:]]*(#|$)' tools/release/published-packages.txt)"

fail() {
  echo "test-promote-latest: FAIL: $*" >&2
  exit 1
}

pass() {
  echo "  ok  $*"
}

# A registry that answers like the registry: one packument per name, and a
# missing name is an HTTP error that `-f` turns into a non-zero exit with an
# empty body — not a body that happens not to parse.
#
# `$1` names the world:
#   behind    every name published up to alpha.7, `latest` still on alpha.1
#   current   every name published up to alpha.7, `latest` on alpha.7
#   released  a stable 1.0.0 on `latest`, with a newer release candidate out
#   ordering  alpha.9 and alpha.10, `latest` on alpha.9
#   untagged  published on the prerelease tag only, with no `latest` at all
#   mixed     `behind`, except that `@uniflowed/host` is not on the registry
#   absent    nothing is on the registry
#   garbage   an HTML error page with a 200 on it
make_registry() {
  world="$1"
  mkdir -p "${work}/${world}"
  cat > "${work}/${world}/curl" <<EOF
#!/bin/sh
world="${world}"
EOF
  cat >> "${work}/${world}/curl" <<'EOF'
# The script has to ask for a failure on an HTTP error. Without `-f` curl
# writes the registry's 404 body to stdout and exits 0, and the caller reads
# an error page as a packument.
fails_on_http_error=""
url=""
for argument in "$@"; do
  case "$argument" in
    https://*) url="$argument" ;;
    -*f*) fails_on_http_error=1 ;;
    -*) ;;
    *) ;;
  esac
done
[ -n "$fails_on_http_error" ] || { echo "curl: this stub needs -f" >&2; exit 2; }
[ -n "$url" ] || { echo "curl: no URL" >&2; exit 2; }
name="${url##*/}"

versions='"0.0.0-alpha.1":{},"0.0.0-alpha.4":{},"0.0.0-alpha.5":{},"0.0.0-alpha.6":{},"0.0.0-alpha.7":{}'

case "$world" in
  absent) exit 22 ;;
  mixed)
    # `-f` on a name the registry does not have: exit 22, and nothing on stdout.
    [ "$name" = "@uniflowed%2fhost" ] && exit 22
    printf '{"dist-tags":{"latest":"0.0.0-alpha.1","alpha":"0.0.0-alpha.7"},"versions":{%s}}\n' "$versions" ;;
  behind)
    printf '{"dist-tags":{"latest":"0.0.0-alpha.1","alpha":"0.0.0-alpha.7"},"versions":{%s}}\n' "$versions" ;;
  current)
    printf '{"dist-tags":{"latest":"0.0.0-alpha.7","alpha":"0.0.0-alpha.7"},"versions":{%s}}\n' "$versions" ;;
  released)
    printf '{"dist-tags":{"latest":"1.0.0","rc":"1.1.0-rc.1"},"versions":{"0.0.0-alpha.7":{},"1.0.0":{},"1.1.0-rc.1":{}}}\n' ;;
  ordering)
    printf '{"dist-tags":{"latest":"0.0.0-alpha.9"},"versions":{"0.0.0-alpha.9":{},"0.0.0-alpha.10":{}}}\n' ;;
  untagged)
    printf '{"dist-tags":{"alpha":"0.0.0-alpha.7"},"versions":{"0.0.0-alpha.7":{}}}\n' ;;
  garbage)
    printf '<html><head><title>502</title></head></html>\n' ;;
esac
EOF
  chmod +x "${work}/${world}/curl"

  # An npm that parses `dist-tag add` the way npm does, and writes down every
  # call it is given. `NPM_REFUSES` is the registry saying no to the move.
  cat > "${work}/${world}/npm" <<'EOF'
#!/bin/sh
echo "npm $*" >> "$NPM_LOG"
case "$1" in
  whoami) echo "tester"; exit 0 ;;
  dist-tag) ;;
  *) echo "npm error this stub only answers whoami and dist-tag" >&2; exit 1 ;;
esac
[ "${2:-}" = add ] || { echo "npm error Unknown subcommand: ${2:-}" >&2; exit 1; }
spec="${3:-}"
name="${spec%@*}"
version="${spec##*@}"
if [ -z "$name" ] || [ "$name" = "$spec" ] || [ -z "$version" ]; then
  echo "npm error must provide a spec with a name and version, and a tag to add" >&2
  exit 1
fi
[ "${4:-}" = latest ] || { echo "npm error the tag to add is missing: ${4:-}" >&2; exit 1; }
[ -n "${NPM_REFUSES:-}" ] && { echo "npm error code E403" >&2; exit 1; }
echo "+latest: ${spec}"
EOF
  chmod +x "${work}/${world}/npm"
}

# Run the script in one world. `$1` is the world, `$2` the log name, the rest
# are the script's own arguments.
run() {
  world="$1"
  label="$2"
  shift 2
  make_registry "$world"
  : > "${work}/${label}.npm"
  NPM_LOG="${work}/${label}.npm" PATH="${work}/${world}:$PATH" \
    sh "$script" "$@" > "${work}/${label}.log" 2>&1
}

# `grep -c` prints 0 and exits 1 on an empty file, so the count is kept and the
# status is dropped — `|| echo 0` would print a second line and every
# comparison against it would fail.
moves() {
  grep -c '^npm dist-tag add ' "${work}/$1.npm" 2>/dev/null || true
}

# 1. The state the registry is in today: `latest` behind on every name.
#    `--check` says so and fails, so the pre-tag check cannot pass over it.
if run behind check-behind --check; then
  fail "check: drift on every name should be a non-zero exit:
$(cat "${work}/check-behind.log")"
fi
reported="$(grep -c '^  behind ' "${work}/check-behind.log")"
[ "$reported" = "$names" ] \
  || fail "check: reported ${reported} of ${names} names behind:
$(cat "${work}/check-behind.log")"
grep -q 'tools/release/promote-latest.sh' "${work}/check-behind.log" \
  || fail "check: it did not name the way out:
$(cat "${work}/check-behind.log")"
pass "--check fails on the drift and names the fix"

# 2. And it needs no npm session, which is what lets `preflight.sh` run it and
#    what lets it run in a job that has no token.
[ "$(moves check-behind)" = 0 ] \
  || fail "check: it called npm:
$(cat "${work}/check-behind.npm")"
grep -q '^npm ' "${work}/check-behind.npm" \
  && fail "check: it called npm at all:
$(cat "${work}/check-behind.npm")"
pass "--check touches npm not at all"

# 3. The move itself: every name, to that name's own newest published version,
#    on `latest` and on no other tag.
run behind move --yes || fail "move: the script failed:
$(cat "${work}/move.log")"
moved="$(moves move)"
[ "$moved" = "$names" ] \
  || fail "move: moved ${moved} of ${names}:
$(cat "${work}/move.log")"
grep -q 'npm dist-tag add @uniflowed/react@0.0.0-alpha.7 latest' "${work}/move.npm" \
  || fail "move: @uniflowed/react was not moved to the newest version:
$(cat "${work}/move.npm")"
grep -v ' latest$' "${work}/move.npm" | grep -q 'dist-tag' \
  && fail "move: something was written to a tag other than latest:
$(cat "${work}/move.npm")"
pass "every name behind is moved to its own newest version, on latest"

# 4. A second run has nothing to do. The tag is a pointer and the script is
#    run by hand, so it has to be safe to run twice.
run current settled --yes || fail "settled: the script failed:
$(cat "${work}/settled.log")"
[ "$(moves settled)" = 0 ] \
  || fail "settled: it moved a tag that was already right:
$(cat "${work}/settled.npm")"
grep -q 'nothing to move' "${work}/settled.log" \
  || fail "settled: it did not say there was nothing to do:
$(cat "${work}/settled.log")"
run current check-settled --check \
  || fail "settled: --check should pass when every latest is the newest:
$(cat "${work}/check-settled.log")"
pass "a registry that is already right is left alone, twice"

# 5. The guard that must never fire today and must fire from the first stable
#    release: a package with a stable version keeps its `latest`, even when a
#    newer prerelease exists. `publish.yml` publishes a prerelease on the
#    prerelease tag precisely so it cannot displace a release, and this script
#    exists only because these packages have no release to protect.
run released released --yes || fail "released: the script failed:
$(cat "${work}/released.log")"
[ "$(moves released)" = 0 ] \
  || fail "released: it moved latest onto a prerelease over a stable release:
$(cat "${work}/released.npm")"
grep -q 'a stable release; left alone' "${work}/released.log" \
  || fail "released: it did not say why it left them alone:
$(cat "${work}/released.log")"
run released check-released --check \
  || fail "released: --check should pass when a stable release holds latest:
$(cat "${work}/check-released.log")"
pass "a name with a stable release keeps its latest"

# 6. Versions are ordered as versions. Sorted as strings, `0.0.0-alpha.9` is
#    the newest of the two and the script would report nothing to do — which
#    is the release after next.
if run ordering ordering --check; then
  fail "ordering: alpha.10 is newer than alpha.9:
$(cat "${work}/ordering.log")"
fi
grep -q 'newest=0.0.0-alpha.10' "${work}/ordering.log" \
  || fail "ordering: the newest version was not alpha.10:
$(cat "${work}/ordering.log")"
pass "alpha.10 is newer than alpha.9"

# 7. A name that has only ever been published on the prerelease tag has no
#    `latest` at all, and `npm install <name>` fails outright rather than
#    resolving to something old. That is the state
#    `tools/release/bootstrap-publish.sh` leaves a brand new name in, so it is
#    the state this has to move rather than skip.
run untagged untagged --yes || fail "untagged: the script failed:
$(cat "${work}/untagged.log")"
grep -q 'latest=-, newest=0.0.0-alpha.7' "${work}/untagged.log" \
  || fail "untagged: a missing latest was not reported as missing:
$(cat "${work}/untagged.log")"
moved="$(moves untagged)"
[ "$moved" = "$names" ] \
  || fail "untagged: moved ${moved} of ${names}:
$(cat "${work}/untagged.log")"
pass "a name with no latest at all gets one"

# 8. A name the registry does not have is reported and skipped, and the names
#    beside it are still moved. A half-sent release is exactly when this is
#    run, and stopping on the first missing name would leave the rest behind.
run mixed mixed --yes || fail "mixed: an absent name should not stop the run:
$(cat "${work}/mixed.log")"
grep -q '^  not on npm  @uniflowed/host$' "${work}/mixed.log" \
  || fail "mixed: the absent name was not reported:
$(cat "${work}/mixed.log")"
moved="$(moves mixed)"
[ "$moved" = "$((names - 1))" ] \
  || fail "mixed: moved ${moved}, expected $((names - 1)):
$(cat "${work}/mixed.log")"
grep -q 'dist-tag add @uniflowed/host' "${work}/mixed.npm" \
  && fail "mixed: it moved a tag on a name that is not published:
$(cat "${work}/mixed.npm")"
pass "a name that is not on the registry is skipped, and the rest still move"

# 9. Every name absent is not an error either — there is nothing to move — but
#    npm must not be called for any of them.
run absent absent --check || fail "absent: nothing to move is not a failure:
$(cat "${work}/absent.log")"
[ "$(moves absent)" = 0 ] || fail "absent: it called npm:
$(cat "${work}/absent.npm")"
pass "a registry with none of the names is reported, not moved"

# 10. An answer that is not a packument stops the run. Reading it as one would
#    report a `latest` nobody set.
if run garbage garbage --yes; then
  fail "garbage: an unparseable answer should stop the run:
$(cat "${work}/garbage.log")"
fi
[ "$(moves garbage)" = 0 ] || fail "garbage: it moved something anyway:
$(cat "${work}/garbage.npm")"
pass "an answer that is not a packument stops the run before anything moves"

# 11. npm refusing the move is a failure, not a run that reports success. And
#     the plan is printed before the first move, so a refusal halfway names
#     what was left.
make_registry behind
: > "${work}/refused.npm"
if NPM_LOG="${work}/refused.npm" NPM_REFUSES=1 PATH="${work}/behind:$PATH" \
  sh "$script" --yes > "${work}/refused.log" 2>&1; then
  fail "refused: npm said no and the script reported success:
$(cat "${work}/refused.log")"
fi
grep -q '^  behind ' "${work}/refused.log" \
  || fail "refused: the plan was not printed before the move was attempted:
$(cat "${work}/refused.log")"
pass "npm refusing a move fails the run"

# 12. An unknown option is a usage error rather than a run that quietly does
#     something else. `--check` and `--yes` differ by whether anything is
#     written, so a typo must not fall through to the writing one.
if run behind usage --dry-run; then
  fail "usage: an unknown option should not run:
$(cat "${work}/usage.log")"
fi
[ "$(moves usage)" = 0 ] || fail "usage: it called npm:
$(cat "${work}/usage.npm")"
pass "an unknown option stops before anything is read or written"

echo "test-promote-latest: ok"
