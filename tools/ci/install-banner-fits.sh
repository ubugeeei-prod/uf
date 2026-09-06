#!/bin/sh
# The installer's banner fits in an eighty-column terminal.
#
# `curl -fsSL https://setup.uniflowed.dev | sh` prints a mark and two lines of
# text before it does anything, and those lines are the first uf a person sees.
# They shared one line until the tagline grew, and
#
#   Unified Toolchain for Flow · Build the strongest React development experience with Modern Flow
#
# is ninety-six columns with its indent. Eighty is the default width of every
# terminal emulator worth naming, so it wrapped mid-sentence onto a second line
# with no indent.
#
# The installer cannot ask how wide the terminal is: `tput` needs a terminfo
# database and a `TERM` that may be `dumb`, `stty` needs a controlling terminal
# that a piped `sh` does not have, and the whole point of the script is to work
# before anything is installed. So the width is a property of the text, and
# this is where that is checked.
#
# It runs the real `uf_brand`, extracted from the real installer, rather than
# measuring string literals a refactor could move: what is being asked is what
# a person sees, not what the source says.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
installer="$repo_root/infra/cloudflare/setup-assets/install.sh"

LIMIT="${UF_BANNER_COLUMNS:-80}"

work="$(mktemp -d "${TMPDIR:-/tmp}/uf-banner.XXXXXX")"
trap 'rm -rf "$work"' EXIT INT TERM

# `uf_brand` and nothing else: the file installs things when it is sourced.
awk '/^uf_brand\(\) \{$/,/^\}$/' "$installer" > "$work/brand.sh"
if [ ! -s "$work/brand.sh" ]; then
  echo "install-banner: could not find uf_brand() in $installer" >&2
  echo "install-banner: if it was renamed, this check has to be told" >&2
  exit 1
fi

widest=0
for colour in "" "1"; do
  (
    # The mark is drawn by these two and is a picture, not text.
    uf_logo_image() { return 0; }
    uf_logo_blocks() { return 0; }
    uf_colour="$colour"
    . "$work/brand.sh"
    uf_brand
  ) 2>"$work/out.$colour" >/dev/null

  # Strip the escape sequences: a bold marker is zero columns wide on screen.
  python3 - "$work/out.$colour" "$LIMIT" <<'PY' || exit 1
import re, sys
text = open(sys.argv[1], encoding="utf8").read()
limit = int(sys.argv[2])
bad = []
widest = 0
for line in text.split("\n"):
    plain = re.sub(r"\x1b\[[0-9;]*m", "", line)
    widest = max(widest, len(plain))
    if len(plain) > limit:
        bad.append((len(plain), plain))
print(f"  widest {widest} columns")
for width, line in bad:
    print(f"  {width} columns: {line}", file=sys.stderr)
if bad:
    print(f"\nThe installer's banner does not fit in {limit} columns.", file=sys.stderr)
    print("It is the first thing anybody sees from `curl … | sh`, and a line", file=sys.stderr)
    print("wider than the terminal wraps without its indent.", file=sys.stderr)
    sys.exit(1)
PY
done

echo "install-banner: fits in $LIMIT columns"
