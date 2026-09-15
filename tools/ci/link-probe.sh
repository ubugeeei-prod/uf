#!/usr/bin/env bash
# Probe for ubugeeei-prod/uf#1071. Not for merge.
#
# Two questions. Which linker do CI and the release really link with? And does
# wild write the same bytes when it links the same inputs again? The linking
# is done by tools/ci/link-probe-hook.sh, from inside rustc's own link step;
# this builds the binary through it and reports.
#
#   tools/ci/link-probe.sh guarantees   uf_fmt's guarantees test binary
#   tools/ci/link-probe.sh uf           the ci-opt uf binary
set -u

what=$1
root=$(CDPATH='' cd "$(dirname "$0")/../.." && pwd)
cd "$root" || exit 1
dir=${RUNNER_TEMP:-/tmp}/link-probe
rm -rf "$dir"
mkdir -p "$dir/out" "$dir/logs"
export UF_LINK_PROBE_DIR="$dir" UF_LINK_PROBE_TRACE="$dir/trace.txt"

section() { printf '\n===== %s: %s\n' "$what" "$1"; }
comment_of() { readelf -p .comment "$1" 2>&1 | grep -v -e '^$' -e 'String dump'; }

section machine
uname -a
nproc
free -g | head -2
rustc -vV
host=$(rustc -vV | sed -n 's/^host: //p')
echo "target spec for $host, linker keys:"
rustc -Z unstable-options --print target-spec-json | grep -n -i -E 'flavor|self-contained|lld'
wild --version
ld.bfd --version | head -1
cc --version | head -1
ls -la "$(rustc --print sysroot)/lib/rustlib/$host/bin/gcc-ld" 2>&1

section build
case "$what" in
  guarantees)
    touch crates/uf_fmt/tests/guarantees.rs
    cargo test -p uf_fmt --test guarantees --profile ci --no-run \
      --message-format=json-render-diagnostics > "$dir/cargo.json"
    echo "cargo exit $?"
    bin=$(grep -o '"executable":"[^"]*guarantees-[^"]*"' "$dir/cargo.json" | tail -1 |
      sed 's/^"executable":"//; s/"$//')
    ;;
  uf)
    cargo build --profile ci-opt --bin uf --message-format=json-render-diagnostics > "$dir/cargo.json"
    echo "cargo exit $?"
    bin=$(grep -o '"executable":"[^"]*/uf"' "$dir/cargo.json" | tail -1 |
      sed 's/^"executable":"//; s/"$//')
    ;;
  *)
    echo "usage: $0 guarantees|uf" >&2
    exit 2
    ;;
esac
echo "cargo's binary: $bin"
if [ ! -x "$bin" ] || [ ! -f "$dir/$what.args" ]; then
  echo "LINK PROBE $what: no link was captured"
  exit 1
fi
cp "$bin" "$dir/cargo-$what"

section "the link rustc asked for"
echo "output: $(cat "$dir/$what.output")"
echo "arguments: $(wc -l < "$dir/$what.args")"
grep -n -E '^-fuse-ld=|^-B|^-Wl,-z|^-Wl,--(build-id|strip|gc-sections|as-needed)|^-s$|^-pie$|^-no-pie$|^-static' "$dir/$what.args"
echo "every run of tools/linker/uf-linker in this job (linker, output):"
sort "$dir/trace.txt" 2> /dev/null | uniq -c

section "which linker wrote each output (.comment)"
for f in "$dir/cargo-$what" "$dir/out/$what.ci-1" "$dir/out/$what.wild-01" "$dir/out/$what.lld-1" "$dir/out/$what.bfd-1"; do
  echo "--- ${f##*/}"
  comment_of "$f"
done

section "link times (label, exit status, seconds)"
sort -V "$dir/$what.times"
awk '{
  split($1, p, "-"); g = p[1]
  if (g == "wild" && p[2] + 0 > 10) g = "wild, five at a time"
  n[g]++; s[g] += $3
  if (!(g in lo) || $3 < lo[g]) lo[g] = $3
  if ($3 > hi[g]) hi[g] = $3
  if ($2 != 0) bad[g]++
} END {
  for (g in n) printf "%s: %d links, %d failed, mean %.2fs, min %.2fs, max %.2fs\n", g, n[g], bad[g], s[g] / n[g], lo[g], hi[g]
}' "$dir/$what.times"
for log in "$dir"/logs/*.log; do
  if [ -s "$log" ]; then
    echo "--- ${log##*/}"
    head -20 "$log"
  fi
done

