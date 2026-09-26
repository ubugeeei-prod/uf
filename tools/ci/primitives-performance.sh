#!/bin/sh
# Controlled same-run comparison. Timings describe these fixtures, not all uf workloads.
set -eu
root=$(git rev-parse --show-toplevel)
cd "$root"
report=${PRIMITIVES_REPORT_DIR:-/tmp/uf-primitives-performance}
mkdir -p "$report"
cargo run -p uf_lint --example import_graph_alloc --profile ci-opt --locked > "$report/import-after.txt"
head_binary="$root/target/release/uf"
if [ -z "${BASE_SHA:-}" ] || [ ! -f "$head_binary" ]; then
  exit 0
fi
baseline=$(mktemp -d "${TMPDIR:-/tmp}/uf-primitives-baseline.XXXXXX")
rmdir "$baseline"
git worktree add --detach "$baseline" "$BASE_SHA"
# The example uses the same public lint API and is identical on both revisions.
cp crates/uf_lint/examples/import_graph_alloc.rs "$baseline/crates/uf_lint/examples/import_graph_alloc.rs"
(cd "$baseline" && tools/upstream/sync.sh)
export CARGO_TARGET_DIR="$root/target"
cargo run --manifest-path "$baseline/Cargo.toml" -p uf_lint --example import_graph_alloc --profile ci-opt --locked > "$report/import-before.txt"
cargo build --manifest-path "$baseline/Cargo.toml" --bin uf --profile ci-opt --locked
cp "$head_binary" "$report/uf-after"
cp "$CARGO_TARGET_DIR/ci-opt/uf" "$report/uf-before"
strip "$report/uf-before" "$report/uf-after"
wc -c "$report/uf-before" "$report/uf-after" > "$report/binary-size.txt"
size "$report/uf-before" "$report/uf-after" >> "$report/binary-size.txt"
rm "$report/uf-before" "$report/uf-after"
cat "$report/import-before.txt" "$report/import-after.txt" "$report/binary-size.txt"
