#!/usr/bin/env sh
# The one-time publish that trusted publishing cannot do for itself.
#
# npm will only let a trusted publisher be configured on a package that already
# exists, and that configuration is a web-UI step with no API. So the first
# version of each package has to be published by a person, from a logged-in
# machine, answering a 2FA prompt. Every publish after this one is
# `.github/workflows/publish.yml`, over OIDC, with no token anywhere.
#
#   npm login
#   tools/release/bootstrap-npm.sh
#
# It publishes each package in tools/release/published-packages.txt at the
# version the workspace is on, skips any version already on the registry, and
# prints the settings URL to enable trusted publishing on each.
set -eu

repo_root="$(CDPATH= cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"

need() {
  command -v "$1" >/dev/null 2>&1 || { echo "bootstrap-npm: missing $1" >&2; exit 1; }
}
need npm
need node

who="$(npm whoami 2>/dev/null || true)"
if [ -z "$who" ]; then
  echo "bootstrap-npm: not logged in. Run 'npm login' first." >&2
  exit 1
fi
echo "bootstrap-npm: publishing as ${who}"

packages="$(grep -vE '^\s*(#|$)' tools/release/published-packages.txt)"

# Refuse to publish a half-set: check every manifest before sending any of it.
for package in $packages; do
  [ -f "packages/${package}/package.json" ] || {
    echo "bootstrap-npm: packages/${package} does not exist" >&2
    exit 1
  }
done

published=""
for package in $packages; do
  directory="packages/${package}"
  name="$(node -p "require('./${directory}/package.json').name")"
  version="$(node -p "require('./${directory}/package.json').version")"

  if npm view "${name}@${version}" version >/dev/null 2>&1; then
    echo "bootstrap-npm: ${name}@${version} is already published"
    published="${published} ${name}"
    continue
  fi

  # A prerelease must not become `latest`, or `npm install <name>` hands an
  # alpha to someone who asked for the stable release.
  tag=latest
  case "$version" in
    *-alpha*) tag=alpha ;;
    *-beta*) tag=beta ;;
    *-rc*) tag=rc ;;
  esac

  echo "bootstrap-npm: publishing ${name}@${version} under the '${tag}' tag"
  # No --otp here on purpose: npm prompts, so the one-time password is typed by
  # the person running this and never passed as an argument or stored.
  ( cd "$directory" && npm publish --access public --tag "$tag" )
  published="${published} ${name}"
done

echo
echo "Now enable trusted publishing on each package, once, on npmjs.com."
echo "Set the publisher to this repository's publish workflow:"
echo
echo "  repository:  ubugeeei-prod/uf"
echo "  workflow:    publish.yml"
echo
for name in $published; do
  echo "  https://www.npmjs.com/package/${name}/access"
done
echo
echo "After that, a 'uf@*' tag publishes every one of them over OIDC."
