#!/usr/bin/env bash
# Probe for ubugeeei-prod/uf#1071. Not for merge.
#
# `tools/linker/uf-cc-linker` calls this with the exact arguments rustc gave
# it, before it runs the real link. For the two outputs the probe is about it
# links the same inputs again, many times, while rustc's temporary objects
# still exist:
#
#   ci-N    the command CI runs, unchanged
#   wild-N  the same without any `-fuse-ld=`, so gcc takes tools/linker/ld,
#           which is wild
#   lld-N   rust-lld, through rustc's gcc-ld directory
#   bfd-N   GNU ld
set -u

linker_dir=$1
shift
dir=${UF_LINK_PROBE_DIR:?}
mkdir -p "$dir/out" "$dir/logs"

# rustc passes `@file` when a command line is too long; read it back.
args=()
for arg in "$@"; do
  file=${arg#@}
  if [ "$file" != "$arg" ] && [ -f "$file" ]; then
    while IFS= read -r line || [ -n "$line" ]; do
      args+=("$(printf '%s' "$line" | sed 's/\\\(.\)/\1/g')")
    done < "$file"
  else
    args+=("$arg")
  fi
done

out=
for ((i = 0; i + 1 < ${#args[@]}; i++)); do
  if [ "${args[i]}" = -o ]; then
    out=${args[i + 1]}
  fi
done
case "${out##*/}" in
  guarantees-*) name=guarantees ;;
  uf-[0-9a-f]*) name=uf ;;
  *) exit 0 ;;
esac
if [ -e "$dir/$name.args" ]; then
  exit 0
fi
printf '%s\n' "${args[@]}" > "$dir/$name.args"
printf '%s\n' "$out" > "$dir/$name.output"

# Everything but the output, and a second copy without `-fuse-ld=`.
rest_all=()
rest_plain=()
for ((i = 0; i < ${#args[@]}; i++)); do
  if [ "${args[i]}" = -o ]; then
    i=$((i + 1))
    continue
  fi
  rest_all+=("${args[i]}")
  case "${args[i]}" in
    -fuse-ld=*) ;;
    *) rest_plain+=("${args[i]}") ;;
  esac
done

host=$(rustc -vV | sed -n 's/^host: //p')
gcc_ld="$(rustc --print sysroot)/lib/rustlib/$host/bin/gcc-ld"

link() {
  label=$1
  shift
  start=$(date +%s.%N)
  "$@" > "$dir/logs/$name.$label.log" 2>&1
  status=$?
  end=$(date +%s.%N)
  printf '%s %s %s\n' "$label" "$status" \
    "$(awk -v a="$start" -v b="$end" 'BEGIN { printf "%.3f", b - a }')" >> "$dir/$name.times"
}

for i in 1 2; do
  link "ci-$i" cc -B "$linker_dir/" "${rest_all[@]}" -o "$dir/out/$name.ci-$i"
done
for i in 01 02 03 04 05 06 07 08 09 10; do
  link "wild-$i" cc -B "$linker_dir/" "${rest_plain[@]}" -o "$dir/out/$name.wild-$i"
done
# Fifteen more, five at a time, so the links compete for the machine the way
# a link inside a busy build does.
for batch in 1 2 3; do
  for j in 1 2 3 4 5; do
    i=$((10 + (batch - 1) * 5 + j))
    link "wild-$i" cc -B "$linker_dir/" "${rest_plain[@]}" -o "$dir/out/$name.wild-$i" &
  done
  wait
done
for i in 1 2 3; do
  link "lld-$i" cc -B "$gcc_ld/" "${rest_plain[@]}" -fuse-ld=lld -o "$dir/out/$name.lld-$i"
done
for i in 1 2; do
  link "bfd-$i" cc "${rest_plain[@]}" -fuse-ld=bfd -o "$dir/out/$name.bfd-$i"
done
exit 0
