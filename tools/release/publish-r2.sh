#!/bin/sh
set -eu

if [ "$#" -lt 1 ]; then
  echo "usage: $0 <release-dir> [--latest]" >&2
  exit 1
fi

release_dir="$1"
publish_latest=0
if [ "${2:-}" = "--latest" ]; then
  publish_latest=1
fi

bucket="${UF_RELEASE_BUCKET:-uf-releases}"

if [ ! -d "$release_dir" ]; then
  echo "publish-r2: release directory does not exist: $release_dir" >&2
  exit 1
fi

if [ ! -f "$release_dir/VERSION" ]; then
  echo "publish-r2: missing VERSION in $release_dir" >&2
  exit 1
fi

version="$(tr -d '[:space:]' < "$release_dir/VERSION")"
if [ -z "$version" ]; then
  echo "publish-r2: VERSION is empty" >&2
  exit 1
fi

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "publish-r2: missing required command: $1" >&2
    exit 1
  fi
}

need npx

put_object() {
  src="$1"
  key="$2"
  echo "publish-r2: $src -> r2://$bucket/$key" >&2
  if [ "${UF_R2_DRY_RUN:-0}" = "1" ]; then
    return 0
  fi
  npx --yes wrangler@latest r2 object put "$bucket/$key" --file "$src" --remote
}

for file in "$release_dir"/*; do
  [ -f "$file" ] || continue
  name="$(basename "$file")"
  put_object "$file" "uf/$version/$name"
  if [ "$publish_latest" -eq 1 ]; then
    put_object "$file" "uf/latest/$name"
  fi
done
