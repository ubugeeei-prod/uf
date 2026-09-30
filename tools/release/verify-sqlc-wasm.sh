#!/bin/sh
# Read the published WASM asset back, verify its identity and run pinned sqlc.
set -eu
version=${1:?Usage: verify-sqlc-wasm.sh VERSION}
repo=${GITHUB_REPOSITORY:-ubugeeei-prod/uf}
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT HUP INT TERM
gh release download "uf@$version" --repo "$repo" --pattern 'sqlc-gen-flow.wasm*' --dir "$scratch"
(cd "$scratch" && sha256sum -c sqlc-gen-flow.wasm.sha256)
cosign verify-blob --bundle "$scratch/sqlc-gen-flow.wasm.sigstore" \
  --certificate-identity "https://github.com/$repo/.github/workflows/release.yml@refs/heads/main" \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  "$scratch/sqlc-gen-flow.wasm"
sh tools/ci/sqlc-wasm.sh "$scratch/sqlc-gen-flow.wasm"
