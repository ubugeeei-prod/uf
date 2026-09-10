#!/bin/sh
# The Nix flake is one flake, reads one toolchain pin, and is pointed at by
# everything that mentions it.
#
# Nix was *present* in this repository for a long time before it worked. There
# were two flakes — this one and a second under `tools/nix/` — with nothing
# saying which a contributor should use; `.envrc` used one, `formal.yml` used
# one, and `README.md` told you to run `nix develop`, which is the other. Both
# named `rust-bin.stable."1.98.0"` for a workspace that `rust-toolchain.toml`
# pins to `nightly-2026-08-01`, and the pin is not a preference: 23 crates in
# Meta's Flow Rust port declare `#![feature(box_patterns)]`, which no stable
# compiler accepts at all. So `nix build .#uf` could not produce a binary, and
# `nix develop` handed you a shell in which the very next line of the README —
# `cargo build --release --bin uf` — cannot succeed.
#
# None of that was noticed for the same reason: nothing ran it. The only Nix in
# CI was the `Formal` job entering a shell to run `why3`, which never builds a
# crate. `.github/workflows/nix.yml` now builds the package and asserts the
# shell's compiler, which is the check that would have caught it.
#
# This is the cheap half of the same guard. Everything below is a property of
# the *files*, needs no Nix installed and takes milliseconds, so it can gate
# every pull request in `Metadata` rather than only the ones that touch the
# flake. It is the difference between the flake being correct today and it
# staying correct: each assertion here is a way the arrangement above silently
# came apart once already.
#
# What this cannot check is that the package builds. Only a build can say that,
# it costs half an hour, and it lives in `nix.yml`.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
cd "$repo_root"

flake="flake.nix"
lock="flake.lock"
toolchain_file="rust-toolchain.toml"
install_doc='docs/app/guide/install/$page.mdx'

errors=0
fail() {
  printf '  FAIL  %s\n' "$1" >&2
  errors=$((errors + 1))
}
pass() {
  printf '  ok    %s\n' "$1"
}

# 1. One flake, and it is at the root.
#
# Two flakes is the trap this started as. They drift — the one that was deleted
# had `nixpkgs-fmt` this one did not and lacked the OpenTofu this one has — and
# a contributor reading `nix develop` in the README has no way to know which
# shell they are in.
flakes="$(git ls-files '*flake.nix')"
flake_count="$(printf '%s\n' "$flakes" | grep -c . || true)"
if [ "$flake_count" -ne 1 ]; then
  fail "expected exactly one flake.nix, found $flake_count:"
  printf '%s\n' "$flakes" | sed 's/^/          /' >&2
  printf '\n        A second flake is a second dev shell to keep correct, and\n' >&2
  printf '        nothing in the repository says which one a contributor is\n' >&2
  printf '        meant to enter. Put the outputs in %s.\n\n' "$flake" >&2
elif [ "$flakes" != "$flake" ]; then
  fail "the only flake is $flakes, not $flake — \`nix develop .\` would not find it"
else
  pass "one flake, at the repository root"
fi

