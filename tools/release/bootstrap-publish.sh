#!/usr/bin/env sh
# Publish, once, the names the registry does not have yet.
#
# `npm trust` binds a name the registry already has. It cannot create one:
#
#   npm error code E404
#   npm error 404 Not Found - POST .../@uniflowed%2fcore/trust
#
# So the first publish of a *new* name cannot go over OIDC, because there is
# nothing to bind the workflow to. It has to come from a person, once, and
# then `tools/release/trust-npm.sh` binds it and every release after that is
# the workflow's. `uf.config.js` calls this `publish.firstPublish.localBootstrap`.
#
#   npm login
#   tools/release/bootstrap-publish.sh
#
# It publishes only what is missing. A name that is already on the registry is
# left alone whatever version it is at — moving an existing package to a new
# version is the release workflow's job, not this one's.
#
# Every publish is shown before anything is sent, and nothing is sent without
# an answer. `--yes` skips the question for a non-interactive run.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
cd "$repo_root"

assume_yes=false
for argument in "$@"; do
  case "$argument" in
    -y | --yes) assume_yes=true ;;
    *) echo "bootstrap-publish: unknown option: $argument" >&2; exit 2 ;;
  esac
done

command -v npm >/dev/null 2>&1 || { echo "bootstrap-publish: missing npm" >&2; exit 1; }
who="$(npm whoami 2>/dev/null || true)"
if [ -z "$who" ]; then
  echo "bootstrap-publish: not logged in. Run 'npm login' first." >&2
  exit 1
fi

list="tools/release/published-packages.txt"
version="$(node -p "require('./packages/core/package.json').version")"

# The dist-tag a prerelease goes to, so `latest` is not moved by one. The same
# rule `publish.yml` applies, because a bootstrap that tagged differently would
# leave the two halves of a release disagreeing.
tag=latest
case "$version" in
  *-alpha*) tag=alpha ;;
  *-beta*) tag=beta ;;
  *-rc*) tag=rc ;;
esac

missing=""
for package in $(grep -vE '^[[:space:]]*(#|$)' "$list"); do
  name="@uniflowed/${package}"
  if npm view "$name" name >/dev/null 2>&1; then
    printf '  on npm      %s\n' "$name"
  else
    printf '  to publish  %s@%s  (--tag %s)\n' "$name" "$version" "$tag"
    missing="${missing} ${package}"
  fi
done

if [ -z "$missing" ]; then
  echo
  echo "bootstrap-publish: every name is on the registry; nothing to do."
  exit 0
fi

count="$(printf '%s' "$missing" | wc -w | tr -d ' ')"
cat <<MESSAGE

${count} package(s) have never been published, as ${who}.

This is the one step that cannot go over OIDC, and it cannot be undone: npm
does not free a name that has been published. Everything after it is the
publish workflow's.
MESSAGE

if [ "$assume_yes" != true ]; then
  printf 'Publish them? [y/N] '
  read -r answer
  case "$answer" in
    y | Y | yes | YES) ;;
    *) echo "bootstrap-publish: nothing was published."; exit 1 ;;
  esac
fi

# A dry run of every one before any real one, so a package that cannot be
# packed stops this before half the names are out.
for package in $missing; do
  ( cd "packages/${package}" && npm publish --access public --tag "$tag" --dry-run >/dev/null ) \
    || { echo "bootstrap-publish: @uniflowed/${package} would not publish" >&2; exit 1; }
done
echo "bootstrap-publish: all ${count} pack cleanly"

published=0
for package in $missing; do
  name="@uniflowed/${package}"
  echo "bootstrap-publish: publishing ${name}@${version}"
  ( cd "packages/${package}" && npm publish --access public --tag "$tag" )
  published=$((published + 1))
done

cat <<MESSAGE

bootstrap-publish: ${published} published.

Now bind them to the workflow, so every release after this one is the
workflow's and no token exists anywhere:

  tools/release/trust-npm.sh
MESSAGE
