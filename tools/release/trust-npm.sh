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
# `npm trust` configures a name that has never been published, which is what
# makes this the whole bootstrap — the first release goes out over OIDC like
# every one after it, and nobody has to publish by hand to create the package
# first. (The npm documentation still describes the older flow, where the
# package had to exist and the binding was a web form; `npm trust` landed in
# npm 11 and supersedes it.)
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

# One dry run before the first real one.
#
# `--dry-run` needs no session and writes nothing; it parses the arguments and
# prints what it would do. It is the only thing that would have caught
# `--allow-publish` — a flag `npm trust github` does not have — which sat in
# this script from the day it was written and would have stopped it on the
# first package, `set -eu`, with the release still blocked and the reason
# reading like an npm outage:
#
#   npm error code EUSAGE
#   npm error Unknown flag: --allow-publish
# Both streams: npm puts some failures on stdout, and a guard that reports
# half of them is a guard that reports warnings and hides the error.
errors="$(mktemp "${TMPDIR:-/tmp}/trust-npm.XXXXXX")"
trap 'rm -f "$errors"' EXIT INT TERM
if ! npm trust github "@uniflowed/${first}" \
  --file "$workflow" \
  --repository "$repository" \
  --yes \
  --dry-run >"$errors" 2>&1; then
  echo "trust-npm: npm rejected the arguments this script passes:" >&2
  grep -v '^npm warn Unknown user config' <"$errors" >&2 || cat "$errors" >&2
  echo >&2
  echo "trust-npm: npm --version ${npm_version}" >&2
  exit 1
fi

for package in $(grep -vE '^[[:space:]]*(#|$)' tools/release/published-packages.txt); do
  name="@uniflowed/${package}"
  # `npm trust list` needs the same session *and* a one-time password, so on a
  # session that has not been through 2FA it returns nothing and every name is
  # attempted. Attempting one that is already bound is not an error — npm says
  # so and moves on — so this is a shortcut, not a guard.
  if npm trust list "$name" 2>/dev/null | grep -q "file: ${workflow}"; then
    echo "trust-npm: ${name} is already bound to ${workflow}"
    continue
  fi
  echo "trust-npm: binding ${name}"
  npm trust github "$name" \
    --file "$workflow" \
    --repository "$repository" \
    --yes
done

echo
echo "Done. A 'uf@*' tag now publishes these over OIDC, with no token anywhere."
echo
echo "Run this again after adding a name to published-packages.txt. The publish"
echo "job cannot check the bindings itself — it authenticates as a workflow and"
echo "'npm trust list' reads them as you — so an unbound name fails mid-release."
