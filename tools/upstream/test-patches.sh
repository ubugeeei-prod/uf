#!/bin/sh
# `sync.sh`'s patch step, against the ways a patch can go missing.
#
# The step exists because a patch to a type checker that is quietly not applied
# is the worst failure this repository has: nothing fails to compile, no test
# goes red, and `uf check` answers differently than it did yesterday. So what
# is worth testing is not that the patches in `tools/upstream/patches/flow`
# apply — the sync proves that in every job — but that each way one could be
# skipped is refused, and named precisely enough that the reader knows which
# file to open.
#
# It runs against a scratch superproject with a scratch submodule rather than
# against `upstream/flow`, so it is hermetic, costs about a second, and can
# make a patch fail on purpose without touching a 40 MB checkout.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
script="$repo_root/tools/upstream/sync.sh"

work="$(mktemp -d "${TMPDIR:-/tmp}/uf-test-upstream-patches.XXXXXX")"
trap 'rm -rf "$work"' EXIT INT TERM

# A submodule cloned from a path needs this, and `git submodule update` clones
# in a child process — the environment form is the one children inherit.
GIT_CONFIG_COUNT=1
GIT_CONFIG_KEY_0=protocol.file.allow
GIT_CONFIG_VALUE_0=always
export GIT_CONFIG_COUNT GIT_CONFIG_KEY_0 GIT_CONFIG_VALUE_0

fail() {
  echo "test-upstream-patches: FAIL: $*" >&2
  exit 1
}

pass() {
  echo "  ok  $*"
}

# A repository shaped like `facebook/flow` as far as the sync can tell: the
# files it asserts are present, and one it can be asked to patch.
fake_flow() {
  upstream="$work/flow.git"
  rm -rf "$upstream"
  mkdir -p "$upstream/rust_port/crates" "$upstream/lib" "$upstream/prelude" \
    "$upstream/tslib" "$upstream/evals/flow-typed/environment"
  echo '[workspace]' > "$upstream/rust_port/Cargo.toml"
  echo 'fn dispatch() -> &str { "the bound" }' > "$upstream/rust_port/crates/dispatch.rs"
  for f in lib/core.js lib/react.js prelude/prelude.js tslib/index.js \
    evals/flow-typed/environment/dom.js \
    evals/flow-typed/environment/bom.js \
    evals/flow-typed/environment/node.js
  do
    echo '// upstream' > "$upstream/$f"
  done
  git -C "$upstream" init --quiet
  git -C "$upstream" config user.email test@example.com
  git -C "$upstream" config user.name test
  git -C "$upstream" config uploadpack.allowfilter true
  git -C "$upstream" add -A
  git -C "$upstream" commit --quiet -m upstream
}

# ... and a superproject shaped like this one: the sync, a patch directory,
# and the submodule at `upstream/flow`.
scratch() {
  root="$work/repo"
  rm -rf "$root"
  mkdir -p "$root/tools/upstream/patches/flow"
  git -C "$root" init --quiet
  git -C "$root" config user.email test@example.com
  git -C "$root" config user.name test
  cp "$script" "$root/tools/upstream/sync.sh"
  git -C "$root" submodule --quiet add "$work/flow.git" upstream/flow
  git -C "$root" add -A
  git -C "$root" commit --quiet -m scratch
}

# The patch every case below starts from: one hunk, one file, a header that
# says what it is for.
write_patch() {
  cat > "$root/tools/upstream/patches/flow/$1" <<'PATCH'
Subject: dispatch returns the type variable rather than its bound

Issue: ubugeeei-prod/uf#205
Upstream: not filed yet

The scratch stand-in for a real fix to the port.

diff --git a/rust_port/crates/dispatch.rs b/rust_port/crates/dispatch.rs
--- a/rust_port/crates/dispatch.rs
+++ b/rust_port/crates/dispatch.rs
@@ -1 +1 @@
-fn dispatch() -> &str { "the bound" }
+fn dispatch() -> &str { "the type variable" }
PATCH
}

run() {
  set +e
  out="$(cd "$root" && ./tools/upstream/sync.sh 2>&1)"
  status=$?
  set -e
}

patched() {
  grep -q 'the type variable' "$root/upstream/flow/rust_port/crates/dispatch.rs"
}

