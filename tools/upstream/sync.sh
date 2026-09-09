#!/bin/sh
# Materialize the upstream sources uf builds against.
#
# Without arguments: `upstream/flow`, which every cargo invocation needs.
# With `--integrations`: also the repositories in `tools/upstream/repos.txt`,
# which nothing in the cargo graph depends on yet.
#
# `uf_flow`'s `upstream-parser` feature builds against Meta's official Flow Rust
# port, which is not published to crates.io. The submodule tracks the whole Flow
# repository (~190 MB), but only `rust_port/` is ever compiled, so this script
# fetches a single shallow commit without blobs and checks out just that
# subtree — roughly 40 MB instead of 190 MB.
#
# Every cargo invocation in this workspace needs the submodule present, because
# Cargo resolves path dependencies even when the feature that uses them is off.
#
# After the checkout, `tools/upstream/patches/flow/` is applied on top — see
# `apply_patches` below and that directory's README. A checkout is the pinned
# commit *plus* those patches, always, on every machine and in every CI job.
set -eu

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"

submodule_path="upstream/flow"
patch_dir="tools/upstream/patches/flow"

# `rust_port` holds the crates, but several of them reach outside it with
# `include_str!`: `flow_flowlib` embeds Flow's own library definitions from
# `lib/`, `prelude/`, and `tslib/`. Leaving those out builds fine until the day
# someone enables the type checker, then fails with an unhelpful missing-file
# error from a macro. Check them out up front.
#
# `evals/flow-typed/environment` is where Flow keeps the DOM, BOM and Node
# globals. They are not in `lib/` — that holds only `core.js` and `react.js` —
# and without them a type checker knows what a `Map` is and not what a
# `document` is. `uf_check` embeds them from there.
sparse_subtrees="rust_port lib prelude tslib evals/flow-typed/environment"

# Applying a patch leaves the submodule's work tree modified, and `git
# submodule update` checks a moved pin out with a plain `git checkout`, which
# refuses to overwrite a modified file. A bump would therefore fail on the
# patched files with a message about local changes rather than about patches.
#
# Two cheap reads say whether the pin moved; `restore_submodule` below is what
# makes the checkout possible, and the patch step puts the patches back on the
# new commit — or says which one stopped applying, which is exactly the report
# a bump wants.
restore_submodule() {
  # Only the sparse cone is in the work tree, and `checkout -- .` restores
  # what is there rather than materializing what is not. It is a stat walk
  # over an index that is already warm: about 20 ms on this checkout.
  git -C "$submodule_path" checkout --quiet -- . 2>/dev/null || :
  # And the files a patch *added*, which `checkout` leaves behind because
  # they are untracked. Without `-x`, so a `target/` somebody built inside
  # the submodule survives: this restores the port, not the disk.
  git -C "$submodule_path" clean --quiet --force -d 2>/dev/null || :
}

pinned=$(git ls-files --stage -- "$submodule_path" | awk '$1 == "160000" { print $2 }')
current=$(git -C "$submodule_path" rev-parse HEAD 2>/dev/null || echo '')
if [ -n "$current" ] && [ -n "$pinned" ] && [ "$current" != "$pinned" ]; then
  restore_submodule
fi

git submodule update --init --depth 1 --filter=blob:none "$submodule_path"
# shellcheck disable=SC2086 # deliberate word splitting: one argument per subtree
git -C "$submodule_path" sparse-checkout set $sparse_subtrees

for required in \
  "rust_port/Cargo.toml" \
  "lib/core.js" \
  "lib/react.js" \
  "prelude/prelude.js" \
  "evals/flow-typed/environment/dom.js" \
  "evals/flow-typed/environment/bom.js" \
  "evals/flow-typed/environment/node.js"
do
  if [ ! -f "$submodule_path/$required" ]; then
    echo "upstream sync failed: $submodule_path/$required is missing" >&2
    exit 1
  fi
done

if [ ! -d "$submodule_path/tslib" ]; then
  echo "upstream sync failed: $submodule_path/tslib is missing" >&2
  exit 1
fi

