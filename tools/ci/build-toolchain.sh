#!/bin/sh
set -eu
# CI downloads the binary built from this run's commit. Keep local builds
# automatic, but never replace that artifact with another release build.
if [ "${UF_CI_PREBUILT:-0}" = 1 ]; then
  test -x target/release/uf || {
    echo 'The CI toolchain artifact is missing or not executable.' >&2
    exit 1
  }
else
  cargo build --release --bin uf
fi
