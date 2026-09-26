#!/bin/sh
# Materialize the official Rust compiler and its matching fixture snapshots.
# Both come from the upstream/react gitlink; Babel is never executed.
set -eu
repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"
tools/upstream/sync.sh
fixtures=$(awk '$1 == "fixtures" { print $2 }' tools/react-compiler/pin.txt)
corpus=upstream/react/$fixtures
count=$(find "$corpus" -type f \( -name '*.js' -o -name '*.jsx' -o -name '*.ts' -o -name '*.tsx' \) | wc -l | tr -d ' ')
printf 'react-compiler: %s official fixtures ready at %s\n' "$count" "$corpus"
