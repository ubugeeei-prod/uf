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
uf_logo_bytes=10511

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

# Unicode marks, unless the locale says the terminal cannot render them.
#
# Mirrors `detect_glyphs` in `crates/uf_term/src/capability.rs`, down to
# treating an unset locale as UTF-8: it is the common case on macOS and in CI
# images that render it correctly, so it is not a downgrade signal. The marks
# themselves are `Status::glyph`'s — the installer and the binary it installs
# should not disagree about what a tick looks like.
case "${LC_ALL:-${LC_CTYPE:-${LANG:-}}}" in
  "") uf_tick="✓" uf_cross="✗" ;;
  *[Uu][Tt][Ff]-8* | *[Uu][Tt][Ff]8*) uf_tick="✓" uf_cross="✗" ;;
  *) uf_tick="+" uf_cross="x" ;;
esac
if [ "${TERM:-}" = "dumb" ] || [ -n "${NO_COLOR:-}" ]; then
  uf_tick="+"
  uf_cross="x"
fi

# `$HOME` written as `~`, because a home directory is most of the width of an
# install path and none of the information in it.
#
# For prose only. A `~` inside the double quotes of an `export PATH=…` does not
# expand — the shell only expands it unquoted, and at the start of a word — so
# a line printed for the reader to paste uses `uf_home_var` instead.
uf_tilde() {
  case "$1" in
    "$HOME"/*) printf '~%s' "${1#"$HOME"}" ;;
    *) printf '%s' "$1" ;;
  esac
}

# The same path with a literal `$HOME`, for a line the reader will paste.
#
# Written into single quotes here so this script does not expand it: the point
# is that the *reader's* shell does, in whatever profile they paste it into.
uf_home_var() {
  case "$1" in
    "$HOME"/*) printf '$HOME%s' "${1#"$HOME"}" ;;
    *) printf '%s' "$1" ;;
  esac
}

# `label   value`, with the label dim and the column fixed.
#
# The width is 9 because `version` is the longest label here and a fixed column
# is what lets the eye run down the values rather than hunting for where each
# one starts.
uf_field() {
  if [ -z "$uf_colour" ]; then
    printf '  %-9s%s\n' "$1" "$2" >&2
  else
    printf '  \033[2m%-9s\033[0m%s\n' "$1" "$2" >&2
  fi
}

# `✓ verb      what`, one line per thing that happened.
#
# Past tense, and the verb first: the reader is skimming for what was done, and
# every line beginning with the same prefix — `uf installer:`, as these lines
# used to — puts fourteen identical characters in front of the only part that
# differs.
uf_step() {
  if [ -z "$uf_colour" ]; then
    printf '  %s %-11s%s\n' "$uf_tick" "$1" "$2" >&2
  else
    printf '  \033[38;2;53;214;246m%s\033[0m \033[2m%-11s\033[0m%s\n' \
      "$uf_tick" "$1" "$2" >&2
  fi
}

# A note that is not a step and not a failure.
uf_note() {
  if [ -z "$uf_colour" ]; then
    printf '  %s\n' "$1" >&2
  else
    printf '  \033[2m%s\033[0m\n' "$1" >&2
  fi
}

# Say what went wrong, say what to do about it, and stop.
#
# Everything after the first argument is a hint line, and that is why this is
# one function rather than a pair of `echo`s: an installer that reports a
# failure without saying what would fix it has told the reader only that they
# are stuck.
uf_fail() {
  message="$1"
  shift
  printf '\n' >&2
  if [ -z "$uf_colour" ]; then
    printf '  %s %s\n' "$uf_cross" "$message" >&2
    for hint in "$@"; do
      [ -n "$hint" ] && printf '    %s\n' "$hint" >&2
    done
  else
    printf '  \033[38;2;255;93;93m%s\033[0m %s\n' "$uf_cross" "$message" >&2
    for hint in "$@"; do
      [ -n "$hint" ] && printf '    \033[2m%s\033[0m\n' "$hint" >&2
    done
  fi
  printf '\n' >&2
  exit 1
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

# The mark, as a 128x128 PNG. The mark rather than the full lockup: the lockup
# carries the wordmark, and the line under it already says "Unified Toolchain
# for Flow" in the terminal's own font — drawing the words twice, once as
# pixels, looks like a mistake.
#
# Embedded rather than fetched: the installer must work before anything of uf's
# exists on the machine, and one more network round trip is one more thing that
# can fail between `curl` and a working `uf`.
uf_logo_png_base64() {
  tr -d '\n' <<'UF_LOGO_PNG'
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
Av8XTnzctzu0Y9QAAAAASUVORK5CYII=
UF_LOGO_PNG
}

# Draw the real logo, or report that this terminal cannot.
uf_logo_image() {
  [ -n "$uf_colour" ] || return 1
  protocol="$(uf_image_protocol)" || return 1

  case "$protocol" in
    kitty)
      # `a=T` transmit and display, `f=100` the payload is a PNG, `t=d` the
      # payload is the data itself, `c`/`r` place it in a box of cells. The box
      # is ten by five for a square mark, because a cell is about twice as tall
      # as it is wide — kitty scales into the box it is given and will not
      # preserve the ratio for you.
      #
      # The protocol caps one sequence's payload at 4096 base64 characters, so
      # the payload is split: every chunk but the last carries `m=1`, "more is
      # coming", and only the first carries the rest of the keys — repeating
      # them on a continuation is an error rather than a redundancy. awk holds
      # one line back so it knows which chunk is the last one.
      printf '  ' >&2
      uf_logo_png_base64 | fold -w 4096 | awk '
        NR > 1 {
          if (NR == 2) printf "\033_Ga=T,f=100,t=d,c=10,r=5,m=1;%s\033\\", previous
          else printf "\033_Gm=1;%s\033\\", previous
        }
        { previous = $0 }
        END {
          if (NR == 0) exit 1
          if (NR == 1) printf "\033_Ga=T,f=100,t=d,c=10,r=5,m=0;%s\033\\\n", previous
          else printf "\033_Gm=0;%s\033\\\n", previous
        }
      ' >&2
      ;;
    iterm2)
      printf '  \033]1337;File=inline=1;preserveAspectRatio=1;size=%s;width=10;height=5:%s\a\n' \
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

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    uf_fail "$1 is not installed" "the installer needs curl, tar, mktemp and uname"
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
  *) uf_fail "no build for $(uname -s)" "uf ships macOS and Linux binaries" ;;
esac

case "$(uname -m)" in
  arm64 | aarch64) arch="aarch64" ;;
  x86_64 | amd64) arch="x86_64" ;;
  *) uf_fail "no build for $(uname -m)" "uf ships aarch64 and x86_64 binaries" ;;
esac

target="${arch}-${os}"

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
      uf_fail "no version at ${channel_url}/VERSION" \
        "set UF_VERSION to install a specific release"
    fi
  fi
elif [ "$requested_version" = "latest" ]; then
  stable_url="https://github.com/${repo}/releases/latest/download"
  if version="$(curl -fsSL "${stable_url}/VERSION" 2>/dev/null | tr -d '[:space:]')" \
    && [ -n "$version" ]; then
    channel_url="$stable_url"
  else
    prerelease="yes"
    tag="$(newest_prerelease_tag)"
    version="${tag#uf@}"
    if [ -z "$version" ]; then
      uf_fail "no release found for ${repo}" \
        "set UF_VERSION to install a specific release"
    fi
    channel_url="https://github.com/${repo}/releases/download/uf@${version}"
  fi
else
  channel_url="https://github.com/${repo}/releases/download/uf@${requested_version}"
fi

uf_field "target" "$target"
if [ -n "${prerelease:-}" ]; then
  uf_field "version" "${version}  (prerelease — no stable release yet)"
else
  uf_field "version" "$version"
fi
printf '\n' >&2

archive="uf-${target}.tar.gz"
archive_url="${channel_url}/${archive}"
checksum_url="${archive_url}.sha256"

tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT INT TERM

# Fetch, keeping curl's own diagnosis for the hint line rather than letting it
# print ahead of ours. "404" and "could not resolve host" are different problems
# with different fixes, and a formatted failure that swallowed the difference
# would be prettier and less useful.
uf_fetch() {
  if ! curl -fsSL "$1" -o "$2" 2>"${tmp_dir}/curl.err"; then
    uf_fail "$3" "$(tr -d '\r' <"${tmp_dir}/curl.err" | tail -1)" "$1"
  fi
}

uf_fetch "$archive_url" "${tmp_dir}/${archive}" \
  "could not download ${archive}"
uf_fetch "$checksum_url" "${tmp_dir}/${archive}.sha256" \
  "could not download the checksum for ${archive}"
uf_step "downloaded" "$archive"

expected="$(awk '{print $1}' "${tmp_dir}/${archive}.sha256")"
if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "${tmp_dir}/${archive}" | awk '{print $1}')"
elif command -v shasum >/dev/null 2>&1; then
  actual="$(shasum -a 256 "${tmp_dir}/${archive}" | awk '{print $1}')"
else
  uf_fail "no sha256sum or shasum" "one of them is needed to verify the download"
fi

if [ "$actual" != "$expected" ]; then
  uf_fail "checksum mismatch for ${archive}" \
    "expected ${expected}, got ${actual} — do not run this binary"
fi

# Refuse an archive that would write outside the runtime directory. The
# checksum only proves the archive matches what the same host advertised, so it
# does not bound where the members land.
if tar -tzf "${tmp_dir}/${archive}" | grep -Eq '^/|(^|/)\.\.(/|$)'; then
  uf_fail "${archive} writes outside its own directory" \
    "the archive is not one uf published — do not unpack it"
fi
# The first twelve characters, the way git shows a commit. The full digest is
# 64 characters of noise to a reader who cannot check it by eye, and it pushed
# every other line's value out of the column.
uf_step "verified" "sha256 $(printf '%.12s' "$expected")"

runtime_dir="${install_root}/runtimes/uf@${version}"
mkdir -p "$runtime_dir" "$bin_dir"
tar -xzf "${tmp_dir}/${archive}" -C "$runtime_dir"
uf_step "unpacked" "$(uf_tilde "$runtime_dir")"

for name in uf ufr ufx; do
  if [ ! -x "${runtime_dir}/bin/${name}" ]; then
    uf_fail "the archive has no bin/${name}" \
      "this build is incomplete — please report it"
  fi
  ln -sfn "${runtime_dir}/bin/${name}" "${bin_dir}/${name}"
done
uf_step "linked" "uf, ufr, ufx into $(uf_tilde "$bin_dir")"

printf '\n' >&2
if [ -z "$uf_colour" ]; then
  printf '  uf %s is ready.\n' "$version" >&2
else
  printf '  \033[1muf %s\033[0m is ready.\n' "$version" >&2
fi

case ":$PATH:" in
  *":${bin_dir}:"*)
    printf '\n' >&2
    uf_note "Run \`uf\` to begin."
    ;;
  *)
    # Not a failure: uf is installed and works by full path. It is one line of
    # shell profile away from working by name, and printing the line is more
    # use than telling the reader that a directory is missing from PATH.
    printf '\n' >&2
    uf_note "$(uf_tilde "$bin_dir") is not on your PATH. Add it:"
    printf '\n' >&2
    if [ -z "$uf_colour" ]; then
      printf '    export PATH="%s:$PATH"\n' "$(uf_home_var "$bin_dir")" >&2
    else
      printf '    \033[1mexport PATH="%s:$PATH"\033[0m\n' "$(uf_home_var "$bin_dir")" >&2
    fi
    ;;
esac
printf '\n' >&2
