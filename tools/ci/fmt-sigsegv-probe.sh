#!/usr/bin/env bash
# Temporary probe for ubugeeei-prod/uf#1071, third version. Not for merge.
#
# Asks one question: does wild write the same bytes when it links the same
# inputs twice? CI relinks the `guarantees` test binary on every run, and a
# linker that is occasionally wrong would give a binary that crashes on one run
# and a fresh, correct one on the rerun, which is the pattern #1071 shows.
set -u

section() { printf '\n===== %s\n' "$1"; }

section system
uname -a
nproc
wild --version
ld.bfd --version | head -1
gcc --version | head -1

section sources
echo "packages js: $(find packages -name '*.js' | wc -l)"
echo "packages js under node_modules: $(find packages -path '*/node_modules/*' -name '*.js' | wc -l)"

section build
cargo test -p uf_fmt --test guarantees --profile ci --no-run 2>&1 | tail -2
touch crates/uf_fmt/tests/guarantees.rs
# A different linker is a different fingerprint, so this relinks the binary
# through tools/ci/probe-linker, whose `ld` links the same inputs 25 more times
# before the real link.
CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER="$PWD/tools/ci/probe-linker/cc-linker" \
  cargo test -p uf_fmt --test guarantees --profile ci --no-run \
  --message-format=json 2> /tmp/probe-build.txt \
  | grep -o '"executable":"[^"]*guarantees-[^"]*"' | tail -1 | sed 's/"executable":"//; s/"$//' > /tmp/probe-bin.txt
tail -2 /tmp/probe-build.txt
BIN=$(cat /tmp/probe-bin.txt)
echo "cargo's binary: $BIN"
cat /tmp/links/linkers.txt 2> /dev/null || echo "the relink hook did not run"
if [ -x "$BIN" ]; then
  cp "$BIN" /tmp/links/cargo
fi
ls -la /tmp/links | grep -v '\.log$'
for log in /tmp/links/*.log; do
  [ -s "$log" ] && { echo "--- $log"; head -5 "$log"; }
done

section hashes
sha256sum /tmp/links/wild-* /tmp/links/bfd /tmp/links/cargo 2> /dev/null | grep -v '\.log$' | sort > /tmp/hashes.txt
cat /tmp/hashes.txt
echo "distinct wild outputs:"
grep 'wild-' /tmp/hashes.txt | cut -d' ' -f1 | sort | uniq -c

section differences
first=$(ls /tmp/links/wild-* | grep -v '\.log$' | head -1)
readelf -S -W "$first" > /tmp/sections.txt
for f in $(ls /tmp/links/wild-* | grep -v '\.log$'); do
  if ! cmp -s "$first" "$f"; then
    count=$(cmp -l "$first" "$f" | wc -l)
    echo "--- $f differs from $first in $count bytes; offsets by section:"
    cmp -l "$first" "$f" | head -5000 | awk '{ print $1 - 1 }' > /tmp/offsets.txt
    awk '
      NR == FNR {
        if ($0 ~ /^ *\[ *[0-9]+\]/) {
          sub(/^ *\[ *[0-9]+\] */, "")
          name = $1; off = strtonum("0x" $4); size = strtonum("0x" $5)
          n++; names[n] = name; offs[n] = off; sizes[n] = size
        }
        next
      }
      {
        hit = "(outside sections)"
        for (i = 1; i <= n; i++) if ($1 >= offs[i] && $1 < offs[i] + sizes[i]) { hit = names[i]; break }
        counts[hit]++
      }
      END { for (h in counts) print "  " counts[h], h }
    ' /tmp/sections.txt /tmp/offsets.txt
  fi
done

section runs
mkdir -p /tmp/cores /tmp/probe-logs
chmod 1777 /tmp/cores
sudo sysctl -w kernel.core_pattern=/tmp/cores/core.%e.%p > /dev/null || true
ulimit -c unlimited
crashes=0
declare -A seen
for f in /tmp/links/wild-* /tmp/links/bfd; do
  case "$f" in *.log) continue ;; esac
  [ -x "$f" ] || continue
  hash=$(sha256sum "$f" | cut -d' ' -f1)
  [ -z "${seen[$hash]:-}" ] || continue
  seen[$hash]=1
  echo "--- $f ($hash)"
  for i in $(seq 1 12); do
    (cd crates/uf_fmt && "$f" --test-threads 32 > "/tmp/probe-logs/$(basename "$f").$i.log" 2>&1)
    code=$?
    if [ "$code" -ne 0 ]; then
      crashes=$((crashes + 1))
      echo "CRASH $f run=$i exit=$code"
      grep -v ' \.\.\. ok$' "/tmp/probe-logs/$(basename "$f").$i.log" | tail -8
    fi
  done
done

section cores
for core in /tmp/cores/core.*; do
  [ -e "$core" ] || continue
  exe="/tmp/links/$(basename "$core" | cut -d. -f2)"
  echo "--- $core ($exe)"
  gdb -q -batch -ex 'p $_siginfo._sifields._sigfault.si_addr' -ex 'bt 25' "$exe" "$core" 2>&1 | head -60
done

section summary
echo "PROBE SUMMARY: distinct wild outputs=$(grep 'wild-' /tmp/hashes.txt | cut -d' ' -f1 | sort -u | wc -l) crashes=$crashes"