# A refusal that does not name the file is a refusal the reader has to guess
# from, which for a directory of near-identical patches is no help at all.
refuses() {
  name="$1"
  needle="$2"
  [ "$status" -ne 0 ] || fail "$name was accepted: $out"
  case "$out" in
    *"$needle"*) ;;
    *) fail "$name is refused without naming \`$needle\`: $out" ;;
  esac
  pass "$name"
}

fake_flow

# --- a patch is applied, and saying so is part of applying it ----------------
scratch
write_patch 0001-dispatch.patch
run
[ "$status" -eq 0 ] || fail "a good patch was refused: $out"
patched || fail "the patch was not applied: $out"
case "$out" in
  *"1 patch(es)"*) ;;
  *) fail "the sync does not say how many patches it applied: $out" ;;
esac
pass "a patch is applied, and the sync says how many"

# --- and re-running neither re-applies it nor rewrites the file --------------
# The mtime is the assertion, not a detail of it: rewriting a file cargo has
# already compiled rebuilds the port, and the port is most of the build. A
# patch step that costs a full rebuild per sync would be turned off.
before="$(ls -l "$root/upstream/flow/rust_port/crates/dispatch.rs")"
run
[ "$status" -eq 0 ] || fail "the second sync failed: $out"
patched || fail "the second sync dropped the patch: $out"
[ "$before" = "$(ls -l "$root/upstream/flow/rust_port/crates/dispatch.rs")" ] ||
  fail "the second sync rewrote an already-patched file, which rebuilds the port"
pass "re-running leaves an applied patch, and its mtime, alone"

# --- deleting the file removes the patch -------------------------------------
# The whole documented procedure for a fix that landed upstream. If the work
# tree kept carrying it, uf would be building against a patch no longer in the
# repository, which is the same invisibility from the other direction.
rm "$root/tools/upstream/patches/flow/0001-dispatch.patch"
run
[ "$status" -eq 0 ] || fail "the sync failed after a patch was deleted: $out"
patched && fail "a deleted patch is still in the work tree: $out"
pass "deleting the file un-applies the patch on the next sync"

# --- the pin moves under a patched work tree ---------------------------------
# `git submodule update` checks a moved pin out with a plain `git checkout`,
# which refuses to overwrite a modified file — so a bump would otherwise fail
# on the patched files, with a message about local changes and nothing about
# patches. The sync has to unpatch before it checks out and patch after.
scratch
write_patch 0001-dispatch.patch
run
[ "$status" -eq 0 ] || fail "the first sync failed: $out"
echo '// upstream, later' > "$upstream/lib/core.js"
git -C "$upstream" commit --quiet -am 'a commit that does not touch the patched file'
bumped="$(git -C "$upstream" rev-parse HEAD)"
git -C "$root" update-index --cacheinfo "160000,$bumped,upstream/flow"
run
[ "$status" -eq 0 ] || fail "a bump under a patched work tree failed: $out"
patched || fail "the bump dropped the patch: $out"
[ "$(git -C "$root/upstream/flow" rev-parse HEAD)" = "$bumped" ] ||
  fail "the bump did not take: $out"
pass "a submodule bump unpatches, checks out, and patches again"

# --- and takes the patch with it when the fix has landed ---------------------
# The day this mechanism is supposed to end. The bump brings a commit that
# already has the fix, the patch stops applying, and the sync says so instead
# of building against a port nobody has read.
echo 'fn dispatch() -> &str { "the type variable" }' > "$upstream/rust_port/crates/dispatch.rs"
git -C "$upstream" commit --quiet -am 'upstream takes the fix'
landed="$(git -C "$upstream" rev-parse HEAD)"
git -C "$root" update-index --cacheinfo "160000,$landed,upstream/flow"
run
refuses "a bump onto a commit that already has the fix" "has landed upstream"
case "$out" in
  *"git rm "*) ;;
  *) fail "the refusal does not say to delete the patch: $out" ;;
esac
pass "and says to delete the patch rather than to fix it"

# The two cases above moved the stand-in upstream forward; put it back, so the
# cases below start from the same commit the ones above did.
fake_flow

