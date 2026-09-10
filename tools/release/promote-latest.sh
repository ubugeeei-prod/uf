#!/usr/bin/env sh
# Point `latest` at the newest release, while there is no stable one.
#
#   tools/release/promote-latest.sh --check   # report only; no npm session
#   npm login
#   tools/release/promote-latest.sh           # show the plan, then move them
#   tools/release/promote-latest.sh --yes     # without the question
#   tools/release/promote-latest.sh --otp=123456   # for a run with no terminal
#
# `publish.yml` publishes a prerelease on the `alpha` tag and never on
# `latest`, which is the right rule and stays: a prerelease must not displace a
# stable release. These packages have no stable release, so nothing has moved
# `latest` since the day of the first publish — with `uf@0.0.0-alpha.7` out,
# `latest` was stuck on older alpha releases across the publish closure, so
# `npm install @uniflowed/react` gave a person releases of drift and a package
# set that did not agree with itself. See ubugeeei-prod/uf#408.
#
# ## Why this is a person's step and not part of the publish job
#
# The publish job authenticates with the OIDC id-token GitHub mints for it, and
# that token cannot do this. Three things say so, and they agree:
#
#   * npm's own documentation: "OIDC authentication supports the `npm publish`
#     and `npm stage publish` commands ... Other npm commands such as
#     `install`, `view`, or `access` still require traditional authentication
#     methods."
#   * the npm CLI: `lib/utils/oidc.js` is required from exactly one place,
#     `lib/commands/publish.js`. `npm dist-tag` never exchanges an id-token, so
#     in a job with no `_authToken` it is an unauthenticated request.
#   * the exchange itself is per package —
#     `POST /-/npm/v1/oidc/token/exchange/package/<name>` — and the token it
#     returns is set on that one command's config in memory. It is not written
#     to an `.npmrc` and does not survive to the next `npm` invocation.
#
# And `npm dist-tag add` is a `PUT /-/package/<name>/dist-tags/<tag>` wrapped
# in `otplease`, so on a 2FA account it asks for a one-time password — the same
# reason `tools/release/trust-npm.sh` is run by hand. Putting an npm token in
# this repository's secrets to avoid that would give away the property the
# whole publish job is built around: there is no npm token in this repository,
# in its secrets, or on anyone's machine.
#
# So this runs after the release, from a session that is a person. Nothing
# about a release depends on it having run: the versions are published, the
# `alpha` tag names them, and `latest` is a pointer that can be moved at any
# time. `tools/release/preflight.sh` runs `--check` before the next tag, so a
# release that skipped it is caught before the drift can grow.
#
# ## What it moves, and what it refuses to
#
# Per name, read from the registry rather than passed in:
#
#   * a name with any stable (non-prerelease) version published is left alone,
#     whatever `latest` says. That is the rule `publish.yml` encodes and this
#     script must not undo — and it is how this step retires itself, because
#     from the first stable release the publish job's own `tag=latest` moves
#     `latest` and there is nothing here to do.
#   * a name whose `latest` is already the newest published version is left
#     alone.
#   * anything else is moved to that name's newest published version.
#
# The version is read from the registry per name rather than taken as an
# argument, so a release that half-published cannot be made worse: `latest`
# only ever moves to a version that is actually there, and re-running after the
# rest of the names go out finishes the job. Nothing is sent until the whole
# plan has been printed, so a name that cannot be read stops this before any
# tag has moved.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
cd "$repo_root"

check_only=false
assume_yes=false
# One code for the whole run.
#
# `npm dist-tag add` on a 2FA account asks for a one-time password per write.
# Interactively npm handles that itself — it keeps the terminal and either
# prompts or opens a browser — so this is for the case where it cannot: a run
# with no terminal, and the `--otp` npm's own docs give for exactly that.
# `UF_NPM_OTP` is the same value from the environment, so a code never has to
# be a shell argument, where it would land in a history file.
otp="${UF_NPM_OTP:-}"
for argument in "$@"; do
  case "$argument" in
    --check) check_only=true ;;
    -y | --yes) assume_yes=true ;;
    --otp=*) otp="${argument#--otp=}" ;;
    *) echo "promote-latest: unknown option: $argument" >&2; exit 2 ;;
  esac
