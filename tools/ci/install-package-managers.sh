#!/usr/bin/env sh
# The package managers `crates/uf_cli/tests/managers.rs` runs uf against.
#
#   tools/ci/install-package-managers.sh DIR
#
# npm arrives with Node (`actions/setup-node`) and bun with `oven-sh/setup-bun`,
# because other tests already need both. pnpm and both Yarns come from here,
# each release in a directory of its own under DIR, with the program linked at
# DIR/<release>/bin: Yarn 1 and Yarn 4 are both called `yarn`, pnpm 10 and
# pnpm 12 are both called `pnpm`, and a test that asked for one and ran
# whichever was installed last would be testing nothing.
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

# The platform, spelled the way pnpm names the package of each executable.
case "$(uname -s)" in
  Linux) os=linux ;;
  Darwin) os=darwin ;;
  *)
    echo "no pnpm executable this script knows is published for $(uname -s)" >&2
    exit 1
    ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) arch=x64 ;;
  aarch64 | arm64) arch=arm64 ;;
  *)
    echo "no pnpm executable this script knows is published for $(uname -m)" >&2
    exit 1
    ;;
esac

# row directory, npm package, version, the program it provides, and the file
# that is that program, relative to the row directory.
install() {
  row="$1"
  package="$2"
  version="$3"
  program="$4"
  file="$dest/$row/$5"
  mkdir -p "$dest/$row/bin"
  # `--ignore-scripts`: a CI step that runs a package's install scripts is a
  # step that runs code nobody read, and none of the four needs one — see pnpm
  # 12 below for the one that would otherwise have used its script.
  npm install --prefix "$dest/$row" --no-save --no-package-lock --no-audit \
    --no-fund --ignore-scripts --loglevel=error "$package@$version" >/dev/null
  if [ ! -x "$file" ]; then
    echo "$package@$version did not install $5" >&2
    exit 1
  fi
  # uf starts a manager itself, without a shell, so the file has to be one the
  # kernel starts: a native executable or a `#!` script. A shell runs a
  # shebang-less script regardless, and so does the version check below, which
  # is how this script once passed a pnpm that uf could not start at all:
  # `Exec format error (os error 8)`, on Linux only.
  magic="$(od -An -tx1 -N4 "$file" | tr -d ' \n')"
  case "$magic" in
    2321* | 7f454c46 | cffaedfe | cefaedfe | cafebabe) ;;
    *)
      echo "$5 from $package@$version is neither a native executable nor a #! script" >&2
      exit 1
      ;;
  esac
  ln -sfn "$file" "$dest/$row/bin/$program"
  # A release that reports a different version is a pin that did not hold.
  # Asked from its own directory, so the manifest of wherever this script was
  # started from is not something pnpm stops to warn about.
  found="$(cd "$dest/$row" && "$dest/$row/bin/$program" --version)"
  if [ "$found" != "$version" ]; then
    echo "$package@$version installed a $program that reports $found" >&2
    exit 1
  fi
  echo "$row: $program $found"
}

install pnpm-10 pnpm 10.34.5 pnpm node_modules/.bin/pnpm
# pnpm 12 is a native executable, published as one package per platform that
# npm installs as an optional dependency. The `pnpm` in `pnpm` itself is a
# shebang-less placeholder its install script replaces with that executable,
# and install scripts do not run here, so the platform's executable is linked
# instead.
install pnpm-12 pnpm 12.4.2 pnpm "node_modules/@pnpm/exe.$os-$arch/pnpm"
install yarn-1 yarn 1.22.22 yarn node_modules/.bin/yarn
install yarn-4 @yarnpkg/cli-dist 4.18.0 yarn node_modules/.bin/yarn

if [ -n "${GITHUB_ENV:-}" ]; then
  echo "UF_TEST_PACKAGE_MANAGERS=$dest" >>"$GITHUB_ENV"
fi
