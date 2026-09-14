<div align="center">

<img width="200" src="brand/uniflowed-mark.svg" alt="">

# uf

**Build the strongest React development experience with Modern Flow.**

[Documentation](https://docs.uniflowed.dev) ·
[Start](https://docs.uniflowed.dev/guide/start) ·
[Why uf](https://docs.uniflowed.dev/guide/why-uf) ·
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

Using that syntax without uf is an assembly job. Babel to strip the types, a
preset that understands `component`, a plugin for the React Compiler, a bundler,
a second config to tell the bundler about the first, a test runner with its own
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

**uf is pre-release software.** Every version so far is a `0.0.0-alpha`
prerelease, and it changes under you: commands, config keys and package exports
move without a deprecation cycle, and no version promises compatibility with the
one before it. [Releases](https://github.com/ubugeeei-prod/uf/releases) lists
them, and `uf --version` names the one you have.

This file names no version and lists no current gaps, because it is copied into
every release archive and would be wrong about the next one. Each question below
is answered where the answer is kept up to date — beside the code that decides
it, or on the page that documents it:

| If you are asking | It is answered in |
| --- | --- |
| Does it run on my machine? macOS and Linux on x86-64 and ARM64; no Windows build, and WSL2 works | [Install](https://docs.uniflowed.dev/guide/install) |
| Which JavaScript hosts — Node.js, Bun, Deno, edge — and what each is still waiting for | [docs/hosts.md](docs/hosts.md) |
| What uf refuses to do on purpose, and every gap with its issue number | [What uf does not do](https://docs.uniflowed.dev/guide/scope) |
| How it stands against Next.js, Vite, Bun and `create-react-app`, losses included | [uf compared](https://docs.uniflowed.dev/guide/compare) |
| Which `@uniflowed/*` packages are on npm, and under which tag | [Packages](https://docs.uniflowed.dev/reference/packages) |
| What is being worked on next | [docs/roadmap.md](docs/roadmap.md) |

## Install

```sh
curl -fsSL https://setup.uniflowed.dev | sh
```

The script picks the archive for your platform, verifies it, and puts `uf`,
`ufr` and `ufx` on your `PATH`. It is short enough to read first, and it is
served from `https://setup.uniflowed.dev/install.sh` if you would rather fetch it
before running it. [Install](https://docs.uniflowed.dev/guide/install) says what
it verifies, how to pin a version, and how to build from source.

On Nix, the repository is a flake and that is the whole install:

```sh
nix run github:ubugeeei-prod/uf          # without installing it
nix profile install github:ubugeeei-prod/uf
```

It pins the Rust toolchain, fetches Meta's Flow port at the commit uf was built
against and applies uf's patches to it, so the build is the same everywhere. For
NixOS, nix-darwin and home-manager there are `nixosModules.default`,
`darwinModules.default` and `homeManagerModules.default`, all taking
`programs.uf.enable`, and an `overlays.default` if you would rather have
`pkgs.uf` and place it yourself.

uf still needs a JavaScript host, because Vite and your test bodies run there.
`uf info` prints the one it found.

## Five minutes

```sh
uf new hello
cd hello
uf install
uf dev          # a dev server, with Fast Refresh through `component`
uf build        # dist/, prerendered, with real gzip and brotli sizes
uf test
```

`uf new` writes the project and nothing else — no `node_modules`, no lockfile,
no git history:

```
  hello
  ├─ app
  │  ├─ $layout.js
  │  ├─ $page.js
  │  ├─ $page.test.js
  │  ├─ Counter.js
  │  └─ useCounter.js
  ├─ .gitignore
  ├─ app.js
  ├─ package.json
  └─ uf.config.js
```

Three of those nine are a demonstration you read once and delete. There is no
`vite.config.ts`, no `babel.config.js`, no `.eslintrc`, no `.flowconfig` and no
`vitest.config.ts`, and there is not going to be one.

`uf install` hands the work to the package manager the project already uses —
npm, pnpm, Yarn or Bun — with lifecycle scripts refused: the manager is passed
`--ignore-scripts`, and a manifest that declares `scripts` of its own is rejected
before anything is fetched. Every command that runs a manager prints which one it
chose and why.

[Your first project](https://docs.uniflowed.dev/guide/project) walks through
every file above, and [Build a reading list](https://docs.uniflowed.dev/guide/tutorial)
builds one application end to end, with what each command printed.

## What the binary does

Every command is in the release binary, and none of them needs a line in
`package.json`. [Commands](https://docs.uniflowed.dev/reference/cli) is the full
reference, with every flag and exit code.

| | | Guide |
| --- | --- | --- |
| `uf new`, `uf init` | Scaffold an application into a new directory, or into this one; `--lib` scaffolds a library | [Your first project](https://docs.uniflowed.dev/guide/project) |
| `uf dev`, `uf build` | Vite's dev server; client and server bundles, prerendered routes, a size report and the build manifest | [Dev and build](https://docs.uniflowed.dev/guide/dev) |
| `uf preview`, `uf start` | Serve that build — through Vite, or through uf's own server with no bundler in the process | [Targets](https://docs.uniflowed.dev/guide/targets) |
| `uf test` | Scheduling and reporting in Rust, one file per worker, bodies on the host | [Testing](https://docs.uniflowed.dev/guide/testing) |
| `uf fmt`, `uf lint` | Flow printed from Meta's parser to match Prettier; uf's rules and Flow's lints, in one pass | [Formatting and linting](https://docs.uniflowed.dev/guide/format) |
| `uf check` | `uf lint`, then Flow's own inference. uf has no second opinion about your types | [Type checking](https://docs.uniflowed.dev/guide/check) |
| `uf install`, `uf add`, `uf remove`, `uf update`, `uf why`, `uf ls`, `uf audit`, `uf search`, `uf pm`, `uf patch`, `uf catalog` | The project's own package manager, with install scripts refused | [Dependencies](https://docs.uniflowed.dev/guide/dependencies) |
| `uf run` (`ufr`), `uf exec` (`ufx`) | A task from `uf.config.js`, and everything it depends on; a package's binary, fetched only when you say so | [Tasks](https://docs.uniflowed.dev/guide/tasks) |
| `uf env`, `uf use`, `uf self-update` | The runtimes a project pins, and the uf that runs it | [Environments](https://docs.uniflowed.dev/guide/env) |
| `uf lsp` | The language server, over stdio | [Editors](https://docs.uniflowed.dev/guide/editors) |
| `uf mcp` | The checks, the tests and the formatter as MCP tools, over stdio, for an agent | [Agents](https://docs.uniflowed.dev/guide/agents) |
| `uf info`, `uf inspect`, `uf explain` | What uf found, what your config resolved to, and which provider does each stage of a command | [Commands](https://docs.uniflowed.dev/reference/cli#project) |
| `uf routes`, `uf i18n`, `uf doc`, `uf prepare`, `uf clean`, `uf release`, `uf publish`, `uf completion` | The route table, the message catalogue, API docs, the pre-commit checks, cleaning up, and releasing | [Commands](https://docs.uniflowed.dev/reference/cli) |

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
This README keeps them apart and does not restate the lists, because a copy of a
list is one more thing to keep true:

| | What it means | Where it is kept |
| --- | --- | --- |
| **Implemented** | Reachable from the binary, covered by a test, documented. The command and config references describe only what exists | [Commands](https://docs.uniflowed.dev/reference/cli), [`uf.config.js`](https://docs.uniflowed.dev/reference/config) |
| **Experimental** | Implemented, reachable, and expected to change — said on the page that documents it | [Packages](https://docs.uniflowed.dev/reference/packages), and each guide |
| **Planned** | Not written. Every gap carries an issue number, and the roadmap sorts them P0–P3 | [What uf does not do](https://docs.uniflowed.dev/guide/scope), [docs/roadmap.md](docs/roadmap.md) |

A declared API that throws is not a feature, and this repository has a check that
says so: `tools/ci/publishable.sh` refuses an implemented package that is on its
way to nobody. The three ways that standard fails, and the table of where uf's
own north star is not true yet, are at the top of
[docs/roadmap.md](docs/roadmap.md).

If you write TypeScript, read [uf compared](https://docs.uniflowed.dev/guide/compare)
first: the syntax argument is worth nothing to you and most of the other rows are
a downgrade.

## The library surface

The Flow packages ship as `@uniflowed/*` modules — the router, the server, typed
HTTP and query caching, effects, state, validation, forms, headless UI
primitives, StyleX styling, the test runner and the rest. "The server" there
means a **BFF** — route handlers, server actions and rendering, the backend
*for this frontend*. It is deliberately not an application backend, and
[architecture red lines](docs/red-lines.md) says why that is a boundary rather
than a gap. They are plain Flow with no native bindings, so what runs in the
browser is what you can read, and `uf_lib` in this repository is the registry
they are all declared in.

[Packages](https://docs.uniflowed.dev/reference/packages) is the annotated list,
including which are on npm and which resolve only through this repository's
workspace.

## Documentation

[docs.uniflowed.dev](https://docs.uniflowed.dev) is the manual. It is built by
`uf build` from `docs/` in this repository — a real uf project, which is the
point of it. It is arranged by what a reader came for:

| If you want to | Start at |
| --- | --- |
| get uf running and build one application with it | [Start](https://docs.uniflowed.dev/guide/start) |
| decide whether your team should use it, and what it costs | [Why uf](https://docs.uniflowed.dev/guide/why-uf) |
| route, load data, render, style and test an application | [Build an app](https://docs.uniflowed.dev/guide/build-an-app) |
| run it on a server, a static host, a phone or a terminal | [Targets](https://docs.uniflowed.dev/guide/targets) |
| own the pipeline: runtimes, dependencies, tasks, CI, editors and agents | [The toolchain](https://docs.uniflowed.dev/guide/toolchain) |
| move an application you already have | [Migrating to uf](https://docs.uniflowed.dev/guide/migrate) |
| look up a flag, a config key or an export | [Reference](https://docs.uniflowed.dev/reference) |

In the repository:

| | |
| --- | --- |
| [docs/architecture.md](docs/architecture.md) | The crates, the Flow parser boundary, and why the toolchain is pinned to a nightly |
| [docs/hosts.md](docs/hosts.md) | Each JavaScript host, what works on it, and what each gap is waiting for |
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

`rust-toolchain.toml` pins a nightly Rust toolchain. That is a requirement rather
than a preference — crates in the port declare `#![feature(box_patterns)]`, which
recent nightlies removed, and no stable toolchain can build them.
[docs/architecture.md](docs/architecture.md) has the exact pin and why it moves
when it moves.

One command runs everything CI runs:

```sh
uf run ci
```

[CONTRIBUTING.md](CONTRIBUTING.md) names the individual steps, and explains the
formatter fixtures before you review a pull request that adds one.

## License

[MIT](LICENSE).
