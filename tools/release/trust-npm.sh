#!/usr/bin/env sh
# Let this repository's publish workflow publish each package, over OIDC.
#
# npm's trusted publishing binds a package name to one workflow in one
# repository. Nothing else can publish it, and no token exists to leak: the
# workflow proves who it is with the OIDC id-token GitHub mints for it.
#
#   npm login
#   tools/release/trust-npm.sh
#
# Binding a name is an account change, so npm asks for a one-time password.
# Have the second factor to hand: this is interactive, once per run, and it is
# why no workflow and no agent can do it.
#
# `npm trust` binds a name the registry already has. It cannot create one:
#
#   npm error code E404
#   npm error 404 Not Found - POST .../@uniflowed%2fcore/trust
#
# so a name that has never been published has to be published once before it
# can be bound. That is what `tools/release/bootstrap-publish.sh` is for, and
# this script says which names need it rather than stopping on the first.
#
# Safe to re-run. A name that is already bound is skipped when `npm trust list`
# can read it, and re-bound harmlessly when it cannot — which is the usual
# case, because that read needs the one-time password too.
set -eu

repo_root="$(CDPATH= cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"

repository="${UF_TRUST_REPOSITORY:-ubugeeei-prod/uf}"
workflow="${UF_TRUST_WORKFLOW:-publish.yml}"

command -v npm >/dev/null 2>&1 || { echo "trust-npm: missing npm" >&2; exit 1; }
npm_version="$(npm --version 2>/dev/null || echo unknown)"

# Ask npm what it can do rather than what number it is.
#
# `npm trust github` and its `--file`/`--repository` arrived partway through
# npm 11, so a major-version gate lets through an npm that has `trust` and not
# the arguments this script passes — and npm does not fail on an argument it
# does not know, it *warns* and carries on with the value as a positional:
#
#   npm warn "publish.yml" is being parsed as a normal command line argument.
#   npm warn Unknown cli config "--file".
#
# which is a run that looks like it worked and bound nothing.
if ! npm trust github --help 2>&1 | grep -q -- '--repository'; then
  cat >&2 <<MESSAGE