# 2. No .nix file names a Rust toolchain version of its own.
#
# This is the bug itself, written as a pattern. A literal version in a .nix
# file is a second answer to a question `rust-toolchain.toml` already answers,
# and the second answer is the one that was wrong for months.
if [ "$flake_count" -ge 1 ]; then
  # Comment lines are exempt: `flake.nix` explains what it used to say and why
  # that was wrong, and a check that forbade describing the bug would forbid
  # the note that stops somebody reintroducing it.
  # `-H` because grep omits the filename when it is handed exactly one file,
  # and there is exactly one .nix file here — which would leave the filter
  # below with `45:  # ...` and no name to anchor against.
  hardcoded="$(git ls-files '*.nix' \
    | xargs grep -Hn 'rust-bin\.\(stable\|beta\|nightly\)\.' 2>/dev/null \
    | grep -v '^[^:]*:[0-9]*: *#' || true)"
  if [ -n "$hardcoded" ]; then
    fail "a .nix file names a Rust toolchain instead of reading $toolchain_file:"
    printf '%s\n' "$hardcoded" | sed 's/^/          /' >&2
    printf '\n        Use `rust-bin.fromRustupToolchainFile ./%s`. That file is\n' "$toolchain_file" >&2
    printf '        what rustup, `dtolnay/rust-toolchain` in CI and every\n' >&2
    printf '        contributor already obey; naming a channel here makes the\n' >&2
    printf '        flake the one build that disagrees with them.\n\n' >&2
  else
    pass "no .nix file names a Rust toolchain"
  fi

  # 3. And the positive half: it actually reads the pin. Absence of a literal
  #    is also what a flake with no Rust in it at all looks like.
  if grep -q 'fromRustupToolchainFile' "$flake"; then
    pass "the flake reads $toolchain_file"
  else
    fail "$flake does not read $toolchain_file"
  fi

  # 4. The upstream patches are applied by the build.
  #
  #    `tools/upstream/sync.sh` defines a checkout of the Flow port as the
  #    pinned commit *plus* `tools/upstream/patches/flow/`, on every machine and
  #    in every job. The flake used to symlink the pinned commit and stop
  #    there. That build compiles and passes its tests and answers `uf check`
  #    differently from every other build of the same commit, which is the one
  #    failure in this repository that is invisible while it is happening.
  if grep -q 'tools/upstream/patches/flow' "$flake"; then
    pass "the flake applies the upstream Flow patches"
  else
    fail "$flake does not apply tools/upstream/patches/flow"
    printf '\n        A Nix build without them is the pinned commit and nothing\n' >&2
    printf '        else. It will compile, and it will typecheck Flow\n' >&2
    printf '        differently from every other build of this commit. See\n' >&2
    printf '        tools/upstream/patches/flow/README.md.\n\n' >&2
  fi

  # 5. The outputs a consumer who is not this repository needs.
  #
  #    A package and a shell serve this repository. An overlay and a module are
  #    what somebody putting uf into their own system configuration reaches
  #    for, and their absence is why "we have a flake" and "Nix is supported"
  #    were not the same sentence.
  for output in \
    'packages' \
    'apps' \
    'devShells' \
    'checks' \
    'formatter' \
    'overlays.default' \
    'nixosModules.default' \
    'darwinModules.default' \
    'homeManagerModules.default'
  do
    if grep -q "^      $output = " "$flake"; then
      pass "declares $output"
    else
      fail "$flake declares no $output"
    fi
  done
fi

# 6. Every input in the flake is locked.
#
# An input added without regenerating the lock is not a slow drift, it is an
# immediate hard stop: `nix build` in any `--locked` context, which is every CI
# job and every consumer pinning this flake, refuses to start.
if [ -f "$lock" ] && [ -f "$flake" ]; then
  inputs="$(sed -n '/^  inputs = {/,/^  };/p' "$flake" \
    | sed -n 's/^    \([a-zA-Z0-9_-]*\)[ .=].*/\1/p' \
    | sort -u)"
  missing=""
  for input in $inputs; do
    grep -q "\"$input\":" "$lock" || missing="$missing $input"
  done
  if [ -n "$missing" ]; then
    fail "declared in $flake and absent from $lock:$missing"
    printf '\n        Run `nix flake lock` and commit the result. Until then\n' >&2
    printf '        every `--locked` consumer refuses to evaluate the flake.\n\n' >&2
  else
    pass "every flake input is locked"
  fi
else
  fail "$lock or $flake is missing"
fi

# 7. Everything that points at a local flake points at one that exists.
#
# `.envrc` and `formal.yml` both named `./tools/nix` while the README named the
# root. Two of those three were about to be wrong whichever flake was deleted,
# and a stale `nix develop ./somewhere` fails with an error about a path rather
# than about the flake.
refs="$(git ls-files \
  | grep -v '^tools/ci/nix-first-class.sh$' \
  | grep -v '^tools/ci/test-nix-first-class.sh$' \
  | xargs grep -ho 'nix \(develop\|build\|flake [a-z]*\) \.[^ "]*' 2>/dev/null \
  | sed 's/.* //' | sort -u)"
