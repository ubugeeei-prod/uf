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
# `npm trust` configures a name that has never been published, which is what
# makes this the whole bootstrap — the first release goes out over OIDC like
# every one after it, and nobody has to publish by hand to create the package
# first. (The npm documentation still describes the older flow, where the
# package had to exist and the binding was a web form; `npm trust` landed in
# npm 11 and supersedes it.)
#
# Idempotent: a package that is already bound is reported and left alone.
set -eu

repo_root="$(CDPATH= cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"

repository="${UF_TRUST_REPOSITORY:-ubugeeei-prod/uf}"
workflow="${UF_TRUST_WORKFLOW:-publish.yml}"

command -v npm >/dev/null 2>&1 || { echo "trust-npm: missing npm" >&2; exit 1; }
case "$(npm --version)" in
  1[1-9].* | [2-9][0-9].*) ;;
  *) echo "trust-npm: npm 11 or newer is required for \`npm trust\`" >&2; exit 1 ;;
esac

who="$(npm whoami 2>/dev/null || true)"
if [ -z "$who" ]; then
  echo "trust-npm: not logged in. Run 'npm login' first." >&2
  exit 1
fi
echo "trust-npm: configuring as ${who}, for ${repository} (${workflow})"

for package in $(grep -vE '^[[:space:]]*(#|$)' tools/release/published-packages.txt); do
  name="@uniflowed/${package}"
  if npm trust list "$name" 2>/dev/null | grep -q "file: ${workflow}"; then
    echo "trust-npm: ${name} is already bound to ${workflow}"
    continue
  fi
  echo "trust-npm: binding ${name}"
  npm trust github "$name" \
    --file "$workflow" \
    --repository "$repository" \
    --allow-publish \
    --yes
done

echo
echo "Done. A 'uf@*' tag now publishes these over OIDC, with no token anywhere."