trust-npm: this npm does not have \`npm trust github --file --repository\`.

  npm --version   ${npm_version}
  node --version  $(node --version 2>/dev/null || echo unknown)

That subcommand is how a package name is bound to this repository's publish
workflow, and it is the whole of the release bootstrap. Upgrade npm and run
this again:

  npm install -g npm@latest

If that answers EBADENGINE, npm's latest wants a newer node than this one.
The last npm 11 has the subcommand and asks only for node >= 22.9.0:

  npm install -g npm@11.11.0
MESSAGE
  exit 1
fi

who="$(npm whoami 2>/dev/null || true)"
if [ -z "$who" ]; then
  echo "trust-npm: not logged in. Run 'npm login' first." >&2
  exit 1
fi
echo "trust-npm: configuring as ${who}, for ${repository} (${workflow})"

first="$(grep -vE '^[[:space:]]*(#|$)' tools/release/published-packages.txt | head -1)"

# What this npm calls "may publish".
#
# The flag set moves between npm releases and it has moved in both directions:
# the npm this script was written against required `--allow-publish`, npm
# 11.11.0 rejects it as an unknown flag, and a newer one requires it again —
#
#   npm error At least one permission flag is required
#             (--allow-publish, --allow-stage-publish)
#
# so it is asked for rather than assumed. A version number would not answer
# this; the help does.
permission=""
if npm trust github --help 2>&1 | grep -q -- '--allow-publish'; then
  permission="--allow-publish"
fi

# One argument list, used by the dry run and by every real bind, so the check
# and the thing it checks cannot drift.
#
# `shift` then `"$@"`, so a call with no extra arguments passes none. Writing
# it as `"${2:-}"` passes an *empty* argument instead, which npm reads as a
# positional it does not have:
#
#   npm error Unknown positional argument:
#
# and which only the real bind hits, because the dry run always has one.
#
# shellcheck disable=SC2086 # deliberate: $permission is one flag or nothing
trust() {
  package="$1"
  shift
  npm trust github "$package" \
    --file "$workflow" \
    --repository "$repository" \
    $permission \
    --yes \
    "$@"
}

# One dry run before the first real one.
#
# `--dry-run` needs no session and writes nothing; it parses the arguments and
# prints what it would do. It is what catches a flag this npm does not take,
# before `set -eu` stops the script on the first package with the release
# still blocked and the reason reading like an npm outage.
#
# Both streams, because npm puts some failures on stdout and a guard that
# reports half of them reports the warnings and hides the error.
errors="$(mktemp "${TMPDIR:-/tmp}/trust-npm.XXXXXX")"
trap 'rm -f "$errors"' EXIT INT TERM
if ! trust "@uniflowed/${first}" --dry-run >"$errors" 2>&1; then
  echo "trust-npm: npm rejected the arguments this script passes:" >&2
  grep -v '^npm warn Unknown user config' <"$errors" >&2 || cat "$errors" >&2
  echo >&2
  echo "trust-npm: npm --version ${npm_version}, permission flag '${permission:-none}'" >&2
  exit 1
fi

bound=0
existing=0
mismatched=""
unpublished=""

# Ask npm what is there, rather than reading what it printed.
#
# Binding a name is an account change, so npm asks for a one-time password
# and prints a URL to authenticate at. Capturing its output to classify the
# failure hid that prompt, and the run stopped on the first name with
#
#   npm error code EOTP
#   npm error This operation requires a one-time password.
#   npm error Open this URL in your browser to authenticate: …
#
# where the URL was in a file nobody was looking at. So the real binds write
# straight to the terminal, and what happened is worked out afterwards from
# two questions npm can answer directly.
is_bound() {
  npm trust list "$1" 2>/dev/null | grep -q "$workflow"
}
is_published() {
  npm view "$1" name >/dev/null 2>&1
}

# `npm trust list` needs the same session *and* a one-time password, so on a
# session that has not been through 2FA it returns nothing and every name is
# attempted. That is fine: a name that already has a configuration answers
#
#   npm error code E409
#   npm error 409 Conflict - a trusted publisher configuration that a token
#   could also match already exists for this package
#
# which is "already done", not a failure — but it is *not* proof that the
# configuration is this repository's workflow, so the one that says so is
# asked for and checked. `set -eu` would stop the run on the first of these,
# which is what it did.
for package in $(grep -vE '^[[:space:]]*(#|$)' tools/release/published-packages.txt); do
  name="@uniflowed/${package}"

  # Nothing to bind yet. `npm trust` binds a name the registry has; it does
  # not create one, and asking it to would spend an authentication on a 404.
  if ! is_published "$name"; then
    echo "trust-npm: ${name} is not on the registry yet"
    unpublished="${unpublished} ${package}"
    continue
  fi

  if is_bound "$name"; then
    echo "trust-npm: ${name} already points at ${workflow}"
    existing=$((existing + 1))
    continue
  fi

  echo "trust-npm: binding ${name}"
  if trust "$name"; then
    bound=$((bound + 1))
    continue
  fi

  # It said no. What is there now is the answer: a configuration naming this
  # workflow means it was already bound and npm refused the duplicate (E409),
  # and anything else is a real failure.
  if is_bound "$name"; then
    echo "trust-npm: ${name} already points at ${workflow}"
    existing=$((existing + 1))
    continue
  fi
  listing="$(npm trust list "$name" 2>&1 || true)"
  if printf '%s' "$listing" | grep -q 'file:'; then
    echo "trust-npm: ${name} already has a configuration, and it is not ${workflow}:"
    printf '%s\n' "$listing" | sed 's/^/    /'
    mismatched="${mismatched} ${name}"
    existing=$((existing + 1))
    continue
  fi
  echo >&2
  echo "trust-npm: ${name} could not be bound, and npm's own output is above." >&2
  echo "trust-npm: npm --version ${npm_version}, permission flag '${permission:-none}'" >&2
  exit 1
done

echo
echo "trust-npm: ${bound} bound, ${existing} already configured, \
$(printf '%s' "$unpublished" | wc -w | tr -d ' ') not on the registry"
if [ -n "$unpublished" ]; then
  cat >&2 <<MESSAGE

These have never been published, so there is nothing to bind yet:
${unpublished}

\`npm trust\` binds a name the registry has; it does not create one. Publish
them once, then run this again:

  tools/release/bootstrap-publish.sh

MESSAGE
fi
if [ -n "$mismatched" ]; then
  cat >&2 <<MESSAGE

These already had a trusted publisher and it does not name ${workflow}:
${mismatched}

A publish from this repository's workflow will be refused for each of them.
Revoke the old configuration and run this again:

  npm trust revoke <package>
MESSAGE
  exit 1
fi
if [ -n "$unpublished" ]; then
  exit 1
fi

echo
echo "Done. A 'uf@*' tag now publishes these over OIDC, with no token anywhere."
echo
echo "Run this again after adding a name to published-packages.txt. The publish"
echo "job cannot check the bindings itself — it authenticates as a workflow and"
echo "'npm trust list' reads them as you — so an unbound name fails mid-release."
