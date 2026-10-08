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
if ! git cat-file -e "${BASE_SHA}^{commit}" 2>/dev/null; then
  git fetch --no-tags --depth=1 origin "$BASE_SHA"
fi
git worktree add --detach "$baseline" "$BASE_SHA"
# The example uses the same public lint API and is identical on both revisions.
cp crates/uf_lint/examples/import_graph_alloc.rs "$baseline/crates/uf_lint/examples/import_graph_alloc.rs"
(cd "$baseline" && tools/upstream/sync.sh)
# Cargo started here would compile the base revision with this directory's
# pin. That source still calls `u32::max_value()` inside index_vec, which
# the newer nightly denies, so the build uses the toolchain the base names
# and a target directory of its own.
baseline_target="$baseline/target"
(
  cd "$baseline"
  CARGO_TARGET_DIR="$baseline_target" cargo run -p uf_lint --example import_graph_alloc --profile ci-opt --locked > "$report/import-before.txt"
  CARGO_TARGET_DIR="$baseline_target" cargo build --bin uf --profile ci-opt --locked
)
cp "$head_binary" "$report/uf-after"
cp "$baseline_target/ci-opt/uf" "$report/uf-before"
strip "$report/uf-before" "$report/uf-after"
wc -c "$report/uf-before" "$report/uf-after" > "$report/binary-size.txt"
size "$report/uf-before" "$report/uf-after" >> "$report/binary-size.txt"
rm "$report/uf-before" "$report/uf-after"
cat "$report/import-before.txt" "$report/import-after.txt" "$report/binary-size.txt"
