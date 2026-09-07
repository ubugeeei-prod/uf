#!/bin/sh
# The CI integrations set only environment variables the installer reads.
#
# `integrations/` holds three files that install `uf` by running the same
# script `curl -fsSL https://setup.uniflowed.dev | sh` runs, configured
# entirely through environment variables. Nothing else connects them: the
# installer is shell, the integrations are YAML for three different systems,
# and no compiler, linter or test sees both.
#
# So a rename in `install.sh` — `UF_BIN_DIR` to `UF_BINDIR`, say — leaves three
# integrations passing a variable nothing reads. The installer then does what
# it does with no configuration at all: it installs into `$HOME/.local/bin`,
# succeeds, and the integration's next step puts a directory that holds nothing
# onto `PATH`. The failure surfaces as `uf: command not found` in somebody
# else's pipeline, on a system this repository has no job for.
#
# This is the cheapest check that would have caught it: read the variable names
# out of the installer, read the ones the integrations set, and require the
# second set to be contained in the first.
#
# It is deliberately one-directional. The installer may read a variable no
# integration sets — most of them are for a human — but an integration that
# sets one the installer does not read is always a mistake.
#
# `UF_CI_*` is the exception, and it is a namespace rather than a waiver: an
# integration has settings of its own — where to fetch the installer from,
# whether to install `curl` first — and GitLab has nowhere but the environment
# to put them. Splitting the prefixes is what keeps "the installer ignored my
# variable" from looking the same as "this template named its own".
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
cd "$repo_root"

installer="infra/cloudflare/setup-assets/install.sh"
integrations="integrations/github-actions/action.yml
integrations/gitlab/uf.gitlab-ci.yml
integrations/circleci/orb.yml"

status=0

fail() {
  echo "integrations-agree: FAIL: $*" >&2
  status=1
}

[ -f "$installer" ] || {
  fail "the installer is not at $installer"
  exit 1
}

# `${UF_X:-…}` and `${UF_X}` and `$UF_X`, which is every way the installer
# names one. Sorted and de-duplicated so the comparison below is a plain
# membership test.
honoured="$(
  sed -n 's/.*\${\{0,1\}\(UF_[A-Z0-9_]*\).*/\1/p' "$installer" |
    sort -u
)"

[ -n "$honoured" ] || {
  fail "found no UF_* variables in $installer — has it been rewritten?"
  exit 1
}

echo "installer honours:"
echo "$honoured" | sed 's/^/  /'

for file in $integrations; do
  [ -f "$file" ] || {
    fail "$file is missing"
    continue
  }
  # Every `UF_NAME:` key and every `UF_NAME=` assignment in the YAML. The
  # trailing `:` or `=` is what distinguishes a variable being *set* from one
  # merely being named in a comment, which several of them are.
  # `UF_CI_*` is the integration's own namespace; everything else under `UF_`
  # is the installer's and has to be one it reads.
  used="$(
    sed -n 's/.*[^A-Z_]\{0,1\}\(UF_[A-Z0-9_]*\)[[:space:]]*[:=].*/\1/p' "$file" |
      grep -v '^UF_CI_' |
      sort -u || true
  )"
  for name in $used; do
    if echo "$honoured" | grep -qx "$name"; then
      echo "  ok  $file sets $name"
    else
      fail "$file sets $name, which $installer does not read"
    fi
  done
done

# The other half of the contract. Each integration decides whether a restored
# cache is usable by running the binary in it, because a key that matched
# proves only that a key matched: a cache that lost a symlink, or one restored
# onto an architecture the key did not distinguish, is a hit that can run
# nothing. An integration that skipped that would install nothing and put an
# empty directory on `PATH`.
for file in $integrations; do
  grep -q -- '--version' "$file" ||
    fail "$file never runs \`uf --version\`, so it cannot tell a broken cache from a good one"
done

if [ "$status" -eq 0 ]; then
  echo "integrations-agree: ok"
fi
exit "$status"
