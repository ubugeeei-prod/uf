#!/usr/bin/env bash
# Temporary probe for ubugeeei-prod/uf#1071. Not for merge.
#
# Called by tools/ci/probe-linker/ld with the exact arguments of the real link
# of the uf_fmt `guarantees` test binary. Links the same inputs again into
# /tmp/links: twelve times through tools/linker/uf-linker (wild, on CI) one
# after another, twelve more four at a time, and once with GNU ld. The real
# link runs afterwards, unchanged.
set -u

uf_linker=$1
shift
args=("$@")
out_index=-1
for ((i = 0; i < ${#args[@]}; i++)); do
  if [ "${args[$i]}" = "-o" ]; then
    out_index=$((i + 1))
  fi
done
if [ "$out_index" -lt 0 ]; then
  echo "relink: no -o in the link arguments" >&2
  exit 0
fi

mkdir -p /tmp/links
printf '%s\n' "${args[@]}" > /tmp/links/args.txt
echo "wild: $(command -v wild)" > /tmp/links/linkers.txt

link_to() {
  local target=$1
  shift
  local a=("${args[@]}")
  a[$out_index]=$target
  "$@" "${a[@]}" > "$target.log" 2>&1 || echo "relink: $* -> $target failed" >&2
}

for k in $(seq 1 12); do
  link_to "/tmp/links/wild-seq-$k" "$uf_linker"
done
for batch in 1 2 3; do
  for k in 1 2 3 4; do
    link_to "/tmp/links/wild-par-$batch-$k" "$uf_linker" &
  done
  wait
done
link_to /tmp/links/bfd ld.bfd
exit 0
