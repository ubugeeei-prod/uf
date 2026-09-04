# Architecture red lines

uf is an all-in-one toolchain, which is the same shape as `create-react-app`.
CRA was not a bad tool — it was the right design for 2016 that could not keep
up — and the way it failed is a specification for how uf could fail. This
document is the list of things uf will not do, and an honest account of where
it does not yet comply.

The one-line version of CRA's lesson:

> Abstraction without composability eventually becomes a bottleneck.

`react-scripts` succeeded completely at hiding configuration and failed at
letting anyone peel that abstraction back a layer at a time. Between "zero
config" and "eject" there was nothing — a cliff, and an irreversible one. uf is
in a more dangerous position than CRA was, because CRA integrated a build
setup and uf integrates the language toolchain, the build, the dev server, the
test runner, the formatter and the linter.

## The principle

**uf owns orchestration, not implementation.**

uf decides what runs, when, and how the pieces connect. It does not own the
bundler, the dev server, the formatter, the linter or the runtime. The user's
experience is all-in-one; the implementation is federated.

> Unified interface, federated implementation.

Those are different things, and CRA's mistake was assuming they had to match.

This is the target, and uf does not meet it today: `uf_fmt` and `uf_test` are
uf's own implementations. That is deliberate — a Flow-aware formatter and a
Flow-aware test runner did not exist, and "orchestrate the provider that does
not exist" is not a plan — but it is an exception rather than a revision of the
principle, and the audit below treats it as one. An exception stays an
exception by staying replaceable.

## The red lines

1. **Never fork Vite or Rolldown behaviour unless it is unavoidable.**
2. **Never mirror the complete configuration schema of an upstream tool.**
   Every option uf re-declares is an option that needs a uf release before
   anyone can use it.
3. **Every built-in provider must be replaceable.** A default is a
   convenience, not an architecture.
4. **No `eject` command.** The path from convention to raw provider access is
   a continuum, not a cliff with a one-way door at the end of it.
5. **No single package controls ecosystem dependency versions.** CRA's fatal
   structure was `ecosystem change → react-scripts → user`: one chokepoint,
   serialising every upgrade in the ecosystem through one release.
6. **Core must not depend on a specific JavaScript runtime.**
7. **All orchestration must be inspectable.** An integrated tool that cannot
   say what it is doing is a black box, and a black box is where the CRA
   problems became unfixable rather than merely annoying.
8. **Provider-specific functionality must remain accessible.** If Vite can do
   it, a uf project can do it, without waiting for uf.
9. **Defaults are conveniences, never architectural requirements.**
10. **uf's core owns the graph, not the tools.**

## The continuum that replaces `eject`

```text
convention  →  configuration  →  provider replacement  →  raw provider API
```

Each step is a smaller decision than the one before it, and none of them is a
door that locks behind you. A project that needs one Vite plugin adds one Vite
plugin; it does not inherit a build system.

## Three runtimes, not one

"Runtime" is three separate questions, and collapsing them is how a toolchain
ends up unable to target anything new:

| | |
| --- | --- |
| **Orchestration host** | where uf itself runs — a native binary |
| **Plugin runtime** | where JavaScript plugins run — Node.js, Bun, Deno |
| **Target runtime** | where the output runs — a browser, a worker, a server |

All three can differ in one build: a native binary orchestrating Vite on
Node.js to produce a bundle for Cloudflare Workers is an ordinary case, not an
exotic one.

## Where uf does not yet comply

Writing the list is worth nothing without saying where we stand against it.

**Red line 2 is half closed.** The `vite` key in `uf.config.js` is merged over
what uf generates, so a Vite option uf has never heard of is now reachable —
which is the part of the `react-scripts` failure that actually stranded people.
What remains is the other half: `uf_config`'s `dev` and `build` still re-declare
Vite's options one at a time — `host`, `port`, `strictPort`, `allowedHosts`,
`fs.allow`, `fs.deny`, `outDir`, `sourcemap` — and `viteConfig()` in
`@uniflowed/vite` maps them across by hand. Every one of those is a second name
for a setting that already has one, and a second name is a thing to keep in
sync.

**Red line 3 is aspirational.** `LintEngine`, `FlowFormatParser`,
`TaskRunnerEngine` and `PackageManagerResolver` are enumerations with exactly
one variant each. The shape of replaceability exists; nothing is actually
replaceable. Until a second provider exists for at least one of them, this
line is a statement of intent.

**Red line 5 is at risk.** `tools/release/bump-version.sh` sets every version
in the repository to one value, and every `@uniflowed/*` package pins its
siblings exactly. That is right for a pre-release where the packages are one
thing, and it is the beginning of lockstep. Adapters need to be able to move
independently before 1.0.

