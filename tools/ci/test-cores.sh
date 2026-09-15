#!/bin/sh
# A crash in the workspace suite leaves a core, kept with the binary that wrote
# it.
#
# `uf_fmt`'s `guarantees` binary died of SIGSEGV twice on 2026-09-14, and the
# log held nothing but the signal: no "has overflowed its stack", no
# backtrace. The rerun passed, and 560 runs on the same runner class looking
# for it found nothing. A crash that rare is diagnosed from the one that
# happens, and a SIGSEGV leaves nothing behind unless something is waiting
# for it. One core gives the faulting address, the signal's code and the stack
# of every thread. See ubugeeei-prod/uf#1071.
#
#   tools/ci/test-cores.sh run <command>...   run <command> with core dumps on
#   tools/ci/test-cores.sh collect            pack each core with its binary
#
# `run` points `kernel.core_pattern` at a directory under $RUNNER_TEMP and
# raises the core size limit for <command> and everything it starts: cargo,
# and the test binaries under it. The pattern writes the crashing binary's path
# into the core's name (`%E`, with `!` for `/`), because the test binaries are
# rebuilt and relinked on every run and a core cannot be read without the exact
# binary that wrote it. Before it starts the command it crashes a shell and
# looks for the core. A pattern the runner refused would otherwise look exactly
# like a run that never crashed.
#
# `collect` runs after a failed step. It prints the kernel's line for each
# fault, packs each core from a binary under `target/` together with that
# binary, and, where gdb is installed, prints the signal and every thread's
# backtrace into the log.
#
# Neither half can fail the job. The suite's own result is the step's result;
# this only adds evidence to it.
set -u

cores="${RUNNER_TEMP:-/tmp}/test-cores"
packed="${RUNNER_TEMP:-/tmp}/test-cores-packed"

case "${1:-}" in
  run)
    shift
    if [ "$#" -eq 0 ]; then
      echo "usage: $0 run <command>..." >&2
      exit 2
    fi
    mkdir -p "$cores"
    rm -f "$cores"/core.*
    if sudo -n sysctl -q -w "kernel.core_pattern=$cores/core.%p.%s.%E" 2>/dev/null &&
      ulimit -c unlimited 2>/dev/null; then
      sh -c 'kill -s SEGV $$' 2>/dev/null
      if ls "$cores"/core.* >/dev/null 2>&1; then
        rm -f "$cores"/core.*
        echo "test-cores: a crash in this step leaves a core in $cores"
      else
        echo "::warning::test-cores: a crash in this step would leave no core; kernel.core_pattern is $(cat /proc/sys/kernel/core_pattern 2>/dev/null)"
      fi
    else
      echo "::warning::test-cores: core dumps could not be turned on (no passwordless sudo, or a hard core limit)"
    fi
    exec "$@"
    ;;

  collect)
    mkdir -p "$packed"
    sudo -n dmesg 2>/dev/null | grep -E 'segfault|general protection|traps:' | tail -20
    count=0
    for core in "$cores"/core.*; do
      [ -e "$core" ] || continue
      name=${core##*/}
      rest=${name#core.}
      pid=${rest%%.*}
      rest=${rest#*.}
      signal=${rest%%.*}
      binary=$(printf '%s\n' "${rest#*.}" | tr '!' '/')
      echo "test-cores: pid $pid died of signal $signal in $binary"
      # The kernel resolves symlinks in `%E`, so this matches on `/target/`
      # rather than on $GITHUB_WORKSPACE, which may be spelled through one.
      case "$binary" in
        */target/*) ;;
        *)
          echo "test-cores: not a binary this workspace built; the core is not kept"
          continue
          ;;
      esac
      count=$((count + 1))
      if [ "$count" -gt 3 ]; then
        echo "test-cores: three cores are packed already; this one is not"
        continue
      fi
      if command -v gdb >/dev/null 2>&1 && [ -f "$binary" ]; then
        gdb -q -batch -ex 'print $_siginfo' -ex 'info threads' -ex 'thread apply all bt 40' \
          "$binary" "$core" 2>&1 | head -400
      fi
      if [ -f "$binary" ]; then
        tar --sparse -czf "$packed/core-$pid.tar.gz" -C "$cores" "$name" -C "${binary%/*}" "${binary##*/}"
      else
        echo "test-cores: $binary is gone; the core is packed without it"
        tar --sparse -czf "$packed/core-$pid.tar.gz" -C "$cores" "$name"
      fi
    done
    echo "test-cores: $count core(s) from this workspace's binaries, packed into $packed"
    exit 0
    ;;

  *)
    echo "usage: $0 run <command>... | collect" >&2
    exit 2
    ;;
esac
