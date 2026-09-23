#!/bin/sh
# Install the sqlc `tests/sqlc` is captured with, checked against its sha256.
#
#   sh tools/ci/install-sqlc.sh [DIR]    # default: ./target/sqlc
#
# Prints the binary's path. One pinned release, because the fixtures under
# `tests/sqlc/cases/*/request.*` are what that release sends a plugin, and
# `node tests/sqlc/capture.mjs --check` fails when a different sqlc would send
# something else — which is the signal to move this pin and the fixtures
# together, in one change.
set -eu

VERSION=1.31.1
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) PLATFORM=linux_amd64 SHA256=497ae4fcdfa64c5b0c311ffe4c2bd991e43991e82e5367792ed78bc2dca27354 ;;
  Darwin-arm64) PLATFORM=darwin_arm64 SHA256=21602158c99eb1f2bae197a66abfb1941d1e9e50b23125bb193349c6b1acc71e ;;
  *) echo "install-sqlc: no pinned sqlc for $(uname -s)-$(uname -m)" >&2; exit 2 ;;
esac

DIR=${1:-target/sqlc}
mkdir -p "$DIR"
ARCHIVE="$DIR/sqlc_${VERSION}_${PLATFORM}.tar.gz"
curl -fsSL -o "$ARCHIVE" "https://github.com/sqlc-dev/sqlc/releases/download/v${VERSION}/sqlc_${VERSION}_${PLATFORM}.tar.gz"
# `sha256sum` is the coreutils spelling and `shasum` the macOS one. Either
# refuses a download that is not the pinned release.
if command -v sha256sum >/dev/null 2>&1; then
  echo "$SHA256  $ARCHIVE" | sha256sum -c - >&2
else
  echo "$SHA256  $ARCHIVE" | shasum -a 256 -c - >&2
fi
tar -xzf "$ARCHIVE" -C "$DIR" sqlc
rm -f "$ARCHIVE"
echo "$(cd "$DIR" && pwd)/sqlc"
