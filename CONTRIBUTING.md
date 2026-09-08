# Contributing

## Getting a checkout working

```sh
git clone https://github.com/ubugeeei-prod/uf
cd uf
nix develop
tools/upstream/sync.sh && cargo build --release --bin uf   # the bootstrap
uf run setup
```

After that one bootstrap line, everything in this repository is a `uf` command.
`uf run` on its own lists them.

The bootstrap cannot itself be a `uf` command, and the reason is structural
rather than an oversight: `upstream/flow` is a *path* dependency, so cargo
cannot build `uf` until the submodule is checked out, and `uf run` needs a
built `uf`. One command breaks that circle.

`uf run upstream:sync` fetches one shallow, blobless commit of Meta's official
Flow Rust port and checks out only `rust_port/`, which costs about 40 MB
instead of the full 190 MB repository. `uf_flow` builds against that port,
which is not published to crates.io. The sync is idempotent, so re-run it after
pulling a submodule bump — `uf run setup` does.

## Commit Style

Use focused conventional commits, for example:

```text
feat: add router manifest generation
test: cover large project discovery
docs: define framework defaults
```

## Implementation Policy

Core engines should be implemented in Rust. Config should be expressed through
`uf.config.js`; parser, linter, formatter, build, test, package, and
runtime engines should stay native.

Generated app projects should not require Babel, Jest, Yarn, npm scripts, or
`.flowconfig`. Project tasks belong in `uf.config.js` and run through `uf run`.
Package management belongs to `uf install`/`uf upgrade` and `@uniflowed/pm`;
runtime inference and acquisition belongs to `@uniflowed/rm`.

Hot paths should prefer `CompactString`, `SmallVec`, arenas, PHF tables, and
borrowed data over `String`, `Vec`, standard hash maps, and cloning. The
repository keeps a Vize-style `clippy.toml` policy so these can be tightened as
crate APIs stabilize.

Fuzzing, formal verification, benchmarks, scripts, and Nix support live under
`tools/` so the root stays focused on the Rust workspace and project metadata.
Use `nix develop ./tools/nix` for the pinned development environment.

## Reviews On Formatter Fixtures

`crates/uf_fmt/tests/fixtures` holds input to a printer, not programs. Nothing
declared in a fixture is meant to be used, half the constructs are deliberately
awkward, and the `.expected.js` beside each one is Prettier's output for it
byte for byte.

GitHub's code-quality review reads them as code anyway, and reports every
binding in them as an unused variable. A pull request that adds a fixture
arrives with a dozen of those, and because `main` requires conversations to be
resolved, they block the merge.

They are false positives and they are resolved as such — with a reply saying
so, not silently. Do not "fix" one by using a variable in a fixture: the
fixture is what Prettier was run on, and changing it changes what the
expectation means.

The exclusion belongs in the analysis rather than in the fixtures, and it is
not configurable from this repository. See ubugeeei-prod/uf#157.

## Verification

One command, and it is the one CI runs:

```sh
uf run ci
```

This repository is a uf project, so its pipeline uses the toolchain it ships
rather than a second description of the same checks. A check that is in CI and
not in `uf run ci` is a check a contributor cannot run before pushing, which is
the thing that arrangement exists to prevent.

The individual steps have names too, for when one of them is what you are
working on:

```sh
uf run rust:fmt:check   # cargo fmt --all -- --check
uf run rust:clippy      # cargo clippy --workspace --all-targets -- -D warnings
uf run rust:test        # cargo test --workspace
uf run rust:bench       # cargo bench --workspace --no-run
uf run fmt:check        # uf's own formatter, over this repository's Flow
uf run test:lib         # uf test#library, the @uniflowed/* suite
uf run docs:build       # uf build#docs
uf run docs:dev         # uf dev#docs, to look at the site while editing it
uf run docs:links       # every link in what the build wrote resolves
uf run security:scan    # the threat model, the build's output, and `uf new`
```

