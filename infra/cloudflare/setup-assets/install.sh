#!/bin/sh
set -eu

# GitHub Releases is the source of truth, so `curl | sh` works with no CDN in
# front of it. UF_RELEASE_BASE switches to the flat `<base>/<version>/<asset>`
# layout a mirror serves.
release_base="${UF_RELEASE_BASE:-}"
repo="${UF_REPO:-ubugeeei-prod/uf}"
requested_version="${UF_VERSION:-latest}"
install_root="${UF_INSTALL_ROOT:-${XDG_DATA_HOME:-$HOME/.local/share}/uf}"
bin_dir="${UF_BIN_DIR:-$HOME/.local/bin}"

# Colour, unless the terminal or the reader has said not to.
#
# `NO_COLOR` is the convention, and a redirected stream is not a terminal — an
# installer whose output is being piped into a log should not fill it with
# escape sequences.
if [ -n "${NO_COLOR:-}" ] || [ ! -t 2 ]; then
  uf_colour=""
else
  uf_colour="yes"
fi

uf_paint() {
  if [ -z "$uf_colour" ]; then
    printf '%s\n' "$2" >&2
  else
    printf '\033[38;2;%sm%s\033[0m\n' "$1" "$2" >&2
  fi
}

# The mark, in the brand's five stops from top to bottom.
#
# One colour per row rather than per character: a per-character gradient means
# slicing a string that is full of multi-byte block characters, and `cut -c`
# counts bytes — it cuts them in half and prints replacement characters.
uf_brand() {
  printf '\n' >&2
  uf_paint '53;214;246'  "  ██    ██   ████████"
  uf_paint '38;119;255'  "  ██    ██   ██"
  uf_paint '92;73;255'   "  ██    ██   ██████"
  uf_paint '143;75;255'  "  ██    ██   ██"
  uf_paint '216;75;255'  "   ██████    ██"
  printf '\n' >&2
  if [ -z "$uf_colour" ]; then
    printf '  %s\n\n' "Unified Toolchain for Flow" >&2
  else
    printf '  \033[1m%s\033[0m \033[2m%s\033[0m\n\n' \
      "Unified Toolchain for Flow" "· one binary for Flow and React" >&2
  fi
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

# The newest release including prereleases.
#
# `releases/latest` is GitHub's *stable* channel: it has no answer while every
# release so far is a prerelease, which is every release uf has published. So
# `latest` resolves in two steps — the stable channel first, because that is
# what `latest` should mean the day a stable release exists, then the releases
# list, which includes prereleases and is ordered newest first. Drafts are not
# in that list for an anonymous caller, so the first entry is the answer.
#
# Parsed with sed rather than jq, because an installer cannot require a JSON
# parser to be installed before it can install anything.
newest_prerelease_tag() {
  curl -fsSL -H 'accept: application/vnd.github+json' \
    "https://api.github.com/repos/${repo}/releases?per_page=1" 2>/dev/null |
    tr ',' '\n' |
    sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' |
    head -1
}

version="$requested_version"
if [ -n "$release_base" ]; then
  channel_url="${release_base}/${requested_version}"
  if [ "$requested_version" = "latest" ]; then
    if ! version="$(curl -fsSL "${channel_url}/VERSION" | tr -d '[:space:]')" \
      || [ -z "$version" ]; then
      echo "uf installer: could not resolve the latest version from ${channel_url}/VERSION" >&2
      echo "uf installer: set UF_VERSION to install a specific release" >&2
      exit 1
    fi
  fi
elif [ "$requested_version" = "latest" ]; then
  stable_url="https://github.com/${repo}/releases/latest/download"
  if version="$(curl -fsSL "${stable_url}/VERSION" 2>/dev/null | tr -d '[:space:]')" \
    && [ -n "$version" ]; then
    channel_url="$stable_url"
  else
    uf_step "no stable release yet, taking the newest prerelease"
    tag="$(newest_prerelease_tag)"
    version="${tag#uf@}"
    if [ -z "$version" ]; then
      echo "uf installer: could not resolve a release for ${repo}" >&2
      echo "uf installer: set UF_VERSION to install a specific release" >&2
      exit 1
    fi
    channel_url="https://github.com/${repo}/releases/download/uf@${version}"
  fi
else
  channel_url="https://github.com/${repo}/releases/download/uf@${requested_version}"
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

# Refuse an archive that would write outside the runtime directory. The
# checksum only proves the archive matches what the same host advertised, so it
# does not bound where the members land.
if tar -tzf "${tmp_dir}/${archive}" | grep -Eq '^/|(^|/)\.\.(/|$)'; then
  echo "uf installer: ${archive} contains paths outside the archive root" >&2
  exit 1
fi

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
