# Patches to `upstream/flow`

Fixes to Meta's Flow Rust port that uf needs and `facebook/flow` has not taken
yet. `tools/upstream/sync.sh` applies every `*.patch` in this directory to the
`upstream/flow` submodule after checking it out, so **a checkout is the pinned
commit plus these patches** — on a laptop, in every CI job, and after
`uf run setup`.

They are a stopgap with an expiry date, not a fork. Each one is a diff small
enough to read in a review, against a named issue, and the first thing to try
for any of them is still to get it upstream.

[ubugeeei-prod/uf#707](https://github.com/ubugeeei-prod/uf/issues/707) is why
this directory exists and what it is designed against.

## The rules the sync enforces

- **Everything here is applied.** The directory is the list; there is no
  second file to forget a patch in. A `*.patch` that does not apply stops the
  sync, which stops the build, which stops every job. A patch quietly dropped
  from a type checker is invisible — the compiler is happy, the tests are
  green, the answers are wrong — so it is made impossible rather than
  unlikely.
- **Names are `NNNN-what-it-does.patch`**, four digits. That is the order they
  apply in, and it is the same order in every locale. Anything else in this
  directory except this README is refused rather than ignored.
- **Each one carries `Issue:`** in a header above the diff, or the sync
  refuses it. A patch to somebody else's type checker has to say whose bug it
  is, or nobody can tell a fix that has landed upstream from one that has not.
- **Re-running the sync is free.** A patch already in the work tree is left
  alone — not rewritten — so cargo does not rebuild the port every time
  someone runs `tools/upstream/sync.sh`.

## Adding one

```sh
# 1. Make the change in the submodule.
$EDITOR upstream/flow/rust_port/crates/...

# 2. Write it out. `git -C` so the paths are relative to the submodule root.
git -C upstream/flow diff > tools/upstream/patches/flow/0002-what-it-does.patch

# 3. Put a header on top of the diff — everything above the first `diff --git`
#    line is ignored by `git apply` and is where the explanation goes.
```

0001 is the worked example. `Issue:` is the only line the sync insists on; the
rest are there because the person who inherits this will not have the issue
open:

```
Subject: one line, what the patch does

Issue: ubugeeei-prod/uf#NNN
Upstream: <link to the facebook/flow PR, or "not filed yet">
Submodule: <the full commit this was made against>

Why it exists, what it changes, and what would have to be true for it to be
deleted.
```

Then `tools/upstream/sync.sh`, and the test that fails without the patch —
`crates/uf_check/tests/upstream_patches.rs`.

## Removing one

**Deleting the file is the whole procedure.** The next sync sees the work tree
carrying a change no patch claims, restores the submodule to the pinned commit
and re-applies what is left.

Delete it when the fix lands upstream and the submodule is bumped past it, and
the sync will tell you when that day arrives rather than leaving the patch to
sit here applying to nothing:

```
upstream sync failed: tools/upstream/patches/flow/0001-....patch has landed upstream.
The pinned commit already contains this patch, so it has nothing left to do.
Delete it, in the same change as the bump that made it redundant:

    git rm tools/upstream/patches/flow/0001-....patch
```

A bump that takes the fix in a different shape gets the other failure — the
patch no longer applies — and that message names both possibilities, because
from the outside they look the same:

```
upstream sync failed: tools/upstream/patches/flow/0001-....patch does not apply.
...
  * It landed upstream and the submodule was bumped past it. Delete
    tools/upstream/patches/flow/0001-....patch, and the test that pins the bug it fixed.
  * The code around it moved. Refresh it against the new commit: ...
```

A patch's other half is a test in this repository that fails without it —
`crates/uf_check/tests/upstream_patches.rs` for the ones here. When the patch
goes, **that test stays**: it is what says the fix survived the bump. Only its
`#[ignore]`, if it has one, goes with the patch.

`tools/upstream/test-patches.sh` runs the mechanism against every way a patch
can go missing, on a scratch submodule. `uf run upstream:patches:test`.

## Refreshing one after a submodule bump

```sh
git -C upstream/flow apply --3way tools/upstream/patches/flow/0001-....patch
# resolve the conflicts in upstream/flow, then
git -C upstream/flow diff > tools/upstream/patches/flow/0001-....patch
# and put the header back on top
```

## Two things to know

`upstream/flow` shows as modified in `git status` whenever a patch is applied.
That is the mechanism working, not a checkout to clean up. Never commit the
submodule pointer to a patched state — the pin is a `facebook/flow` commit, and
the patches are here.

The sync owns that work tree. It restores it whenever it finds a change no
patch accounts for, so an edit made directly in `upstream/flow` lives until the
next sync and no longer. Snapshot it into a patch file here before running
anything that syncs, `uf run setup` included.
