#!/bin/sh
set -eu

release_base="${UF_RELEASE_BASE:-https://releases.uniflowed.dev/uf}"
requested_version="${UF_VERSION:-latest}"
install_root="${UF_INSTALL_ROOT:-${XDG_DATA_HOME:-$HOME/.local/share}/uf}"
bin_dir="${UF_BIN_DIR:-$HOME/.local/bin}"

uf_brand() {
  printf '%s\n' \
    "uf  Unified Toolchain for Flow" \
    "    All-in-one toolchain for Flow and React." \
    "    ----------------------------------------" \
    "    Unified  Fast  Elegant  Modern  Developer-first" >&2
}

uf_step() {
  printf 'uf installer: %s\n' "$1" >&2
}

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "uf installer: missing required command: $1" >&2
    exit 1
  fi
}

need curl
need tar
need mktemp
need uname

uf_brand

case "$(uname -s)" in
  Darwin) os="apple-darwin" ;;
  Linux) os="unknown-linux-gnu" ;;
  *)
    echo "uf installer: unsupported OS: $(uname -s)" >&2
    exit 1
    ;;
esac

case "$(uname -m)" in
  arm64 | aarch64) arch="aarch64" ;;
  x86_64 | amd64) arch="x86_64" ;;
  *)
    echo "uf installer: unsupported architecture: $(uname -m)" >&2
    exit 1
    ;;
esac

target="${arch}-${os}"
uf_step "target ${target}"

case "$requested_version" in
  uf@*) requested_version="${requested_version#uf@}" ;;
esac

channel_url="${release_base}/${requested_version}"
version="$requested_version"
if [ "$requested_version" = "latest" ]; then
  version="$(curl -fsSL "${channel_url}/VERSION" | tr -d '[:space:]')"
fi
uf_step "version ${version}"

archive="uf-${target}.tar.gz"
archive_url="${channel_url}/${archive}"
checksum_url="${archive_url}.sha256"

tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT INT TERM

uf_step "downloading ${archive}"
curl -fsSL "$archive_url" -o "${tmp_dir}/${archive}"
curl -fsSL "$checksum_url" -o "${tmp_dir}/${archive}.sha256"

expected="$(awk '{print $1}' "${tmp_dir}/${archive}.sha256")"
if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "${tmp_dir}/${archive}" | awk '{print $1}')"
elif command -v shasum >/dev/null 2>&1; then
  actual="$(shasum -a 256 "${tmp_dir}/${archive}" | awk '{print $1}')"
else
  echo "uf installer: missing sha256sum or shasum" >&2
  exit 1
fi

if [ "$actual" != "$expected" ]; then
  echo "uf installer: checksum mismatch for ${archive}" >&2
  echo "expected: $expected" >&2
  echo "actual:   $actual" >&2
  exit 1
fi
uf_step "checksum verified"

runtime_dir="${install_root}/runtimes/uf@${version}"
mkdir -p "$runtime_dir" "$bin_dir"
tar -xzf "${tmp_dir}/${archive}" -C "$runtime_dir"
uf_step "installed runtime ${runtime_dir}"

for name in uf ufr ufx; do
  if [ ! -x "${runtime_dir}/bin/${name}" ]; then
    echo "uf installer: archive did not contain bin/${name}" >&2
    exit 1
  fi
  ln -sfn "${runtime_dir}/bin/${name}" "${bin_dir}/${name}"
done
uf_step "linked uf, ufr, ufx into ${bin_dir}"

echo "uf ${version} installed to ${runtime_dir}" >&2
case ":$PATH:" in
  *":${bin_dir}:"*) ;;
  *)
    echo "uf installer: add ${bin_dir} to PATH to use uf from new shells" >&2
    ;;
esac
