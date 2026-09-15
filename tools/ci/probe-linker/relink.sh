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

# Expand `@file` arguments, which gcc uses for long command lines. The files
# use the same quoting xargs reads.
args=()
for arg in "$@"; do
  if [[ "$arg" == @* && -f "${arg:1}" ]]; then
    while IFS= read -r line; do
      args+=("$line")
    done < <(xargs -a "${arg:1}" printf '%s\n')
  else
    args+=("$arg")
  fi
done

out_index=-1
for ((i = 0; i < ${#args[@]}; i++)); do
  if [ "${args[$i]}" = "-o" ]; then
    out_index=$((i + 1))
  fi
done
if [ "$out_index" -lt 0 ]; then
  echo "relink: no -o in the link arguments" >> /tmp/links/invocations.txt
  exit 0
fi

printf '%s\n' "${args[@]}" > /tmp/links/args.txt
echo "relink: ${#args[@]} arguments, output ${args[$out_index]}, wild at $(command -v wild)" >> /tmp/links/invocations.txt

link_to() {
  local target=$1
  shift
  local a=("${args[@]}")
  a[$out_index]=$target
  "$@" "${a[@]}" > "$target.log" 2>&1 || echo "relink: $* -> $target failed" >> /tmp/links/invocations.txt
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
link_to /tmp/links/bfd ld.bfd -L "$(dirname "$(gcc -print-libgcc-file-name)")"
exit 0