section hashes
sha256sum "$dir/cargo-$what" "$dir"/out/* | sed "s|$dir/||" | sort -k2 -V
for group in ci wild lld bfd; do
  echo "$group: $(cat "$dir"/out/"$what.$group"-* 2> /dev/null | wc -c) bytes in all," \
    "$(sha256sum "$dir"/out/"$what.$group"-* | cut -d' ' -f1 | sort -u | wc -l) distinct" \
    "of $(ls "$dir"/out/"$what.$group"-* | wc -l)"
done

# Section-by-section count of the bytes two files differ in.
compare() {
  if cmp -s "$1" "$2"; then
    echo "identical: ${1##*/} ${2##*/}"
    return
  fi
  echo "DIFFERENT: ${1##*/} ${2##*/}, sizes $(stat -c %s "$1") and $(stat -c %s "$2")"
  readelf -S -W "$1" | sed -n 's/^ *\[ *[0-9]*\] *//p' |
    while read -r name type addr off size rest; do
      case "$name" in NULL | Name) continue ;; esac
      echo "$((16#$off)) $((16#$off + 16#$size)) $name"
    done > "$dir/sections.txt"
  cmp -l "$1" "$2" 2> /dev/null | head -2000000 | awk '
    NR == FNR { lo[NR] = $1; hi[NR] = $2; nm[NR] = $3; n = NR; next }
    {
      o = $1 - 1; hit = "(headers or no section)"
      for (i = 1; i <= n; i++) if (o >= lo[i] && o < hi[i]) { hit = nm[i]; break }
      c[hit]++; total++
    }
    END { for (h in c) printf "  %9d bytes in %s\n", c[h], h; printf "  %9d bytes in all\n", total }
  ' "$dir/sections.txt" -
  readelf -n "$1" | grep -i 'build id'
  readelf -n "$2" | grep -i 'build id'
}

section differences
for group in ci wild lld bfd; do
  set -- "$dir"/out/"$what.$group"-*
  first=$1
  shift
  for f in "$@"; do
    compare "$first" "$f"
  done
done
echo "the real link against the same command run by the probe:"
compare "$dir/out/$what.ci-1" "$dir/cargo-$what"
echo "CI's command against each linker, whole-file only:"
for f in "$dir/out/$what.wild-01" "$dir/out/$what.lld-1" "$dir/out/$what.bfd-1"; do
  if cmp -s "$dir/out/$what.ci-1" "$f"; then echo "ci-1 == ${f##*/}"; else echo "ci-1 != ${f##*/}"; fi
done

section runs
declare -A seen
for f in "$dir/cargo-$what" "$dir"/out/*; do
  hash=$(sha256sum "$f" | cut -d' ' -f1)
  if [ -n "${seen[$hash]:-}" ]; then
    continue
  fi
  seen[$hash]=1
  started=$(date +%s)
  case "$what" in
    guarantees) (cd crates/uf_fmt && "$f" --test-threads 32 -q > "$dir/logs/run.${f##*/}.txt" 2>&1) ;;
    uf) "$f" --version > "$dir/logs/run.${f##*/}.txt" 2>&1 ;;
  esac
  echo "${f##*/}: exit $? after $(($(date +%s) - started))s: $(tail -2 "$dir/logs/run.${f##*/}.txt" | tr '\n' ' ')"
done

if [ "$what" = guarantees ] && [ "$host" = x86_64-unknown-linux-gnu ]; then
  section "released Linux binaries"
  for target in x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu; do
    url="https://github.com/ubugeeei-prod/uf/releases/download/uf%400.0.0-alpha.35/uf-$target.tar.gz"
    echo "--- uf@0.0.0-alpha.35 $target"
    if curl -fsSL "$url" -o "$dir/release.tar.gz"; then
      mkdir -p "$dir/release-$target"
      tar -xzf "$dir/release.tar.gz" -C "$dir/release-$target"
      readelf -h "$dir/release-$target/bin/uf" | grep Machine
      comment_of "$dir/release-$target/bin/uf"
    else
      echo "could not fetch $url"
    fi
  done
fi

section summary
summary="LINK PROBE $what on $host:"
for group in ci wild lld bfd; do
  summary="$summary $group=$(sha256sum "$dir"/out/"$what.$group"-* | cut -d' ' -f1 | sort -u | wc -l)-distinct"
done
if cmp -s "$dir/out/$what.ci-1" "$dir/cargo-$what"; then
  summary="$summary real-link==ci-1"
else
  summary="$summary real-link!=ci-1"
fi
echo "$summary"
