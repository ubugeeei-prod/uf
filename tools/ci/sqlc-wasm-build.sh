#!/bin/sh
set -eu
root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$root"
cargo build --profile ci-opt -p uf_sqlc --bin sqlc-gen-flow --target wasm32-wasip1
sh tools/ci/sqlc-wasm.sh "$root/target/wasm32-wasip1/ci-opt/sqlc-gen-flow.wasm"