**Red line 7 is met.** `uf inspect --json` prints the resolved configuration
and the route table, and `uf explain <command>` names the provider for every
stage of `dev`, `build`, `test`, `fmt`, `lint` and `check` — which binary runs
it, and what it does. It stays met by covering every command a person runs, so
a stage that cannot be explained is a stage that should not exist.

**Red line 10 is the one to watch.** `uf_fmt` and `uf_test` are uf's own
implementations rather than orchestrated providers. That is a deliberate
choice — a Flow-aware formatter and a Flow-aware test runner did not exist —
but each is a place where uf owns a tool rather than the graph, and each has
to stay replaceable or it becomes the thing this document is about.

## How each open line gets closed

Naming a violation is the easy half. This is what closing each one looks like,
and what would prove it closed — because "we should fix that" is how a red line
becomes a permanent footnote.

**Line 2 — stop re-declaring Vite's schema.** Keep only the settings uf
genuinely owns: the ones with cross-tool meaning, and the ones uf *enforces*
rather than forwards. `dev.allowedHosts` is the clearest keeper — uf refuses to
bind a routable address without it, so it is a uf rule that happens to look like
a Vite option. `build.budgets` is uf's own and has no Vite equivalent. Every
remaining key whose entire effect is to be copied into Vite's config should go,
and the `vite` passthrough is where it goes.

*Closed when:* a test walks the config schema and fails on any key whose only
consumer is `viteConfig()`.

**Line 3 — make one provider actually replaceable.** `LintEngine`,
`FlowFormatParser`, `TaskRunnerEngine` and `PackageManagerResolver` are each an
enumeration with one variant, which is the shape of replaceability with none of
the substance. The nearest real second variant is already half-specified: the
formatter is supposed to route `.json`, `.jsonc`, `.css` and `.ts` to Biome, and
today the config key exists and the routing does not.

*Closed when:* at least one of those enumerations has a second variant that a
project can select, `uf explain fmt` names whichever was selected, and the
non-default one is exercised by a test.

**Line 5 — let the adapters move apart.** `tools/release/bump-version.sh` sets
every version in the repository to one value and every `@uniflowed/*` package
pins its siblings exactly. That is honest for a pre-release where the packages
are one thing shipped in pieces, and it is exactly the structure red line 5
exists to forbid, so it has to end before the structure calcifies.

*Closed when:* a sibling dependency is a range rather than an exact pin, and the
release script can bump one package without bumping the rest.

**Line 10 — keep the two owned tools replaceable.** `uf_fmt` and `uf_test` are
uf's own implementations, and that will not change: a Flow-aware formatter and a
Flow-aware test runner did not exist. What has to be true is that they are
reached the same way every other provider is, so a project that wants a
different one is configuring uf rather than fighting it.

*Closed when:* `uf fmt` and `uf test` resolve their implementation through the
same seam as the bundler and the dev server, and `uf explain` reports it.

## What is not on the table

Some things are worth naming as permanently out of scope, because each is a
plausible-sounding step toward the thing this document is about.

- **An `eject` command**, in any spelling. Not `uf eject`, not
  `uf config --write`, not a "print the effective Vite config so you can copy
  it" flag that becomes one in practice. The continuum above is the whole
  answer, and every step on it is reversible.
- **A uf-shaped name for a thing Flow already names.** `@noflow` is how a file
  says it is plain JavaScript; a `check.exclude` list would have been a second
  answer to a question with one, in a place a reader is not looking.
- **Vendoring a provider to make it fit.** If uf needs behaviour Vite does not
  have, the fix is upstream or a plugin, never a fork — red line 1, and the
  reason it is first.

## The failure this does not prevent

CRA's third failure was not architectural. It solved the build and stopped:
routing, data fetching and code splitting turned out to be one problem rather
than three, CRA could not integrate them, and every production application built
its own framework on top of the tool that existed to remove that work.

No red line prevents that one. It is prevented by uf being a framework as well
as a toolchain — the router, the RSC graph, server actions and the data layer —
and by those being answers uf actually ships rather than integration points it
documents. The audit above is about not becoming a bottleneck. This is about
being worth using in the first place, and the roadmap is the only defence
against it.

## Why this is written down

Every one of these is easy to cross for a good local reason, and each crossing
is invisible until the toolchain is the bottleneck. CRA's maintainers did not
decide to become a chokepoint; they made a series of individually reasonable
decisions with no line to notice they had crossed.