done

command -v node >/dev/null 2>&1 || { echo "promote-latest: missing node" >&2; exit 1; }

list="tools/release/published-packages.txt"
work="$(mktemp -d "${TMPDIR:-/tmp}/uf-promote-latest.XXXXXX")"
trap 'rm -rf "$work"' EXIT INT TERM

# What `latest` points at, what the newest published version is, and whether
# any stable one exists — three fields, tab separated.
#
# The ordering is semver's and not the shell's, because the shell's is
# lexicographic and would call `0.0.0-alpha.9` newer than `0.0.0-alpha.10`.
# That is not a hypothetical corner: this project is on alpha.7 and counting,
# and the first release past nine would have moved every `latest` backwards.
summarise() {
  node -e '
const versions = (packument) => Object.keys(packument.versions || {});

const parse = (raw) => {
  const core = raw.split("+")[0];
  const dash = core.indexOf("-");
  const numbers = (dash === -1 ? core : core.slice(0, dash)).split(".");
  return {
    numbers: numbers.map((part) => Number(part)),
    pre: dash === -1 ? null : core.slice(dash + 1).split("."),
  };
};

const compare = (left, right) => {
  const a = parse(left);
  const b = parse(right);
  for (let index = 0; index < 3; index += 1) {
    const one = a.numbers[index] || 0;
    const other = b.numbers[index] || 0;
    if (one !== other) return one - other;
  }
  // A release outranks any prerelease of the same numbers, and that is the
  // whole reason this script asks whether a stable version exists.
  if (a.pre === null && b.pre === null) return 0;
  if (a.pre === null) return 1;
  if (b.pre === null) return -1;
  for (let index = 0; index < Math.max(a.pre.length, b.pre.length); index += 1) {
    const one = a.pre[index];
    const other = b.pre[index];
    if (one === undefined) return -1;
    if (other === undefined) return 1;
    const oneIsNumber = /^[0-9]+$/.test(one);
    const otherIsNumber = /^[0-9]+$/.test(other);
    if (oneIsNumber && otherIsNumber) {
      if (Number(one) !== Number(other)) return Number(one) - Number(other);
    } else if (oneIsNumber !== otherIsNumber) {
      return oneIsNumber ? -1 : 1;
    } else if (one !== other) {
      return one < other ? -1 : 1;
    }
  }
  return 0;
};

const packument = JSON.parse(require("node:fs").readFileSync(process.argv[1], "utf8"));
const published = versions(packument);
if (published.length === 0) {
  console.error("no versions");
  process.exit(1);
}
const newest = published.reduce((best, one) => (compare(one, best) > 0 ? one : best));
const stable = published.some((one) => !one.includes("-"));
const latest = (packument["dist-tags"] || {}).latest || "-";
process.stdout.write([latest, newest, stable ? "stable" : "prerelease"].join("\t") + "\n");
' "$1"
}

echo "promote-latest: reading the dist-tags of every name in $list"

plan="$work/plan"
: >"$plan"
absent=0
held=0
settled=0

