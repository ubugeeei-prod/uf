#!/bin/sh
# `nix-first-class.sh` against repositories it can safely make wrong.
#
# Every assertion in that check is a thing that was already true and stopped
# being true without anybody noticing — two flakes that disagreed, a toolchain
# named twice, upstream patches a build silently skipped, a README pointing at
# documentation that did not exist. A guard against those is only worth having
# if it fails on them, and a guard over a repository where they are all absent
# reports success whether or not it works.
#
# So each case below plants exactly one of them in a scratch repository and
# asserts the check goes red, then removes it and asserts the check goes green.
# The pairing matters as much as the failure: a check that is red for every
# input is not a check either.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
script="$repo_root/tools/ci/nix-first-class.sh"

work="$(mktemp -d "${TMPDIR:-/tmp}/uf-test-nix-first-class.XXXXXX")"
trap 'rm -rf "$work"' EXIT INT TERM

fail() {
  echo "test-nix-first-class: FAIL: $*" >&2
  exit 1
}

pass() {
  echo "  ok  $*"
}

run() {
  ( cd "$1" && sh tools/ci/nix-first-class.sh >/dev/null 2>&1 )
}

# A repository shaped like uf's: one flake at the root that reads the toolchain
# file, applies the upstream patches and declares every consumer output; a lock
# covering its inputs; and an install page that documents Nix.
#
# `git ls-files` is what the check reads, so this has to be a real repository
# with real commits.
scratch() {
  root="$work/$1"
  rm -rf "$root"
  mkdir -p "$root/tools/ci" "$root/tools/upstream/patches/flow" \
    "$root/docs/app/guide/install"
  cp "$script" "$root/tools/ci/nix-first-class.sh"

  # A correct repository copies the subtrees its sync checks out, and greets
  # the reader on stderr. Both are facts about *this* pair of files, so the
  # fixture carries a miniature of each.
  cat > "$root/tools/upstream/sync.sh" <<'SYNC'
#!/bin/sh
sparse_subtrees="rust_port lib prelude tslib evals/flow-typed/environment"
SYNC

  cat > "$root/flake.nix" <<'FLAKE'
{
  description = "scratch";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, rust-overlay }:
    let
      rustToolchainFor = pkgs: pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      postPatch = "tools/upstream/patches/flow";
    in
    {
      overlays.default = _final: _prev: { };
      nixosModules.default = { };
      darwinModules.default = { };
      homeManagerModules.default = { };
      packages = { };
      apps = { };
      devShells = { };
      checks = { };
      formatter = { };
      devShellHook = ''
        for subtree in rust_port lib prelude tslib evals; do
          cp -R $subtree .
        done
        shellHook = '\'''
          echo "scratch dev shell" >&2
        '\'';
      '';
    };
}
FLAKE

  cat > "$root/flake.lock" <<'LOCK'
{
  "nodes": {
    "nixpkgs": { "locked": {} },
    "rust-overlay": { "locked": {} },
    "root": { "inputs": { "nixpkgs": "nixpkgs", "rust-overlay": "rust-overlay" } }
  },
  "root": "root",
  "version": 7
}
LOCK

  printf '[toolchain]\nchannel = "nightly-2026-08-01"\n' > "$root/rust-toolchain.toml"
  printf 'use flake .\n' > "$root/.envrc"
  printf 'Enter it with `nix develop .` first.\n' > "$root/README.md"

  cat > "$root/docs/app/guide/install/_uf.page.mdx" <<'DOC'
# Install

## With Nix

Run `nix profile install github:ubugeeei-prod/uf`.

## From source
DOC

  ( cd "$root" \
    && git init -q . \
    && git add -A \
    && git -c user.email=t@e -c user.name=t commit -qm one ) >/dev/null 2>&1
  echo "$root"
}

# The scratch repository has to pass before any of its mutations mean anything.
# If this line fails, every red below is red for the wrong reason.
root="$(scratch clean)"
run "$root" || fail "rejected a repository that has nothing wrong with it"
pass "passes on a well-formed repository"

# 1. Two flakes. This is the arrangement uf actually had: a second flake under
#    `tools/nix` whose shell had drifted from the root's, with nothing saying
#    which one `nix develop` in the README meant.
root="$(scratch two-flakes)"
mkdir -p "$root/tools/nix"
cp "$root/flake.nix" "$root/tools/nix/flake.nix"
( cd "$root" && git add -A && git -c user.email=t@e -c user.name=t commit -qm two ) >/dev/null 2>&1
if run "$root"; then
  fail "accepted a repository with two flakes in it"
fi
pass "rejects a second flake"

# 2. A toolchain named in the flake instead of read from `rust-toolchain.toml`.
#    The exact line that made `nix build` impossible: a stable channel for a
#    workspace that only builds on a pinned nightly.
root="$(scratch hardcoded-toolchain)"
sed 's|pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml|pkgs.rust-bin.stable."1.98.0".default|' \
  "$root/flake.nix" > "$root/flake.nix.new"
mv "$root/flake.nix.new" "$root/flake.nix"
( cd "$root" && git add -A && git -c user.email=t@e -c user.name=t commit -qm hard ) >/dev/null 2>&1
if run "$root"; then
  fail "accepted a flake that names a Rust channel instead of reading the pin"