`docs:links` reads `docs/dist/docs`, so `docs:build` has to have run — or run
`uf run docs:verify`, which is the two of them and the security scan over one
build, and is what the `Docs build` job runs. It touches no network:
`uf run docs:links:external` is the pass that does, it is not in `ci`, and a
schedule runs it, because a check that fails when somebody else's host is slow
is a check people learn to re-run until it goes green.

`rust-toolchain.toml` pins `nightly-2026-08-01`, and every command above uses
it. The pin is a requirement, not a preference: `uf` parses and type-checks Flow
with Meta's official Rust port, 23 of whose crates declare
`#![feature(box_patterns)]` — a feature the compiler removed around the
2026-09-01 nightly. That date is the newest nightly that still accepts it.

Run `uf run upstream:sync` before any cargo command; the port lives in the
`upstream/flow` submodule and nothing builds without it.

The `Upstream Flow` CI job builds the parser alone on the floating `nightly`
channel. It is an early warning that the port still compiles there, so the pin
can be advanced deliberately:

```sh
RUSTUP_TOOLCHAIN=nightly cargo test -p uf_flow
```

When GitHub Actions is configured, use Actions as the final merge gate.

### Adding a job to the pipeline

`ci.yml` ends with a job named `CI` that runs whatever happened above it
(`if: always()`) and is red unless every job it names came back `success`.
Every other job in that file declares `needs: toolchain`, because `Toolchain`
builds the `uf` binary the rest of the pipeline runs its checks through — and a
job whose dependency fails is not run, it reports `skipped`, and GitHub counts
a skipped required check as satisfied. Without that gate a `Toolchain` that
does not compile turns the entire required set green and an armed auto-merge
fires, which is what left `main` not compiling on 2026-09-06. See
ubugeeei-prod/uf#367.

So a job added to `ci.yml` belongs in that gate's `needs:`. A job that must
*not* block a merge — the way `Upstream Flow` must not, because it builds on
the moving nightly on purpose to say early when the pin has to advance — says
so in a comment instead:

```yaml
  # ci-gate excludes <job> — <why it must not block a merge>
```

`uf run ci:gate` reads the workflow and fails on a job that is in neither, and
on a required context that could report `skipped` for any other reason, so this
is not something a review has to remember to catch.

### The merge queue

`main` requires branches to be up to date and the pipeline takes about eleven
minutes. Those two multiply: every merge makes every other open pull request
`BEHIND`, so pull requests land one at a time, each paying a full pipeline for
a base change that did not touch it. Ten green pull requests is roughly two
hours of CI to land work that was already green, and dropping the requirement
is not the answer — it is what stopped `main` from being broken by two pull
requests that were each green apart.

A merge queue is the mechanism for that shape. It batches pull requests, builds
the *result* of merging them together once, and merges the batch if that build
is green. Ten pull requests cost one pipeline instead of ten, and the guarantee
is stronger than being up to date: up to date proves a branch against the base
it was rebased onto, never against the other branch merging beside it.

So `ci.yml` and `security.yml` run on `merge_group` as well as
`pull_request` — those are the two files that report a context branch
protection requires, and a required check that does not run on the queue's
speculative merge commit is not a check that fails, it is one the batch sits
behind until it times out. `uf run ci:gate` holds every such workflow to that
trigger, so adding a required context from a new file cannot quietly leave the
queue waiting.

Nothing here changes what a contributor runs: `uf run ci` describes the same
checks, and `gh pr merge --squash --auto` still arms a merge.

**When the queue kicks a pull request out.** It does that when the batch built
red and yours is the change that broke it — usually against another pull
request in the same batch, not against `main`, which is the case being up to
date could never have caught. GitHub comments on the pull request and removes
it from the queue; the branch is untouched. Read the failed `merge_group` run
to see what the combination broke, push the fix, and re-arm the merge. Nothing
has to be reset by hand, and a pull request that was kicked out for somebody
else's failure can simply be queued again.