# --- a patch that does not apply stops everything ----------------------------
scratch
write_patch 0001-dispatch.patch
# The context the patch expects is gone, the way it would be after a bump onto
# a commit that rewrote the function — or took the fix.
sed -i.bak 's/the bound/something else/' "$root/upstream/flow/rust_port/crates/dispatch.rs"
rm -f "$root/upstream/flow/rust_port/crates/dispatch.rs.bak"
run
refuses "a patch that no longer applies" "0001-dispatch.patch"
case "$out" in
  *"ubugeeei-prod/uf#205"*) ;;
  *) fail "the failure does not name the issue the patch is for: $out" ;;
esac
case "$out" in
  *"landed upstream"*) ;;
  *) fail "the failure does not say a patch can be deleted rather than fixed: $out" ;;
esac
pass "the failure names the issue and both ways out"

# --- and leaves nothing half-applied -----------------------------------------
# Two patches, the second broken. The first must not survive: a work tree that
# is the pinned commit plus *some* of the patches is the state nothing else in
# this repository could detect.
scratch
write_patch 0001-dispatch.patch
cat > "$root/tools/upstream/patches/flow/0002-broken.patch" <<'PATCH'
Subject: a hunk with no matching context

Issue: ubugeeei-prod/uf#205

diff --git a/rust_port/crates/dispatch.rs b/rust_port/crates/dispatch.rs
--- a/rust_port/crates/dispatch.rs
+++ b/rust_port/crates/dispatch.rs
@@ -1 +1 @@
-fn dispatch() -> &str { "a line that is not there" }
+fn dispatch() -> &str { "unreachable" }
PATCH
run
[ "$status" -ne 0 ] || fail "a broken second patch was accepted: $out"
patched && fail "the first patch was left applied after the second failed: $out"
pass "a failure leaves the work tree unpatched, not half-patched"

# --- a file that is not named like a patch is refused, not ignored -----------
# The case the whole design turns on. `0001-dispatch.patch.orig`, `fix.diff`,
# `0001-dispatch.patch.rej` — every one of them sits in the directory looking
# like a patch and would never be applied.
scratch
write_patch 0001-dispatch.patch
cp "$root/tools/upstream/patches/flow/0001-dispatch.patch" \
  "$root/tools/upstream/patches/flow/fix.diff"
run
refuses "a file in the directory that is not NNNN-*.patch" "fix.diff"

scratch
write_patch 0001-dispatch.patch
cp "$root/tools/upstream/patches/flow/0001-dispatch.patch" \
  "$root/tools/upstream/patches/flow/0002-dispatch.patch.orig"
run
refuses "an editor backup left beside a patch" "0002-dispatch.patch.orig"

# --- a patch with no issue is refused ----------------------------------------
# It is the line that lets the next person delete it. Without one, a patch
# outlives the bug it was written for and nobody can prove it.
scratch
write_patch 0001-dispatch.patch
grep -v '^Issue:' "$root/tools/upstream/patches/flow/0001-dispatch.patch" \
  > "$work/anon" && mv "$work/anon" "$root/tools/upstream/patches/flow/0001-dispatch.patch"
run
refuses "a patch with no \`Issue:\` line" "Issue:"

# --- and one git cannot read ------------------------------------------------
scratch
cat > "$root/tools/upstream/patches/flow/0001-prose.patch" <<'PATCH'
Subject: notes, not a diff

Issue: ubugeeei-prod/uf#205

I meant to write the diff here and did not.
PATCH
run
refuses "a patch file with no diff in it" "0001-prose.patch"

# --- README.md is the one thing that may sit beside them ---------------------
scratch
write_patch 0001-dispatch.patch
echo '# Patches' > "$root/tools/upstream/patches/flow/README.md"
run
[ "$status" -eq 0 ] || fail "the README beside the patches was refused: $out"
patched || fail "the README stopped the patch from applying: $out"
pass "README.md is the one file that may sit beside the patches"

# --- an empty directory is a sync with no patch step -------------------------
# Where this repository is on the day the last patch lands upstream, and the
# state the step has to stay silent and cheap in.
scratch
run
[ "$status" -eq 0 ] || fail "a sync with no patches failed: $out"
case "$out" in
  *"patch(es)"*) fail "a sync with no patches talks about patches: $out" ;;
  *) ;;
esac
pass "no patches is a quiet sync"

echo "test-upstream-patches: every case passed"
