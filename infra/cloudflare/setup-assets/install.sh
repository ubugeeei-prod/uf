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

# Decoded size of the embedded logo, which the iTerm2 protocol asks for.
uf_logo_bytes=8177

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

# Which inline-image protocol this terminal speaks, if any.
#
# There is no way to ask it. Both protocols have a query form, but the answer
# arrives on stdin — and stdin here is the script itself, coming down a pipe
# from curl. So this is detection from the environment, and it is deliberately
# conservative: anything unrecognised gets the block mark, which always renders.
#
# Mirrors `ImageProtocol` in `crates/uf_term/src/image/protocol.rs`. The two
# have to agree: the installer and the binary it installs should not disagree
# about what the terminal they are both printing to can draw.
uf_image_protocol() {
  case "${UF_INLINE_IMAGES:-}" in
    0 | never | no | off | false) return 1 ;;
    kitty) printf kitty; return 0 ;;
    iterm2 | iterm) printf iterm2; return 0 ;;
  esac

  # A multiplexer rewrites the stream it forwards, and the failure mode is not
  # a missing picture — it is escape-sequence garbage across the pane.
  [ -z "${TMUX:-}" ] || return 1
  [ -z "${STY:-}" ] || return 1

  if [ -n "${KITTY_WINDOW_ID:-}" ]; then
    printf kitty
    return 0
  fi
  case "${TERM:-}" in
    *kitty* | *ghostty*) printf kitty; return 0 ;;
  esac
  case "${TERM_PROGRAM:-}" in
    WezTerm | ghostty) printf kitty; return 0 ;;
    iTerm.app | vscode | Hyper | rio) printf iterm2; return 0 ;;
  esac
  if [ -n "${KONSOLE_VERSION:-}" ]; then
    printf iterm2
    return 0
  fi
  return 1
}

# The logo, as a 240x65 PNG. Embedded rather than fetched: the installer must
# work before anything of uf's exists on the machine, and one more network
# round trip is one more thing that can fail between `curl` and a working `uf`.
uf_logo_png_base64() {
  tr -d '\n' <<'UF_LOGO_PNG'
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
4HNT4P8Dj9AIO6tNCf4AAAAASUVORK5CYII=
UF_LOGO_PNG
}

# Draw the real logo, or report that this terminal cannot.
uf_logo_image() {
  [ -n "$uf_colour" ] || return 1
  protocol="$(uf_image_protocol)" || return 1

  case "$protocol" in
    kitty)
      # `a=T` transmit and display, `f=100` the payload is a PNG, `t=d` the
      # payload is the data itself, `c`/`r` place it in a box of cells.
      #
      # The protocol caps one sequence's payload at 4096 base64 characters, so
      # the payload is split: every chunk but the last carries `m=1`, "more is
      # coming", and only the first carries the rest of the keys — repeating
      # them on a continuation is an error rather than a redundancy. awk holds
      # one line back so it knows which chunk is the last one.
      printf '  ' >&2
      uf_logo_png_base64 | fold -w 4096 | awk '
        NR > 1 {
          if (NR == 2) printf "\033_Ga=T,f=100,t=d,c=19,r=5,m=1;%s\033\\", previous
          else printf "\033_Gm=1;%s\033\\", previous
        }
        { previous = $0 }
        END {
          if (NR == 0) exit 1
          if (NR == 1) printf "\033_Ga=T,f=100,t=d,c=19,r=5,m=0;%s\033\\\n", previous
          else printf "\033_Gm=0;%s\033\\\n", previous
        }
      ' >&2
      ;;
    iterm2)
      printf '  \033]1337;File=inline=1;preserveAspectRatio=1;size=%s;width=19;height=5:%s\a\n' \
        "$uf_logo_bytes" "$(uf_logo_png_base64)" >&2
      ;;
    *) return 1 ;;
  esac
}

# The mark, in the brand's five stops from top to bottom.
#
# One colour per row rather than per character: a per-character gradient means
# slicing a string that is full of multi-byte block characters, and `cut -c`
# counts bytes — it cuts them in half and prints replacement characters.
uf_logo_blocks() {
  uf_paint '53;214;246'  "  ██    ██   ████████"
  uf_paint '38;119;255'  "  ██    ██   ██"
  uf_paint '92;73;255'   "  ██    ██   ██████"
  uf_paint '143;75;255'  "  ██    ██   ██"
  uf_paint '216;75;255'  "   ██████    ██"
}

uf_brand() {
  printf '\n' >&2
  uf_logo_image || uf_logo_blocks
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