fi
pass "rejects a hardcoded Rust toolchain"

# 3. But the same text in a comment is allowed, because `flake.nix` has to be
#    able to explain what it used to say. A check that forbade describing the
#    bug would delete the note that stops it coming back.
root="$(scratch toolchain-in-comment)"
printf '  # It used to say `rust-bin.stable."1.98.0"`, which no crate builds on.\n' \
  >> "$root/flake.nix"
( cd "$root" && git add -A && git -c user.email=t@e -c user.name=t commit -qm comment ) >/dev/null 2>&1
run "$root" || fail "rejected a comment that merely mentions a Rust channel"
pass "allows a Rust channel named in a comment"

# 4. A build that does not apply the upstream patches. This one compiles,
#    passes its tests, and answers `uf check` differently from every other
#    build of the same commit — the only failure here that is invisible while
#    it is happening.
root="$(scratch no-patches)"
sed 's|tools/upstream/patches/flow||' "$root/flake.nix" > "$root/flake.nix.new"
mv "$root/flake.nix.new" "$root/flake.nix"
( cd "$root" && git add -A && git -c user.email=t@e -c user.name=t commit -qm nopatch ) >/dev/null 2>&1
if run "$root"; then
  fail "accepted a flake that never applies the upstream Flow patches"
fi
pass "rejects a build that skips the upstream patches"

# 5. A missing consumer output. `overlays.default` is the one somebody putting
#    uf into their own configuration reaches for first, and a flake with only a
#    package and a shell serves this repository and nobody else.
root="$(scratch no-overlay)"
grep -v '^      overlays.default = ' "$root/flake.nix" > "$root/flake.nix.new"
mv "$root/flake.nix.new" "$root/flake.nix"
( cd "$root" && git add -A && git -c user.email=t@e -c user.name=t commit -qm noov ) >/dev/null 2>&1
if run "$root"; then
  fail "accepted a flake with no overlays.default"
fi
pass "rejects a missing consumer output"

# 6. An input declared and not locked. Not a slow drift: every `--locked`
#    consumer, which is every CI job, refuses to evaluate the flake at all.
root="$(scratch unlocked-input)"
sed 's|    nixpkgs.url = .*|    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";\n    flow.url = "github:facebook/flow";|' \
  "$root/flake.nix" > "$root/flake.nix.new"
mv "$root/flake.nix.new" "$root/flake.nix"
( cd "$root" && git add -A && git -c user.email=t@e -c user.name=t commit -qm unlocked ) >/dev/null 2>&1
if run "$root"; then
  fail "accepted an input that is in flake.nix and not in flake.lock"
fi
pass "rejects an unlocked input"

# 7. A pointer at a flake that is not there. Deleting one of two flakes makes
#    every reference to it stale, and the failure is an error about a path
#    rather than about the flake.
root="$(scratch stale-reference)"
printf 'Use `nix develop ./tools/nix` for the pinned environment.\n' >> "$root/README.md"
( cd "$root" && git add -A && git -c user.email=t@e -c user.name=t commit -qm stale ) >/dev/null 2>&1
if run "$root"; then
  fail "accepted a documented flake path that does not exist"
fi
pass "rejects a stale flake reference"

# 8. An install page with no Nix on it, which is what the page said while
#    `README.md` told the reader that Nix was documented there.
root="$(scratch undocumented)"
printf '# Install\n\n## From source\n' > "$root/docs/app/guide/install/_uf.page.mdx"
( cd "$root" && git add -A && git -c user.email=t@e -c user.name=t commit -qm undoc ) >/dev/null 2>&1
if run "$root"; then
  fail "accepted an install page that documents no Nix install path"
fi
pass "rejects an install page with no Nix section"

# 9. A flake that copies only `rust_port`. This is the build that compiles for
#    twenty minutes and then cannot read `lib/core.js`, because `flow_flowlib`
#    embeds Flow's own definitions from directories beside `rust_port` rather
#    than inside it.
root="$(scratch partial-subtrees)"
sed 's|for subtree in rust_port lib prelude tslib evals; do|for subtree in rust_port; do|' \
  "$root/flake.nix" > "$root/flake.nix.new"
mv "$root/flake.nix.new" "$root/flake.nix"
( cd "$root" && git add -A && git -c user.email=t@e -c user.name=t commit -qm partial ) >/dev/null 2>&1
if run "$root"; then
  fail "accepted a flake that copies fewer subtrees than the sync checks out"
fi
pass "rejects a flake that drops a subtree the sync checks out"

# 10. A dev shell that greets the reader on stdout. `nix develop . --command
#     rustc --version` then returns the greeting, and so does every other
#     question anyone asks that shell.
root="$(scratch loud-shell)"
sed 's|echo "scratch dev shell" >&2|echo "scratch dev shell"|' \
  "$root/flake.nix" > "$root/flake.nix.new"
mv "$root/flake.nix.new" "$root/flake.nix"
( cd "$root" && git add -A && git -c user.email=t@e -c user.name=t commit -qm loud ) >/dev/null 2>&1
if run "$root"; then
  fail "accepted a shellHook that writes its banner to stdout"
fi
pass "rejects a dev shell banner on stdout"

echo "test-nix-first-class: ok"
