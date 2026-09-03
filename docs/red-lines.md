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

```
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

**Red line 2 is violated today.** `uf_config`'s `dev` and `build` re-declare
Vite's options one at a time — `host`, `port`, `strictPort`, `allowedHosts`,
`fs.allow`, `fs.deny`, `outDir`, `sourcemap` — and `viteConfig()` in
`@uniflowed/vite` maps them across by hand. A Vite option uf has not heard of
is unreachable, which is precisely the `react-scripts` failure. Fixing this
means uf keeping only the settings it genuinely owns (the ones with
cross-tool meaning, or that uf *enforces*, such as the `allowedHosts` gate on
binding a routable address) and passing everything else through natively.

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

**Red line 7 is partly met.** `uf inspect --json` prints the resolved
configuration and the route table. There is no `uf explain <command>` yet:
nothing says which provider will run each stage, at which version, and which
files the configuration came from.

**Red line 10 is the one to watch.** `uf_fmt` and `uf_test` are uf's own
implementations rather than orchestrated providers. That is a deliberate
choice — a Flow-aware formatter and a Flow-aware test runner did not exist —
but each is a place where uf owns a tool rather than the graph, and each has
to stay replaceable or it becomes the thing this document is about.

## Why this is written down

Every one of these is easy to cross for a good local reason, and each crossing
is invisible until the toolchain is the bottleneck. CRA's maintainers did not
decide to become a chokepoint; they made a series of individually reasonable
decisions with no line to notice they had crossed.
