#!/usr/bin/env bash
# Temporary probe for ubugeeei-prod/uf#1071. Not for merge.
#
# Runs the `uf_fmt` `guarantees` test binary many times in parallel on the CI
# runner, with core dumps on, and prints what the kernel and gdb say about any
# process that dies.
set -u

section() { printf '\n===== %s\n' "$1"; }

section system
uname -a
ldd --version | head -1
nproc
head -3 /proc/meminfo
echo "overcommit_memory=$(cat /proc/sys/vm/overcommit_memory) max_map_count=$(cat /proc/sys/vm/max_map_count)"
echo "thp=$(cat /sys/kernel/mm/transparent_hugepage/enabled 2>/dev/null)"
grep -m1 'model name' /proc/cpuinfo
grep -o -w 'avx2\|avx512f\|amx_tile\|user_shstk' /proc/cpuinfo | sort | uniq -c
ulimit -a
echo "core_pattern=$(cat /proc/sys/kernel/core_pattern)"

section build
cargo test -p uf_fmt --test guarantees --profile ci --no-run --message-format=json 2> /tmp/probe-build.txt \
  | grep -o '"executable":"[^"]*guarantees-[^"]*"' | tail -1 | sed 's/"executable":"//; s/"$//' > /tmp/probe-bin.txt
tail -3 /tmp/probe-build.txt
BIN=$(cat /tmp/probe-bin.txt)
echo "binary: $BIN"
if [ ! -x "$BIN" ]; then
  echo "PROBE SUMMARY: build failed"
  exit 1
fi

section setup
mkdir -p /tmp/cores /tmp/probe-logs
chmod 1777 /tmp/cores
sudo sysctl -w kernel.core_pattern=/tmp/cores/core.%p || echo "could not set core_pattern"
ulimit -c unlimited
command -v gdb > /dev/null || sudo apt-get install -y -qq gdb > /dev/null 2>&1 || echo "no gdb"
sudo dmesg -C 2> /dev/null || true

workers=16
iterations=8
worker() {
  local w=$1 i code
  for i in $(seq 1 "$iterations"); do
    "$BIN" --test-threads 32 > "/tmp/probe-logs/$w.$i.log" 2>&1
    code=$?
    if [ "$code" -ne 0 ]; then
      echo "CRASH worker=$w run=$i exit=$code"
      grep -v ' \.\.\. ok$' "/tmp/probe-logs/$w.$i.log" | tail -12
    fi
  done
}

section runs
started=$(date +%s)
for w in $(seq 1 "$workers"); do worker "$w" & done
wait
echo "elapsed=$(( $(date +%s) - started ))s"

section dmesg
sudo dmesg | grep -i 'segfault\|trap\|guarantees\|oom\|killed process' | tail -30

section cores
count=0
for core in /tmp/cores/core.*; do
  [ -e "$core" ] || continue
  count=$((count + 1))
  [ "$count" -le 3 ] || continue
  echo "--- $core"
  gdb -q -batch \
    -ex 'set pagination off' \
    -ex 'p $_siginfo.si_signo' \
    -ex 'p $_siginfo.si_code' \
    -ex 'p $_siginfo._sifields._sigfault.si_addr' \
    -ex 'info registers rip rsp' \
    -ex 'x/3i $pc' \
    -ex 'bt 30' \
    -ex 'info threads' \
    -ex 'thread apply all bt 10' \
    "$BIN" "$core" 2>&1 | head -400
  gdb -q -batch -ex 'maint info sections' "$BIN" "$core" 2>&1 | grep -v '\.debug\|\.note\|\.rela\|\.gnu' | head -300
done

section summary
echo "PROBE SUMMARY: runs=$((workers * iterations)) cores=$count"
[ "$count" -eq 0 ]
