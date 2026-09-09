<div align="center">

<img width="200" src="brand/uniflowed-mark.svg" alt="">

# uf

**Build the strongest React development experience with Modern Flow.**

[Documentation](https://docs.uniflowed.dev) ·
[Install](https://docs.uniflowed.dev/guide/install) ·
[Your first project](https://docs.uniflowed.dev/guide/project) ·
[What uf does not do](https://docs.uniflowed.dev/guide/scope) ·
[Roadmap](docs/roadmap.md)

</div>

`uf` is a single native binary that runs, builds, tests, formats, lints and
type-checks a React application written in Flow. It is not a wrapper around
tools you assemble: the dev server, the production build, the test runner, the
formatter and the linter all read the same syntax tree, produced once by Meta's
own Flow parser, which is compiled into the binary.

The name is short for *unified flow*. The command is `uf`; the package scope on
npm is `@uniflowed`.

## Why it exists

React is written in Flow, and Flow has spent years growing syntax that exists to
describe React — `component` and `hook` declarations, `renders` types, `match`,
enums. Nothing else has any of it.

Using that syntax today is an assembly job. Babel to strip the types, a preset
that understands `component`, a plugin for the React Compiler, a bundler, a
second config to tell the bundler about the first, a test runner with its own
transform pipeline that has to be configured the same way, a linter with a
parser plugin so it can read the syntax, a formatter, and a `tsconfig.json` you
do not use but something insists on. Every one of those is a place where two
tools can disagree about what your code means, and when they disagree the error
surfaces somewhere unrelated: a test that passes and a build that fails.

uf's position is that there should be exactly one answer to *what does this file
mean*, that it should be given once, in Rust, and that everything else should
use it. That is the whole idea.
[Why Flow](https://docs.uniflowed.dev/guide/why) argues the language;
[What uf is](https://docs.uniflowed.dev/guide) argues the toolchain.

Two things uf deliberately does not do. It does not replace Vite and does not
want to — the dev server and the production build **are** Vite, driven over a
JSON protocol, so Vite's plugins keep working and the `vite` key in
`uf.config.js` is merged over whatever uf generated. And it does not type-check
your code: Flow does that, and `uf check` runs it. The rules uf holds itself to,
and the audit of where it does not yet meet them, are in
[docs/red-lines.md](docs/red-lines.md).

## Where it stands

**uf is `0.0.0-alpha`.** Eight prereleases so far, `uf@0.0.0-alpha.0` through
`uf@0.0.0-alpha.7`. It installs in seconds and scaffolds a project that builds —
and it changes under you: commands, config keys and package exports move without
a deprecation cycle, and no version here promises compatibility with the one
before it.

The specific shape of "alpha", so you can decide before you spend an afternoon:

- **Runtimes.** Node.js and Bun load Flow, and a test starts each of them.
  Deno runs a uf project too, by a different road — it has no module hook, so
  uf compiles ahead of time and hands it an import map — which is why it is
  graded *experimental*: a module uf could not enumerate is still Flow to Deno.
  The edge runtimes have no host at all
  ([#246](https://github.com/ubugeeei-prod/uf/issues/246)). The per-host
  matrix — what works, what does not, and what each gap is waiting for — is
  [`docs/hosts.md`](./docs/hosts.md).
- **Deployment.** `uf build --adapter` writes for `node`, `container`, `edge`,
  `serverless` and `static`, all five against one `@uniflowed/server/fetch`
  handler, and `--compile` writes a single executable file — with Bun or with
  Node's single-executable applications, whichever the project's Capability JS
  Host is, and `--target` builds it for another platform. No server output has
  ever been deployed to a real platform, and no cross-built binary has ever
  been run. `bun` and `deno` are names in an enum, waiting on a benchmark
  ([#391](https://github.com/ubugeeei-prod/uf/issues/391)).
- **Server Components.** `"use client"` and `"use server"` are scanned, graphed
  and manifested, and the build reads that: a route no client boundary reaches
  keeps its page out of the browser bundle entirely. Nothing smaller than a
  route is split, so a Server Component *above* a boundary still ships whole,
  and server actions are analysed, keyed, typed and not yet callable
  ([#252](https://github.com/ubugeeei-prod/uf/issues/252)).
- **Tests.** About nine times faster than Vitest on a 1,000-test suite, and about
  three times slower than Bun's runner, which
  [Testing](https://docs.uniflowed.dev/guide/testing) explains rather than hides.
- **Platforms.** macOS and Linux, on x86-64 and ARM64. There is no Windows build;
  `install.ps1` resolves and says so rather than failing obscurely, and WSL2
  works.
- **Packages.** Seventeen `@uniflowed/*` packages are on npm — the closure a new
  project needs, plus the test runner. Eleven more are implemented and still
  workspace-only ([#210](https://github.com/ubugeeei-prod/uf/issues/210)); they
  wait in `tools/release/pending-packages.txt`. Every release so far is a
  prerelease and `latest` still points where the first publish left it, so name
  the tag if you install one by hand: `npm install @uniflowed/ui@alpha`.

### Rough edges in `0.0.0-alpha.7`

Three of these stand between the next section and a reader who follows it, so
they are named here rather than found there. All three are scoped to a version
on purpose: this block goes when the release that closes them does.

- **A new project installs `0.0.0-alpha.1` packages.** `uf new` writes
  `"latest"` for each `@uniflowed/*` dependency, and `latest` on npm still
  points where the first publish left it, because a prerelease must not displace
  a stable release and there has never been one. `uf test` in such a project
  stops with `@uniflowed/host` is not installed. The scaffold now pins the exact
  version of the uf that wrote it and `latest` has a script to move it
  ([#408](https://github.com/ubugeeei-prod/uf/issues/408),
  [#416](https://github.com/ubugeeei-prod/uf/pull/416)); neither has shipped.
- **Pinning to `alpha` instead trades that for a different failure.**
  `@uniflowed/host` names `./write-atomically` in its `exports` map and leaves
  the file out of its `files` array, so the tarball does not contain it — in
  `alpha.5`, `.6` and `.7` alike — and `uf build` and `uf test` stop at
  `ERR_MODULE_NOT_FOUND`. Fixed on `main`
  ([#410](https://github.com/ubugeeei-prod/uf/pull/410)).
- **`uf fmt` exits non-zero in a scaffolded project.** The default non-Flow
  formatter is Biome, `package.json` is a non-Flow file, and the scaffold does
  not install Biome. Install it, or set `fmt.nonFlow.formatter: "none"` in
  `uf.config.js`.

What does work on `0.0.0-alpha.7` as published, from a default
`uf init` followed by `uf install`: `uf build` writes `dist/`, and
`uf lint` and `uf check` report on the project. For the rest, a checkout is the
way in until the next release — see
[Building from a checkout](#building-from-a-checkout).

[What uf does not do](https://docs.uniflowed.dev/guide/scope) is the complete
list, sorted into refusals — decisions that will not change, each with its reason
— and gaps, each with an issue number.

## Install

```sh
curl -fsSL https://setup.uniflowed.dev | sh
```

```
  Unified Toolchain for Flow

  target   aarch64-apple-darwin
  version  0.0.0-alpha.7  (prerelease — no stable release yet)

  ✓ downloaded uf-aarch64-apple-darwin.tar.gz
  ✓ verified   sha256 ab8c17c6adc1
  ✓ unpacked   …/uf/runtimes/uf@0.0.0-alpha.7
  ✓ linked     uf, ufr, ufx into …/bin

  uf 0.0.0-alpha.7 is ready.
```

The script picks the archive for your platform, checks it against the sha256 in
the release manifest before unpacking anything, and links `uf`, `ufr` and `ufx`
into `~/.local/bin`, with the runtime itself under `$XDG_DATA_HOME/uf`
(`UF_INSTALL_ROOT` and `UF_BIN_DIR` override both). It is short enough to read
first, and it is served from `https://setup.uniflowed.dev/install.sh` if you
would rather fetch it before running it.

Nix, and building from a checkout, are on
[the install page](https://docs.uniflowed.dev/guide/install).

uf still needs a JavaScript host — Node.js or Bun — because Vite and your test
bodies run there. `uf info` prints the one it found.

## Five minutes

```sh
uf new hello
cd hello
uf install
uf dev          # a dev server on :5173, with Fast Refresh through `component`
uf build        # dist/, prerendered, with real gzip and brotli sizes
uf test
```

`uf new` writes the project and nothing else — no `node_modules`, no lockfile,
no git history:

```
uf new · hello
──────────────

  hello
  ├─ app
  │  ├─ Counter.js
  │  ├─ _uf.layout.js
  │  ├─ _uf.page.js
  │  ├─ _uf.page.test.js
  │  └─ useCounter.js
  ├─ .gitignore
  ├─ app.js
  ├─ package.json
  └─ uf.config.js

✓ created 9 files in …/hello
```

Three of those nine are a demonstration you read once and delete. There is no
`vite.config.ts`, no `babel.config.js`, no `.eslintrc`, no `.flowconfig` and no
`vitest.config.ts`, and there is not going to be one.

`uf install` hands the work to the package manager the project already uses, with
lifecycle scripts refused — `--ignore-scripts` to the manager, and a manifest
that declares `scripts` of its own is rejected before anything is fetched. The
`chosen by` line is uf saying that its own resolver, `@uniflowed/pm`, cannot
fetch yet:

```
  manager    npm
  chosen by  uf.lock names uf, whose resolver cannot fetch yet
  command    npm install --ignore-scripts --loglevel=http
  runtime    node · /opt/homebrew/bin/node
  lockfile   package-lock.json · 240 packages · 136.26 kB

  config    ························  88.2µs
  workspace ························ 683.2µs
  resolve   ························    7.3s
  fetch     ························   1.31s
  lockfile  ························   734µs
  total     ························   8.62s

✓ dependencies installed in 8.62s
```

Then `uf build` — Vite, driven by uf, with every module transformed in-process
and every prerenderable route written as HTML:

```
uf build · hello
────────────────

  config       ························  73.9µs
  routes       ························  53.1µs
  router types ························ 130.4µs
  rsc analysis ························ 354.5µs
  vite         ························   1.13s
  manifest     ························ 145.3µs
  rsc manifest ························ 108.9µs
  bundle size  ························ 912.9ms
  total        ························   2.04s

  engine             vite
  host               node
  prerendered pages  1
  modules            7
  client components  1

  shipped
    assets  10
    raw     209.41 kB
    gzip    66.10 kB
    brotli  57.04 kB

✓ build succeeded in 2.04s
```

and `uf test`, whose discovery, ordering, worker pool and report are Rust and
whose test bodies run on the host:

```
uf test · hello
───────────────

  ✓ app/_uf.page.test.js  useCounter > is a hook, so it is only callable from a component or another hook

✓ 1 passed, 0 failed in 297.6ms
```

Those four blocks are one real run of `uf new`, `uf install`, `uf build` and
`uf test`, at version `0.0.0-alpha.7` on Node v25.8.1, trimmed — the banner,
npm's own output, the per-asset size table and absolute paths — and not
otherwise edited. `uf dev` has no transcript here because it does not exit.

What it ran against is worth stating exactly, because it is not yet what
`curl … | sh` gives you: a `uf` built from `main`, whose scaffold pins the
packages to its own version, and the published `@uniflowed/host` for that
version with the files `main` ships and `0.0.0-alpha.7` left out of the tarball
put back into it. That pair is the next release;
[Rough edges in `0.0.0-alpha.7`](#rough-edges-in-000-alpha7) is the distance
between it and today.

The compressed figures are measured by really compressing the bytes at fixed
settings, never estimated, and `build.budgets` in `uf.config.js` turns them into
a check.

## What the binary does

Every command is in the release binary, and none of them needs a line in
`package.json`. [Commands](https://docs.uniflowed.dev/reference/cli) is the full
reference, with every flag and exit code.

| | |
| --- | --- |
| `uf new`, `uf init` | Scaffold an application into a new directory, or into this one; `--lib` scaffolds a library |
| `uf clean` | Removes what a rebuild writes again — never a lockfile, and `node_modules` only when asked |
| `uf dev` | Vite's dev server, with React Fast Refresh through `component` declarations |
| `uf build` | Client and server bundles, prerendered routes, a size report and the build manifest |
| `uf build`, for a library | `uf new --lib` turns the router off, and that makes the build compile your entries to `dist/` instead: every declared dependency left as an import, and a package that ships the Flow source and the JavaScript beside it |
| `uf preview`, `uf start` | Serve that build — through Vite, or through uf's own server with no bundler in the process |
| `uf test` | Scheduling and reporting in Rust, one file per worker, bodies on the host |
| `uf fmt` | Flow printed from Meta's parser to match Prettier; JSON, CSS and TypeScript handed to Biome |
| `uf lint` | uf's rules and Flow's built-in lints, one pass, one report; `--fix` writes the fixes it can make |
| `uf check` | Flow's own inference. uf has no second opinion about your types |
| `uf install` | The project's package manager, with lifecycle scripts refused |
| `uf add`, `uf remove`, `uf update`, `uf why` | The same manager, one dependency at a time, rewriting the lockfile and the store |
| `uf ls`, `uf audit`, `uf search` | The same manager, reading — and, where it has no such command, saying so rather than reaching for another one |
| `uf run`, `ufx` | A task from `uf.config.js`; a package's binary |
| `uf info`, `uf inspect`, `uf explain` | What uf found, what your config resolved to, and which provider does each stage of a command |
| `uf prepare` | The code generation and checks a commit should not go without; `--fix` makes the checks write |
| `uf lsp` | The language server, over stdio |
| `uf mcp` | The same commands as MCP tools, over stdio, for an agent |
| `uf env`, `uf use`, `uf self-update` | The JavaScript hosts a project pins, the shared store they live in, and the uf that runs it |

## One config file

This is the whole of a scaffolded project's `uf.config.js`. Nothing in it
describes the application, because `app/` is the route root and `app.js` the
entry by default; the `tasks` block is there so `uf run build` and your pipeline
say the same thing:

```js
// @flow
import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  tasks: {
    dev: { command: "uf dev" },
    build: { command: "uf build" },
    check: { command: "uf check" },
    lint: { command: "uf lint" },
    fmt: { command: "uf fmt" },
    test: { command: "uf test" },
  },
});
```

That one file is also where the runtime, the router, the build, the test runner,
the formatter and the linter are configured when you need to configure them. It
is Flow, so `defineConfig` type-checks the object where you write it rather than
where uf reads it. Every option and its default is in
[the config reference](https://docs.uniflowed.dev/reference/config).

## Implemented, experimental, planned

uf is aiming at a wide surface from an early position, so the difference between
those three decides whether a feature is something you can use this afternoon.
This README keeps them apart and does not restate the lists, because a fourth
copy of a list is a fourth thing to keep true:

| | What it means | Where it is kept |
| --- | --- | --- |
| **Implemented** | Reachable from the binary, covered by a test, documented. The command and config references describe only what exists | [Commands](https://docs.uniflowed.dev/reference/cli), [`uf.config.js`](https://docs.uniflowed.dev/reference/config) |
| **Experimental** | Implemented, reachable, and expected to change — said on the page that documents it. `@uniflowed/effect`'s requirement subtraction, `@uniflowed/tui` without mouse or selection, `uf build --compile`, whose cross-built binaries are checked by their file format because nothing here can run one | [Packages](https://docs.uniflowed.dev/reference/packages) |
| **Planned** | Not written. Every gap carries an issue number, and the roadmap sorts them P0–P3 | [What uf does not do](https://docs.uniflowed.dev/guide/scope), [docs/roadmap.md](docs/roadmap.md) |

A declared API that throws is not a feature, and this repository has a check that
says so: `tools/ci/publishable.sh` refuses an implemented package that is on its
way to nobody. The three ways that standard fails, and the table of where uf's
own north star is not true yet, are at the top of
[docs/roadmap.md](docs/roadmap.md).

[uf compared](https://docs.uniflowed.dev/guide/compare) puts uf beside Next.js,
Vite, Bun and `create-react-app`, with the rows uf loses in the same tables as
the rows it wins. If you write TypeScript, read that one first: the syntax
argument is worth nothing to you and most of the other rows are a downgrade.

## The library surface

The Flow packages ship as `@uniflowed/*` modules — the router, the server, typed
HTTP and query caching, effects, state, validation, forms, headless UI
primitives, StyleX styling, the test runner and the rest. "The server" there
means a **BFF** — route handlers, server actions and rendering, the backend
*for this frontend*. It is deliberately not an application backend, and
[architecture red lines](docs/red-lines.md) says why that is a boundary rather
than a gap. They are plain Flow
with no native bindings, so what runs in the browser is what you can read, and
`uf_lib` in this repository is the registry they are all declared in.

[Packages](https://docs.uniflowed.dev/reference/packages) is the annotated list,
including which are on npm and which resolve only through this repository's
workspace.

## Documentation

[docs.uniflowed.dev](https://docs.uniflowed.dev) is the manual. It is built by
`uf build` from `docs/` in this repository — a real uf project, which is the
point of it.

In the repository:

| | |
| --- | --- |
| [docs/architecture.md](docs/architecture.md) | The crates, the Flow parser boundary, and why the toolchain is pinned to a nightly |
| [docs/red-lines.md](docs/red-lines.md) | What uf will never do, and where it does not yet comply |
| [docs/roadmap.md](docs/roadmap.md) | The north star, the three ways it fails, and P0–P3. [Issue #1](https://github.com/ubugeeei-prod/uf/issues/1) tracks it |
| [docs/security.md](docs/security.md) | The threat model: a published CVE per row, and the decision that makes the same bug impossible here |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Getting a checkout working, and the one command CI runs |

## Building from a checkout

This is the contributor's path. You do not need it to use uf.

```sh
git clone https://github.com/ubugeeei-prod/uf
cd uf
nix develop
tools/upstream/sync.sh && cargo build --release --bin uf   # the bootstrap
uf run setup
```

After that one line, everything in this repository is a `uf` command, and
`uf run` on its own lists them. The bootstrap cannot itself be one, and the
reason is structural rather than an oversight: `upstream/flow` is a *path*
dependency, so cargo cannot build `uf` until the submodule is checked out, and
`uf run` needs a built `uf`.

`tools/upstream/sync.sh` checks out only `rust_port/` from a shallow, blobless
clone of Meta's official Flow Rust port, which is not published to crates.io,
and applies `tools/upstream/patches/flow` on top — fixes uf needs that
`facebook/flow` has not taken yet. Nothing builds without it: `uf` parses and
type-checks Flow with that port and has no second backend.

`rust-toolchain.toml` pins `nightly-2026-08-01`. That is a requirement rather
than a preference — 23 crates in the port declare `#![feature(box_patterns)]`,
which the compiler removed around the 2026-09-01 nightly, and no stable
toolchain can build them.

One command runs everything CI runs:

```sh
uf run ci
```

[CONTRIBUTING.md](CONTRIBUTING.md) names the individual steps, and explains the
formatter fixtures before you review a pull request that adds one.

## License

[MIT](LICENSE).
