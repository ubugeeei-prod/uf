#!/bin/sh
# Install the emulators the deploy matrix runs, pinned, and say where they are.
#
#   tools/deploy-matrix/install-tools.sh <directory> wrangler|kumo|lambda-image...
#
# Each tool is installed at one version and checked against a digest, so a
# column of the matrix is never quietly checked against a newer emulator than
# the documentation names:
#
#   wrangler      `npm ci` of `emulators/package-lock.json` — Wrangler 4.128.0
#                 and workerd with their whole tree, by integrity hash
#   kumo          the Kumo release tarball for linux/amd64, by SHA-256
#   lambda-image  the AWS Lambda Node 24 image, by digest (`docker pull`)
#
# The variables `run.mjs` reads (`WRANGLER_BIN`, `KUMO_BIN`) are printed, and
# appended to `$GITHUB_ENV` when that is set. Nothing here needs a credential.
set -eu

if [ "$#" -lt 2 ]; then
  echo "usage: tools/deploy-matrix/install-tools.sh <directory> wrangler|kumo|lambda-image..." >&2
  exit 2
fi

here="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)"
tools="$1"
shift
mkdir -p "$tools"
tools="$(CDPATH='' cd -- "$tools" && pwd)"

kumo_version="0.30.0"
kumo_sha256="3f7194c4ee12fda95eb8f60c06f2c38d6014d730dc4dfdb584012a5c546861ef"
# Kept in step with `LAMBDA_IMAGE` in `lib/hosts.mjs`, which is what runs it.
lambda_image="public.ecr.aws/lambda/nodejs:24@sha256:486431faaa8a45cc6b23c06ce5281670ed98faa7a45f06411121c33b7247df49"

export_variable() {
  echo "$1=$2"
  if [ -n "${GITHUB_ENV:-}" ]; then
    echo "$1=$2" >> "$GITHUB_ENV"
  fi
}

for tool in "$@"; do
  case "$tool" in
    wrangler)
      mkdir -p "$tools/wrangler"
      cp "$here/emulators/package.json" "$here/emulators/package-lock.json" "$tools/wrangler/"
      (cd "$tools/wrangler" && npm ci --no-audit --no-fund >/dev/null)
      "$tools/wrangler/node_modules/.bin/wrangler" --version
      export_variable WRANGLER_BIN "$tools/wrangler/node_modules/.bin/wrangler"
      ;;
    kumo)
      archive="$tools/kumo_${kumo_version}_linux_amd64.tar.gz"
      if [ ! -f "$archive" ]; then
        curl --fail --location --retry 3 --silent --show-error \
          "https://github.com/sivchari/kumo/releases/download/v${kumo_version}/kumo_${kumo_version}_linux_amd64.tar.gz" \
          --output "$archive"
      fi
      echo "$kumo_sha256  $archive" | sha256sum -c -
      mkdir -p "$tools/kumo"
      tar -xzf "$archive" -C "$tools/kumo" kumo
      export_variable KUMO_BIN "$tools/kumo/kumo"
      ;;
    lambda-image)
      docker pull --quiet "$lambda_image"
      ;;
    *)
      echo "install-tools: no tool named $tool" >&2
      exit 2
      ;;
  esac
done
