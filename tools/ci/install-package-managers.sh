#!/usr/bin/env sh
# The package managers `crates/uf_cli/tests/managers.rs` runs uf against.
#
#   tools/ci/install-package-managers.sh DIR
#
# npm arrives with Node (`actions/setup-node`) and bun with `oven-sh/setup-bun`,
# because other tests already need both. pnpm and both Yarns come from here,
# each release in a directory of its own under DIR: Yarn 1 and Yarn 4 are both
# called `yarn`, pnpm 10 and pnpm 12 are both called `pnpm`, and a test that
# asked for one and ran whichever was installed last would be testing nothing.
#
# The releases are pinned here and nowhere else. Every job that runs the
# workspace suite runs this script — `tools/ci/workspace-suite-runtimes.sh`
# fails the build when one does not — so every job tests the same releases.
#
# - pnpm 10.34.5 is the release the dependency guide locks `pnpm@10` at.
# - pnpm 12.4.2 is pnpm's `latest`, and it links by path only: `pnpm link`
#   with a name, or with nothing, is an error there.
# - Yarn 1.22.22 is the last Yarn 1, and Yarn 4.18.0 Yarn's `latest`.
#
# Run it locally the same way, then point the tests at DIR:
#
#   tools/ci/install-package-managers.sh /tmp/uf-package-managers
#   UF_TEST_PACKAGE_MANAGERS=/tmp/uf-package-managers cargo test -p uf_cli --test managers
#
# In GitHub Actions it also writes `UF_TEST_PACKAGE_MANAGERS` to `$GITHUB_ENV`.
set -eu

dest="${1:?usage: tools/ci/install-package-managers.sh DIR}"
mkdir -p "$dest"
dest="$(CDPATH= cd "$dest" && pwd)"

# row directory, npm package, version, the program it provides.
install() {
  row="$1"
  package="$2"
  version="$3"
  program="$4"
  mkdir -p "$dest/$row"
  # `--ignore-scripts`: none of the four needs an install script to run, and a
  # CI step that runs a package's scripts is a step that runs code nobody read.
  npm install --prefix "$dest/$row" --no-save --no-package-lock --no-audit \
    --no-fund --ignore-scripts --loglevel=error "$package@$version" >/dev/null
  bin="$dest/$row/node_modules/.bin/$program"
  if [ ! -x "$bin" ]; then
    echo "$package@$version did not install $program at $bin" >&2
    exit 1
  fi
  # A release that reports a different version is a pin that did not hold.
  # Asked from its own directory, so the manifest of wherever this script was
  # started from is not something pnpm stops to warn about.
  found="$(cd "$dest/$row" && "$bin" --version)"
  if [ "$found" != "$version" ]; then
    echo "$package@$version installed a $program that reports $found" >&2
    exit 1
  fi
  echo "$row: $program $found"
}

install pnpm-10 pnpm 10.34.5 pnpm
install pnpm-12 pnpm 12.4.2 pnpm
install yarn-1 yarn 1.22.22 yarn
install yarn-4 @yarnpkg/cli-dist 4.18.0 yarn

if [ -n "${GITHUB_ENV:-}" ]; then
  echo "UF_TEST_PACKAGE_MANAGERS=$dest" >>"$GITHUB_ENV"
fi