for package in $(grep -vE '^[[:space:]]*(#|$)' "$list"); do
  name="@uniflowed/${package}"

  # `-f` so a 404 is a failure rather than a body that does not parse. A name
  # the registry does not have is reported and skipped: `latest` cannot point
  # at a version nobody published, and `preflight.sh` is what fails on it.
  if ! curl -fsS --connect-timeout 10 --max-time 30 \
    "https://registry.npmjs.org/@uniflowed%2f${package}" >"$work/packument.json" 2>/dev/null; then
    printf '  not on npm  %s\n' "$name"
    absent=$((absent + 1))
    continue
  fi

  if ! summary="$(summarise "$work/packument.json")"; then
    echo "promote-latest: could not read the registry's answer for ${name}" >&2
    exit 1
  fi
  latest="$(printf '%s' "$summary" | cut -f1)"
  newest="$(printf '%s' "$summary" | cut -f2)"
  stability="$(printf '%s' "$summary" | cut -f3)"

  if [ "$stability" = stable ]; then
    printf '  released    %-26s latest=%s (a stable release; left alone)\n' "$name" "$latest"
    held=$((held + 1))
    continue
  fi
  if [ "$latest" = "$newest" ]; then
    printf '  current     %-26s latest=%s\n' "$name" "$latest"
    settled=$((settled + 1))
    continue
  fi
  printf '  behind      %-26s latest=%s, newest=%s\n' "$name" "$latest" "$newest"
  printf '%s %s\n' "$name" "$newest" >>"$plan"
done

behind="$(wc -l <"$plan" | tr -d ' ')"

printf '\npromote-latest: %s behind, %s current, %s with a stable release, %s not on npm\n' \
  "$behind" "$settled" "$held" "$absent"

if [ "$behind" -eq 0 ]; then
  echo "promote-latest: nothing to move."
  exit 0
fi

if [ "$check_only" = true ]; then
  cat >&2 <<MESSAGE

'latest' is behind on ${behind} name(s), so 'npm install <name>' does not give
the newest release. The publish workflow cannot fix this — its OIDC token can
publish and nothing else — so it is one command on a machine you are logged in
on:

  npm login
  tools/release/promote-latest.sh

It is idempotent, and it leaves alone any name that has a stable release.
MESSAGE
  exit 1
fi

command -v npm >/dev/null 2>&1 || { echo "promote-latest: missing npm" >&2; exit 1; }
who="$(npm whoami 2>/dev/null || true)"
if [ -z "$who" ]; then
  echo "promote-latest: not logged in. Run 'npm login' first." >&2
  exit 1
fi

cat <<MESSAGE

Moving 'latest' on ${behind} name(s), as ${who}. Each moves to that name's own
newest published version, printed above.

This changes nothing that is published. A dist-tag is a pointer: every version
stays exactly where it is, and this can be run again at any time.
MESSAGE

if [ "$assume_yes" != true ]; then
  printf 'Move them? [y/N] '
  read -r answer
  case "$answer" in
    y | Y | yes | YES) ;;
    *) echo "promote-latest: nothing was moved."; exit 1 ;;
  esac
fi

# On file descriptor 3, not on stdin.
#
# `done <"$plan"` redirects stdin for the *whole loop*, so every `npm` in it
# inherits the plan file instead of the terminal — and `npm dist-tag add` on a
# 2FA account needs the terminal: it asks for a one-time password, or prints a
# URL and waits for the browser. With the file on stdin it can do neither and
# fails immediately with `EOTP`, on the first name, having moved nothing. That
# is what happened on `uf@0.0.0-alpha.12`.
moved=0
failed=0
failures=""
while read -r name version <&3; do
  echo "promote-latest: ${name}@${version} -> latest"
  # Not `set -e`'s business. Many writes can share one authentication between
  # them: a code that expires mid-run must not throw away the writes that
  # worked, or the report of which they were. The command is idempotent and
  # the plan is read from the registry, so the recovery is to run it again —
  # and the names already moved will not be in the next plan.
  if npm dist-tag add "${name}@${version}" latest ${otp:+--otp="$otp"}; then
    moved=$((moved + 1))
  else
    failed=$((failed + 1))
    failures="${failures}  ${name}@${version}
"
  fi
done 3<"$plan"

printf '\npromote-latest: %s moved.\n' "$moved"
if [ "$failed" -gt 0 ]; then
  printf 'promote-latest: %s did not move:\n%s' "$failed" "$failures" >&2
  printf '\nRun it again — it reads the plan from the registry, so the names\n'  >&2
  printf 'that moved are not in the next one. If npm asked for a one-time\n'      >&2
  printf 'password, `--otp=<code>` passes one to every write in the run.\n'       >&2
  exit 1
fi
