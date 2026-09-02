#!/bin/sh
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
cd "$repo_root"

target="${UF_TARGET:-}"
version="${UF_VERSION:-}"
profile="${UF_CARGO_PROFILE:-dist}"
if [ -z "$version" ]; then
  version="$(cargo metadata --no-deps --format-version 1 | node -e '
const fs = require("node:fs");
const metadata = JSON.parse(fs.readFileSync(0, "utf8"));
const pkg = metadata.packages.find((item) => item.name === "uf_cli");
if (!pkg) process.exit(1);
process.stdout.write(pkg.version);
')"
fi

if [ -z "$target" ]; then
  target="$(rustc -vV | awk -F': ' '/^host:/ { print $2 }')"
fi

if [ -z "$target" ]; then
  echo "package-binaries: could not determine Rust target" >&2
  exit 1
fi

profile_dir="$profile"
cargo_profile_flag="--profile $profile"
if [ "$profile" = "release" ]; then
  profile_dir="release"
  cargo_profile_flag="--release"
fi

bin_root="target/$profile_dir"
if [ "${UF_CARGO_TARGET:-1}" != "0" ]; then
  bin_root="target/$target/$profile_dir"
  # shellcheck disable=SC2086
  cargo build $cargo_profile_flag --locked --package uf_cli --bins --target "$target"
else
  # shellcheck disable=SC2086
  cargo build $cargo_profile_flag --locked --package uf_cli --bins
fi

stage_dir="dist/stage/uf-${version}-${target}"
out_dir="dist/release/uf/${version}"
archive="uf-${target}.tar.gz"

rm -rf "$stage_dir"
mkdir -p "$stage_dir/bin" "$out_dir"

for name in uf ufr ufx; do
  if [ ! -x "$bin_root/$name" ]; then
    echo "package-binaries: missing built binary: $bin_root/$name" >&2
    exit 1
  fi
  cp "$bin_root/$name" "$stage_dir/bin/$name"
done

cp README.md LICENSE "$stage_dir/"
printf '%s\n' "$version" > "$stage_dir/VERSION"
printf '%s\n' "$target" > "$stage_dir/TARGET"

tar -czf "$out_dir/$archive" -C "$stage_dir" .
if command -v sha256sum >/dev/null 2>&1; then
  sha="$(sha256sum "$out_dir/$archive" | awk '{print $1}')"
else
  sha="$(shasum -a 256 "$out_dir/$archive" | awk '{print $1}')"
fi
printf '%s  %s\n' "$sha" "$archive" > "$out_dir/$archive.sha256"
printf '%s\n' "$version" > "$out_dir/VERSION"

# The release workflow publishes to GitHub Releases; a mirror sets
# UF_RELEASE_BASE to its own flat `<base>/<version>/<asset>` layout. Recording a
# URL nothing serves is how a manifest ends up advertising a 404.
if [ -n "${UF_RELEASE_BASE:-}" ]; then
  url="${UF_RELEASE_BASE}/${version}/${archive}"
else
  url="https://github.com/${UF_REPO:-ubugeeei-prod/uf}/releases/download/uf@${version}/${archive}"
fi

cat > "$out_dir/manifest-${target}.json" <<EOF
{
  "name": "uf",
  "version": "${version}",
  "target": "${target}",
  "archive": "${archive}",
  "sha256": "${sha}",
  "url": "${url}"
}
EOF

echo "$out_dir/$archive"
