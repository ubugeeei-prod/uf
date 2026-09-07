#!/bin/sh
# Render `brand/og.html` to `brand/og.png`, at 1200×630.
#
# The card is a source file rather than a binary somebody once exported from a
# design tool: `brand/og.html` is CSS a reader can change, and this turns it
# into the PNG a crawler fetches. Run it after editing that file.
#
# The output is committed. A build should not need a browser to produce an
# image that changes twice a year, and CI has no Chrome — which is also why
# this is a script somebody runs rather than a step in `uf run ci`. What CI
# *can* check is that the two have not drifted, which is
# `tools/brand/test-render-og.sh`.
#
# 1200×630 is Open Graph's own size, and the one every platform states as its
# minimum. Not 2×: a card is displayed at about 500 points wide, the text here
# is 25 points and up, and half a megabyte of PNG buys nothing a reader sees.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
cd "$repo_root"

source_file="brand/og.html"
output="${1:-brand/og.png}"

[ -f "$source_file" ] || {
  echo "render-og: $source_file is missing" >&2
  exit 1
}

# Chrome, because the card is CSS and nothing else on this machine lays out
# CSS. Named in full rather than looked up on PATH: the macOS install is an
# app bundle and puts nothing there.
chrome="${CHROME:-}"
if [ -z "$chrome" ]; then
  for candidate in \
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
    "/Applications/Chromium.app/Contents/MacOS/Chromium" \
    "$(command -v google-chrome 2>/dev/null || true)" \
    "$(command -v chromium 2>/dev/null || true)"
  do
    if [ -n "$candidate" ] && [ -x "$candidate" ]; then
      chrome="$candidate"
      break
    fi
  done
fi
[ -n "$chrome" ] || {
  echo "render-og: no Chrome or Chromium found; set CHROME to one" >&2
  exit 1
}

# `--headless` writes to a path rather than stdout, so the output is named
# here and moved into place only once it exists — a failed render must not
# leave a truncated card in `brand/`.
work="$(mktemp -d "${TMPDIR:-/tmp}/uf-og.XXXXXX")"
trap 'rm -rf "$work"' EXIT

"$chrome" \
  --headless \
  --disable-gpu \
  --hide-scrollbars \
  --force-device-scale-factor=1 \
  --window-size=1200,630 \
  --screenshot="$work/og.png" \
  "file://$repo_root/$source_file" >/dev/null 2>&1

[ -s "$work/og.png" ] || {
  echo "render-og: $chrome produced no image" >&2
  exit 1
}

mv "$work/og.png" "$output"
echo "render-og: wrote $output"