refs="$refs
$(sed -n 's/^use flake \(.*\)$/\1/p' .envrc 2>/dev/null || true)"
bad=""
for ref in $refs; do
  # These are quoted out of prose as often as out of a shell, so strip the
  # punctuation that ends a sentence or closes a Markdown code span.
  ref="$(printf '%s' "$ref" | sed 's/[`",.)]*$//')"
  [ -n "$ref" ] || continue

  # Only local paths. `github:owner/repo` in the docs is somebody else's to
  # resolve, and `tools/ci/links-resolve.sh` is what reads those.
  case "$ref" in
    .) dir="." ;;
    ./*) dir="${ref%/}" ;;
    *) continue ;;
  esac
  # A flake reference can name an output, `.#uf` — the flake is the part
  # before the `#`.
  dir="${dir%%#*}"
  [ -n "$dir" ] || dir="."
  if [ ! -f "$dir/flake.nix" ]; then
    bad="$bad $ref"
  fi
done
if [ -n "$bad" ]; then
  fail "these point at a directory with no flake.nix:$bad"
else
  pass "every local flake reference resolves"
fi

# 8. The install page documents Nix.
#
# `README.md` says, in as many words, that Nix is on the install page. It was
# not: that page had "From a release" and "From source" and no third heading,
# so the one pointer to the Nix instructions led to a page that did not have
# them. A supported install path with no documentation is a supported install
# path nobody can use.
if [ -f "$install_doc" ]; then
  if grep -qi '^## .*nix' "$install_doc"; then
    pass "the install page has a Nix section"
  else
    fail "$install_doc has no Nix section, and README.md points at it for one"
  fi
  if grep -q 'nix profile install\|nix run\|overlays.default' "$install_doc"; then
    pass "the install page shows how to install from the flake"
  else
    fail "$install_doc names Nix but shows no way to install with it"
  fi
else
  fail "$install_doc is missing"
fi

# The flake copies what the sync checks out.
#
# `flow_flowlib` reaches *outside* `rust_port` with `include_str!` — Flow's own
# library definitions live in `lib/`, `prelude/` and `tslib/`, and the globals
# in `evals/flow-typed/environment` are in none of those. `sync.sh` names all
# five in `sparse_subtrees` and says why; the flake's `postPatch` has to bring
# the same ones or the build stops at the first crate that embeds a libdef,
# with `couldn't read .../lib/core.js` after twenty minutes of compiling.
#
# Compared by top-level name: the sync sparse-checks out a path
# (`evals/flow-typed/environment`), the flake copies the directory it is under.
sync="tools/upstream/sync.sh"
if [ -f "$sync" ]; then
  sync_subtrees="$(
    sed -n 's/^sparse_subtrees="\(.*\)"$/\1/p' "$sync" |
      tr ' ' '\n' | sed 's|/.*||' | sort -u | tr '\n' ' '
  )"
  flake_subtrees="$(
    sed -n 's/^ *for subtree in \(.*\); do$/\1/p' "$flake" |
      tr ' ' '\n' | sed 's|/.*||' | sort -u | tr '\n' ' '
  )"
  if [ -z "$sync_subtrees" ]; then
    fail "$sync no longer declares sparse_subtrees, so nothing can be compared to it"
  elif [ -z "$flake_subtrees" ]; then
    fail "$flake no longer copies a list of subtrees in postPatch"
  elif [ "$sync_subtrees" = "$flake_subtrees" ]; then
    pass "the flake copies the subtrees the sync checks out ($flake_subtrees)"
  else
    fail "$flake copies [$flake_subtrees] and $sync checks out [$sync_subtrees]"
  fi
else
  fail "$sync is missing"
fi

# A dev shell answers questions; a banner on stdout is not an answer.
#
# `nix develop . --command rustc --version` returned "uniflowed dev shell: Rust
# 1.99.0-nightly, ..." because the hook echoed to stdout, and the Dev shell job
# read its own banner back as a compiler version and failed a correct flake.
if grep -q 'shellHook' "$flake"; then
  loud="$(
    sed -n "/shellHook = /,/''/p" "$flake" | grep '^ *echo ' | grep -cv '>&2' || :
  )"
  if [ "${loud:-0}" -ne 0 ]; then
    fail "$flake has $loud shellHook echo(es) on stdout; send the banner to stderr with >&2"
  else
    pass "the dev shell banner stays off stdout"
  fi
fi

if [ "$errors" -ne 0 ]; then
  printf '\nnix-first-class: %s failed\n' "$errors" >&2
  exit 1
fi

printf '\nnix-first-class: ok\n'