# --- patches ------------------------------------------------------------------
#
# `tools/upstream/patches/flow/` holds fixes to the port that uf needs and
# `facebook/flow` has not taken yet. They are applied here, so a checkout is
# the pinned commit *plus* those patches — on a laptop, in every CI job, and
# after `uf run setup`, with no way to end up with one and not the other.
#
# The rules this step is built around, in the order they matter:
#
# 1. **Nothing is skipped quietly.** Every `*.patch` in the directory is
#    applied. There is no list of patches to apply, because a list is a second
#    place to forget one; the directory *is* the list. A patch that does not
#    apply stops the sync, and therefore stops the build, and therefore stops
#    every job. A dropped patch to a type checker would otherwise be invisible:
#    the compiler is happy, the tests are green, and the answers are wrong.
# 2. **Idempotent.** A patch already in the work tree is left alone. The check
#    is `git apply --reverse --check`, which asks the tree rather than a stamp
#    file, so it cannot say "applied" about a tree that is not.
# 3. **Cheap when there is nothing to do.** Leaving an applied patch alone
#    means not rewriting the file, which means not giving it a new mtime,
#    which means cargo does not rebuild the port. That is the whole reason
#    this is not simply "restore the tree and re-apply everything": correct,
#    and it would recompile the port on every sync.
#
# Removing a patch is deleting the file. The next sync notices the work tree
# still carries it, restores the tree, and re-applies what is left — and when
# a bump brings a commit that already contains a patch, the sync says so rather
# than leaving it here applying to nothing. Refreshing one after a bump is
# `git -C upstream/flow diff`; the README in that directory is the contract,
# and ubugeeei-prod/uf#707 is why any of this exists.
patch_count=0
for entry in "$patch_dir"/*; do
  [ -e "$entry" ] || continue # the unmatched glob, when there are no patches
  case "${entry##*/}" in
    README.md) continue ;;
    # Four digits, so the order is the filename and is the same in every
    # locale. A patch that depends on an earlier one sorts after it.
    [0-9][0-9][0-9][0-9]-*.patch) ;;
    *)
      cat >&2 <<EOF
upstream sync failed: $entry is not a patch this step would apply.

Files in $patch_dir are named NNNN-what-it-does.patch, and everything that is
not README.md is applied in that order. A file with another name would sit
next to the patches looking like one and never be applied, which is the single
failure this step exists to prevent, so it is refused instead.
EOF
      exit 1
      ;;
  esac

  # Reviewability, made mechanical: a patch to somebody else's type checker
  # has to say whose bug it is, or the next person to read it cannot tell a
  # fix that has landed upstream from one that has not.
  if ! grep -q '^Issue:[[:space:]]*[^[:space:]]' "$entry"; then
    cat >&2 <<EOF
upstream sync failed: $entry has no \`Issue:\` line.

Every patch carries one in the header above the diff, naming the issue it
exists for, so that it can be deleted with confidence when that issue closes.
See $patch_dir/README.md.
EOF
    exit 1
  fi

  # A patch git cannot even parse would otherwise surface as a raw git error
  # from the middle of the step below, where it reads as a broken sync rather
  # than as a broken file.
  if ! git apply --numstat -- "$entry" > /dev/null 2>&1; then
    cat >&2 <<EOF
upstream sync failed: $entry is not a diff git can read.

Patches here are what \`git -C $submodule_path diff\` writes, with a header
above them. See $patch_dir/README.md.
EOF
    exit 1
  fi

  patch_count=$((patch_count + 1))
done

work=$(mktemp -d "${TMPDIR:-/tmp}/uf-upstream-patch.XXXXXX")
trap 'rm -rf "$work"' EXIT INT TERM

# What the patches claim, against what the work tree actually carries. Anything
# in the second that is not in the first is a patch that used to be here and
# was deleted, or a hand edit — either way the tree is no longer the pinned
# commit plus this directory, and the honest repair is to rebuild it from both.
#
# This runs when the directory is *empty* too, and that case is the whole
# reason it is not folded into the loop below: deleting the file is the
# documented way to remove a patch, and a work tree that kept carrying one uf
# no longer ships would be the same invisibility from the other direction.
for entry in "$patch_dir"/[0-9][0-9][0-9][0-9]-*.patch; do
  [ -e "$entry" ] || continue
  git -C "$submodule_path" apply --numstat -- "$repo_root/$entry" | cut -f3
done | sort -u > "$work/claimed"

git -C "$submodule_path" -c core.quotepath=false \
  status --porcelain --untracked-files=all |
  cut -c4- | sort -u > "$work/present"

if [ -n "$(comm -13 "$work/claimed" "$work/present")" ]; then
  printf 'upstream/flow: work tree carries changes no patch claims, rebuilding it\n'
  restore_submodule
fi

if [ "$patch_count" -gt 0 ]; then
  for entry in "$patch_dir"/[0-9][0-9][0-9][0-9]-*.patch; do
    # The day this is all supposed to end, and it has to be said out loud or
    # nobody ever says it: the *pinned commit* already contains the patch.
    # `--cached` asks the index, which is the commit, rather than the work
    # tree, which is the commit plus whatever has been applied to it — so this
    # is true only when upstream took the fix and the submodule was bumped
    # past it. Left alone it would sit in the directory forever, applying to
    # nothing and read by nobody as still needed.
    if git -C "$submodule_path" apply --cached --reverse --check -- "$repo_root/$entry" 2>/dev/null; then
      cat >&2 <<EOF

upstream sync failed: $entry has landed upstream.

  $(grep -m1 '^Issue:' "$entry")
  submodule at $(git -C "$submodule_path" rev-parse --short HEAD)

The pinned commit already contains this patch, so it has nothing left to do.
Delete it, in the same change as the bump that made it redundant:

    git rm $entry

The test that pins the bug it fixed stays — it is what says the fix survived
the bump — and if it is still \`#[ignore]\`d, that goes with the patch.
EOF
      exit 1
    fi

    # Already there, byte for byte. Not touching the file is what keeps cargo
    # from rebuilding the port on every sync.
    if git -C "$submodule_path" apply --reverse --check -- "$repo_root/$entry" 2>/dev/null; then
      continue
    fi

    if git -C "$submodule_path" apply -- "$repo_root/$entry" 2>"$work/why"; then
      printf 'upstream/flow: applied %s\n' "${entry##*/}"
      continue
    fi

    # Loud, and specific about which of the two answers this wants. The work
    # tree goes back to the pinned commit first, so that the sentence below is
    # true and nothing downstream compiles a half-patched type checker.
    restore_submodule
    cat >&2 <<EOF

upstream sync failed: $entry does not apply.

  $(grep -m1 '^Issue:' "$entry")
  submodule at $(git -C "$submodule_path" rev-parse --short HEAD)

git apply said:

$(sed 's/^/  /' "$work/why")

Nothing was skipped and nothing is half-applied — $submodule_path has been put
back to the pinned commit, unpatched, and no build will run until this is
resolved. A patch stops applying for two reasons, and they want opposite
answers:

  * It landed upstream and the submodule was bumped past it. Delete
    $entry, and the test that pins the bug it fixed.
  * The code around it moved. Refresh it against the new commit:

      git -C $submodule_path apply --3way $repo_root/$entry
      # resolve, then
      git -C $submodule_path diff > $entry
      # and put the header back on top

$patch_dir/README.md has the long version.
EOF
    exit 1
  done
fi

if [ "$patch_count" -eq 0 ]; then
  printf 'upstream/flow ready at %s\n' "$(git -C "$submodule_path" rev-parse --short HEAD)"
else
  printf 'upstream/flow ready at %s + %d patch(es) from %s\n' \
    "$(git -C "$submodule_path" rev-parse --short HEAD)" "$patch_count" "$patch_dir"
fi

# The rest of `upstream/` is pinned by commit in `tools/upstream/repos.txt`
# rather than tracked as submodules, for the reason that file gives: nothing in
# the cargo graph depends on them, and this script runs in every CI job.
#
# `--integrations` fetches them. Each is filtered to the subtrees uf actually
# reads — React's compiler crates, Relay's compiler crates, React Native's
# codegen and Libraries — because the three repositories together are about a
# gigabyte and uf reads perhaps eighty megabytes of it.
[ "${1:-}" = "--integrations" ] || exit 0

manifest=tools/upstream/repos.txt

while IFS='|' read -r name url commit subtrees; do
  name=$(echo "$name" | tr -d '[:space:]')
  case "$name" in '' | '#'*) continue ;; esac
  url=$(echo "$url" | tr -d '[:space:]')
  commit=$(echo "$commit" | tr -d '[:space:]')
  subtrees=$(echo "$subtrees" | sed 's/^ *//; s/ *$//')

  dest=upstream/$name
  git_dir=$repo_root/.git/upstream/$name

  if [ ! -e "$dest/.git" ]; then
    rm -rf "$git_dir"
    mkdir -p "$(dirname "$git_dir")"
    git init --quiet --separate-git-dir "$git_dir" "$dest"
    git -C "$dest" remote add origin "$url"
    git -C "$dest" config core.sparseCheckout true
  fi

  if [ "$(git -C "$dest" rev-parse HEAD 2>/dev/null || echo none)" != "$commit" ]; then
    printf 'upstream: fetching %s at %.12s\n' "$name" "$commit"
    git -C "$dest" fetch --quiet --depth 1 --filter=blob:none origin "$commit"
    # shellcheck disable=SC2086 # deliberate word splitting: one argument per subtree
    git -C "$dest" sparse-checkout set $subtrees
    git -C "$dest" checkout --quiet --detach "$commit"
  fi

  for subtree in $subtrees; do
    if [ ! -e "$dest/$subtree" ]; then
      echo "upstream sync failed: $dest/$subtree is missing" >&2
      exit 1
    fi
  done

  printf 'upstream/%s ready at %s\n' "$name" "$(git -C "$dest" rev-parse --short HEAD)"
done < "$manifest"
