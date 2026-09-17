# Hosts

A uf application is meant to run on any JavaScript runtime. This page says which
ones it runs on **today**, which it does not, and what each of the latter is
waiting for — because a portability claim without a table like this is
marketing, and because for a long time the table did not exist and the claim
did.

The source of truth is `uf_runtime::HOSTS`, not this page. Every row below is
the row in that table; `uf inspect --json` prints the same thing under
`engines.hostSupport`, and the test-runner section repeats the rows for the
hosts it can start under `engines.testRunner.hostSupport`.
`crates/uf_runtime/src/tests.rs` fails if a row grades itself `implemented`
without naming the test that starts the runtime.

## The matrix

| Host | Runs a uf project | Flow loader | Permissions enforced | Checked by |
| --- | --- | --- | --- | --- |
| Node.js | **implemented** | `@uniflowed/host/register` | `read`, `write` | `crates/uf_cli/tests/testing.rs`, `crates/uf_cli/tests/permissions.rs`, and the whole library suite |
| Bun | **implemented** | `@uniflowed/host/bun-preload` | none | `crates/uf_cli/tests/bun_host.rs` |
| Deno | **implemented** | `@uniflowed/host/deno-preload` | all five | `crates/uf_cli/tests/deno_host.rs` |
| Edge / workers | experimental | — | none expressible | `tools/ci/edge-worker-smoke.sh` |
| Serverless | planned | — | none expressible | — |
| Container | planned | — | none expressible | — |
| `uf` (self-hosted) | planned | — | — | — |

One row carries a version floor: **a `uf build --adapter bun` deployment needs
Bun 1.4.2 or newer**, for the reason [Bun](#bun) gives. Nothing else here has
one, and running *on* Bun as a host does not.

"Checked by" is the column that matters. Node, Bun and Deno are marked
implemented because a test in this repository starts the binary, runs a Flow
project through it and watches the process finish. Bun's row said "implemented"
for a long time on the strength of a README sentence, and when a test was finally
written (ubugeeei-prod/uf#418) the preload turned out to be broken in two
independent ways at once — it could not load a single ordinary CommonJS
dependency, and it never exited. That is what an unchecked row is worth.

Deno's row was *experimental* until Deno 2.8 gave it a module hook, which is the
distinction the middle grade exists for: a uf project ran there, and the way it
was made to run — an ahead-of-time pass — had gaps a person could meet. See
[Deno](#deno) for what the hook closed and the two things it did not.

Edge is experimental in the same deliberately narrow sense. `uf build --adapter
edge` writes a Cloudflare Worker and CI serves that generated deployment under
Wrangler's local workerd. What does **not** exist is a source-level host: `uf
test` does not run a test file inside a Worker, a decision recorded under
[Edge and worker runtimes](#edge-and-worker-runtimes), and nothing is checked
against a remote deployment.

## The browser, which is a different question

`uf test --browser` runs a test file in a real page, and the browser is a host
in `uf_test`'s sense — a process uf starts, writes requests to, and reads
events from. It is deliberately **not** a row above, because the matrix grades
runtimes that *run a uf project* and a browser already runs one: it is the
target runtime, which [Three runtimes, not one](red-lines.md) is careful to
keep separate from the plugin runtime. A row for it would be answering a
question the column headings do not ask.

What it is, in the same shape as the rows above:

| | |
| --- | --- |
| Runs a test file | yes, one page load per file |
| Flow loader | the driver's module server, through the same `uf transform` |
| Permissions enforced | none, and a declared set is refused rather than ignored |
| Coverage | no: `NODE_V8_COVERAGE` is Node's own switch |
| Driver | Node, whatever `app.runtime.capabilityJsHost` says |
| Checked by | `crates/uf_cli/tests/browser_host.rs` and `tests/library/browser-layout.test.js` |

The driver row is the surprising one. A page cannot read a pipe, so a Node
process holds the browser's process handle and serves the page its modules; the
runtime under test is the browser, and the driver only shuttles JSON between a
pipe and a socket, so making it follow the project's Capability JS Host would
add an axis to a feature that has enough of them.

The browser binary itself is a dependency uf does not install: it drives one
that is already on the machine, named by `UF_BROWSER` or found on `PATH`, and
refuses the run by name when there is none. See
[Testing](app/guide/testing/$page.mdx) for what browser mode cannot do yet.

## What a project may name, and which key decides

Several keys in `uf.config.js` mention a runtime. The tool keys choose the
process that starts, and `capabilityJsHost` chooses it when none of them names
one:

| key | what it decides | what it accepts |
| --- | --- | --- |
| `runtime`, `build.runtime`, `test.runtime` | **the runtime a command starts**, at the release `uf.lock` locks and from the store: `build.runtime` for `uf dev`, `uf build` and `uf preview`, `test.runtime` for `uf test`, and `runtime` for `uf start`, `uf run`, `uf exec` and every command whose own key is absent — see [Environments](app/guide/env/$page.mdx) | `node`, `deno`, `bun`, as `name[@version]` |
| `app.runtime.capabilityJsHost.default` | the host `uf dev`, `uf test` and `uf build` start when no tool key names one, tried first and then the rest of `hosts` while `autoDetect` is on | `node`, `deno`, `bun` |
| `app.runtime.default`, `app.runtime.compatibility` | what this project *says* it is written for; it reaches `.uf/install.json` and `uf inspect --json` as the hosts that must be available | the rows above that have a Flow loader — today `node`, `deno`, `bun` |
| `app.runtime.deploy.adapter` | which artefact `uf build` writes | the deploy adapters, which are a longer list and a different question |

The `app.runtime.default` row used to accept all seven names in the table, `edge`,
`serverless`, `container` and `uf` included, and the default `compatibility`
claimed three of them. Nothing enforced the claim and nothing could: those four
rows have no Flow loader, so a uf project cannot import its own first file
there. What naming one changed was not the run — it went on being Node — but
what uf told the reader afterwards: `uf explain` printed the name as the
JavaScript host, and every `uf install` wrote it into `.uf/install.json` among
the hosts that must be available. That is this page's own sentence, produced by
uf: a name in an enum is not compatibility.

So a name uf has no host for is refused at the key that wrote it, and the
message names the level the table gives it, the two keys that do decide
something, and the issue tracking the host. The accepted set is read off
`uf_runtime::HOSTS` rather than listed a second time — `uf_config`'s tests fail
if the two disagree — so a host that earns a loader opens its name here by
earning it. See ubugeeei-prod/uf#246.

## Node.js

The reference host. `node --import @uniflowed/host/register app.js` installs
module hooks that transform every Flow module through `uf transform` as it
loads, with `--enable-source-maps` so a stack frame names the line you wrote.
`uf test`, `uf dev` and `uf build` all run here, and the `@uniflowed/*` suite
that `uf test` runs is a Node suite.

Its permission model is `--permission` with `--allow-fs-read` and
`--allow-fs-write`. There is no network dimension, no environment dimension, and
`--allow-child-process` is every program or none — see [Permissions](#permissions)
for what uf does about that.

## Bun

**A deployment built with `uf build --adapter bun` needs Bun 1.4.2 or newer.**
That is the floor `uf_runtime::BUN_MINIMUM` declares, the generated `server.js`
refuses anything older by name, and `.github/workflows/ci.yml` runs the Bun
adapter test on exactly that version as well as on the newest Bun — a floor no
job runs is a floor nobody knows is right.

The floor is about a parser, not a feature. React's published server build
contains a labelled statement in `else` position — `else a: if (…)`, in every
`react-dom-server*.production` file, the `bun`-conditioned one included — and
Bun's engine refuses it before 1.4.2 with
`SyntaxError: Cannot find scope for the label 'a'`. So an older Bun cannot
parse `handler.js` at all and the process dies at start-up, before it answers
anything.

It is not uf's syntax to lower, which is why this is a declared minimum rather
than a change to what uf emits: Node runs the identical bundle, the construct
has been legal JavaScript since ES1, uf's own transform emits none of it, and
asking for React's `bun` export condition resolves to a build that carries it
too. See ubugeeei-prod/uf#1048.

Running on Bun *as a host* — `uf dev`, `uf test`, the preload below — is not
affected and has no floor. Note also that the `node` adapter's output contains
the same construct, because it is the same React: a `--adapter node` directory
started with `bun server.js` fails the same way, and the fix is the same Bun.

`bun --preload @uniflowed/host/bun-preload app.js`, which is the same transform
reached through Bun's plugin API. The filter is
`packages/host/transform.js`'s `FLOW_MODULE_PATTERN`, which is `isFlowModule`
written as a pattern and pinned equal to it by
`packages/host/flow-modules.test.js` — Bun's `onLoad` has no way to say "not
mine", so the decision has to be made before the hook rather than inside it.

Bun has **no permission model of any kind**. A project that declares
`permissions` in `uf.config.js` and runs on Bun is refused, with a message
naming the hosts that can enforce it. That refusal is deliberate: a set that
four hosts enforce and one silently ignores is worse than no set at all,
because somebody would rely on it.

The preload keeps what it compiles in `.uf/cache/transform/`, under the same
key and in the same bytes as Node's loaders, source map included — so a warm
run on Bun compiles nothing, and either host warms the cache for the other.

Coverage does not work here. Bun implements no `NODE_V8_COVERAGE`, so
`uf test --coverage` says so rather than reporting a run of zeroes.

## Deno

A uf project runs on Deno 2.8 and newer through **the same loader** a Node new
enough for `registerHooks` uses: `@uniflowed/host`'s in-thread hooks,
`packages/host/internal/sync-hooks.js`. Deno implemented `node:module`'s
`registerHooks` in 2.8 and has never implemented `register()`, so the in-thread
hooks are the only ones it can take — and they are the ones Node prefers anyway.
`deno run --preload <path>/deno-preload.js app.js` installs them, with a path,
because Deno reads `--preload` as one. `uf test` starts every worker that way,
and `@uniflowed/vite`'s driver — `uf dev`, `uf build`, `uf preview` and
`uf start` — installs the same hooks itself on Deno, at the moment it installs
Node's.

What that loader does is Node's, with one difference in how it waits.
`registerHooks` runs in the importing thread and has to return a module's source
rather than a promise of it, so a module already in `.uf/cache/transform` is a
file read on both runtimes. A miss on Node is handed to a transform thread while
the importing thread sleeps on a shared cell; on Deno it is compiled by one
short-lived `uf transform` child instead, because the thread crashed Deno —
2.9.6 on Linux x86_64 panicked with `Fatal error in :0: unreachable code` in
every test of one CI run that compiled a module through it, and in none that
read the cache, while another run of the same commit passed. A crash that
depends on the run is worse than the ten milliseconds a child costs. The cache
is shared, one key and one framing in `packages/host/internal/flow-cache.js`, so
a module either runtime compiled is one the other reads.

Two things were Deno's to add, and both are about its sandbox. The variables
the loader reads are read so that one a worker was not granted counts as unset
rather than throwing `NotCapable`. And the check that `uf` is executable reads
the file's mode bits when Deno refuses `access(2)` without `--allow-sys`;
without that, a sandboxed Deno could not name its compiler, and a loader that
cannot name its compiler never reads or writes the cache.

### What the hook closed

Before 2.8 Deno had no hook to install anything in, and uf's Deno loader was an
ahead-of-time pass: compile every Flow module a run could reach into `.uf/deno/`
and hand Deno an import map pointing the original specifiers at the copies. A
pass compiles what it can enumerate; a hook is asked about every module. That
difference was the whole of this row's old grade, and every part of it went
with the pass:

* **a module reached some other way was still Flow** — a path computed at run
  time, a file a test writes and then imports. The hook compiles it like any
  other.
* **`uf test --watch` was refused**, because the pass ran once and a watch
  session would have kept re-running the tree it wrote. The worker re-imports
  each file under a fresh `?uf-run=` query, the hook is asked about that import,
  and an edit reaches the next run.
* **module mocking raised `UnsupportedError`**, because it needs synchronous,
  in-thread hooks. Those are what Deno has now, and `uft.mock` installs its
  interception through the same call.
* **`uf dev` and `uf build` did not run at all**: the driver had no loader, and
  `uf.config.js` imports `@uniflowed/config`, which is Flow.

### What it does not do

* **Coverage.** `NODE_V8_COVERAGE` is Node's switch. Deno counts through
  `--coverage`, in a profile format of its own with no source-map cache beside
  it for uf to map back through, so `uf test --coverage` on Deno says so rather
  than reporting zeroes.
* **A Deno older than 2.8.** There is no hook to install, so `uf test` reads
  `deno --version` before it starts a worker and refuses by version, naming the
  release it found and the one it needs. The loader makes the same check for
  anyone who starts Deno by hand.
* **A native addon first required after the hooks are installed.** Measured on
  Deno 2.9: while any `load` hook is registered — even one that only calls
  `nextLoad` — `require()` of a `.node` addon fails with `Invalid or unexpected
  token`, because Deno compiles the addon's bytes as script, and no answer a
  hook can give for the addon is taken. A `resolve`-only hook does not do it.
  The driver is arranged around this: Vite, and Rolldown's native binding with
  it, are static imports of the driver, and the hooks go in afterwards. A *test*
  on Deno that imports a package with a native addon still meets it, because a
  worker's hooks are installed before its first import. That one is Deno's to
  fix — its `load` chain has to leave an addon loadable — and uf's side of it is
  tracked by ubugeeei-prod/uf#246.
* **A dynamic-loader variable in the environment of a Deno you start yourself.**
  Deno will not let a process whose `--allow-run` names programs start one while
  `LD_LIBRARY_PATH`, `LD_PRELOAD` or any other `LD_*` or `DYLD_*` variable is set,
  and `node:child_process` then answers with no process at all (measured on Deno
  2.9.6). The loader starts `uf transform` for every module its cache does not
  hold, so under a scoped grant nothing new would compile. `uf test` leaves those
  variables out of every Deno worker it starts. A `deno run --preload
  @uniflowed/host/deno-preload` of your own needs them unset, and the error names
  the variable in the way.

### What a real Deno is started to check

`crates/uf_cli/tests/deno_host.rs` runs the binary rather than reasoning about
it:

| What | Deno 2.8 and newer |
| --- | --- |
| the `node:` built-ins the transform client imports, `spawnSync` included | load |
| `import "@uniflowed/test"` with no loader | **fails**: it resolves from `node_modules`, and the package is Flow |
| Flow syntax with no loader | `SyntaxError`, against the line you wrote |
| a relative import, a bare `@uniflowed/*` import, a path computed at run time and a re-import under a query, through the preload | compile, under the permission set uf translates rather than `-A` |
| a module already in the cache, with `uf transform` not permitted | loads: a warm run starts no compiler |
| a module the Node loader compiled, with `uf transform` not permitted | loads from the same cache |
| `uf test`, `uf test --json`, a reasoned skip, `uft.mock` and `uft.unmock` | pass |
| `uf test --watch`, after an edit to a dependency under it | reports the edit on the next run |
| `uf build` of the served-app fixture, then `uf start` | builds, and answers the served-app questions |
| a Deno whose `--version` says 2.7 | refused by version, before a worker starts |

## What a dependency's host conditions get you

A dependency can ship one target per condition: `node` against `browser`, `bun`
or `deno` against neither, and `flow` against compiled JavaScript. `uf check`
answers the conditions that are true of uf's portable module graph: `flow` and
`import`. It answers no host condition.

That means a package written as `{ "bun": "./b.js", "import": "./i.js" }` is
typed as `./i.js`: right on Node, wrong on Bun, and intentionally visible as the
cost of one shared graph. A package that publishes only host branches resolves
to nothing and is reported separately from a missing package by
`uf check --json` as a host-conditional module. Which host graph the checker
should resolve, if any, is ubugeeei-prod/uf#735.

### Permissions, and why there is no `-A`

Deno **is the only host that enforces the whole permission set**, and the model
in `uf.config.js` was designed against its five categories.

It is also the only host whose default is deny, which means uf cannot hand it
"no permission model" the way it hands that to Node and Bun. There are only two
things to pass: the toolchain's own access, or `-A`. `uf test` passed `-A`
unconditionally for a long time, and `-A` is a grant nothing later on the
command line takes back — so every run on the one host that could enforce the
model began by turning it off, and narrowing was something a project had to
remember to ask for.

So a Deno run gets the toolchain's access whether or not the project declared
anything: the project root, the directory its packages resolve from, `.uf`, the
one `uf` binary the loader compiles through — granted by name, which is the
scope Node's `--allow-child-process` cannot express — and the environment
variables uf itself set on the worker, plus `NODE_V8_COVERAGE`, which Deno's own
`node:child_process` reads whenever a child is started with an explicit
environment. `uf explain test` prints them. A test that wants more than that — the network, `/etc`, a variable nobody
declared — asks for it in `uf.config.js`, which is what the permission model is
for.

## Edge and worker runtimes

Cloudflare Workers, Vercel Edge, Deno Deploy: these are not source-level hosts
in the way Node, Bun and Deno are. A worker runtime has no loader hook, no
child process to run `uf transform` in, and no filesystem, so the transform
must happen at deploy time and the permission set has nothing to translate into
— the platform's sandbox *is* the permission model, and it is not uf's to
configure.

`infra/cloudflare/` is the documentation site's own deployment and is not an
application target; it should not be read as one.

This is the ahead-of-time question Deno's loader asked until Deno 2.8 gave it a
hook. A worker runtime has no hook to wait for, so here the answer stays ahead
of time. Tracked by ubugeeei-prod/uf#246.

### What the worker smoke checks

Worth stating in the column's own terms rather than leaving as an em dash.
`uf build --adapter edge` does write a Cloudflare Worker — `worker.js`,
`wrangler.json` and `static/` — and `tests/library/deploy.test.js` drives that
handler's answers and compares them with `uf start`'s. That is still a Node
test, not a Worker test.

`tools/ci/edge-worker-smoke.sh` is the Worker test. It serves three Workers,
each under `wrangler dev --local`:

| Worker | What is asked of it |
| --- | --- |
| the `served-app` fixture, built with `--adapter edge` | the home page, a prerendered page, a dynamic route, `GET` and `POST` on a route handler, the not-found boundary, and a hashed client script from the assets binding. After those requests, the Worker's own log must hold the access line for `/api/health`, with a request id uf generated, filed at `info` |
| the `rsc-split-app` fixture, built the same way | a server action, called with the id the build minted and a cookie the action reads, and the same call from another origin, refused with 403 |
| a probe generated from `packages/vite/internal/worker-builtins.js` | every Node built-in that table says a Worker provides only as a stub must still throw at the table's compatibility date |

The log check is there because of a real bug. uf's default sink writes every
level to `console.error`, because stdout is a protocol in the process that runs
`uf start`. A Worker's log store files a line by the method that wrote it, so
under `wrangler dev` every request's access line came out as an `ERROR`: a
Worker answering 200s read as one failing on every request. The generated
`worker.js` now installs a logger that writes each level through its own
method, unless the application installed one first. It does so only where
`navigator.userAgent` is `Cloudflare-Workers`. uf's adapter parity test imports
the same file under Node and reads its answers from stdout, which is where
`console.info` writes on Node, so everywhere else the default stays.

### The limits a Worker imposes, by name

A Worker has no filesystem and no processes, and at the compatibility date uf
writes it provides some Node built-ins only as stubs. Those are measured, not
assumed. Each built-in was imported into a Worker and one function of it
called:

- **Stubs, every function throws:** `child_process`, `cluster`, `fs`, `http`,
  `http2`, `https`, `repl` and `wasi`. The error is `[unenv] <function> is not
  implemented yet!`.
- **Partial, not listed:** `net` connects and refuses `createServer`, and
  `dgram`, `inspector` and `worker_threads` construct objects without throwing.
  A build cannot name a module that half works.

What uf does with that:

- **At the build**, `uf build --adapter edge` warns about each stub the server
  bundle reaches, and names the importing file: project code first, a
  dependency by its package name. It is a warning and not a refusal, because a
  deployment with a `wrangler.json` of its own at a newer date may have the
  module, and uf cannot see that deployment.
- **At request time**, a call that reaches a stub answers the same fixed 500 as
  any other failure. The log line names the function as a Node API this Worker
  does not provide.
- **In CI**, the smoke measures the table again. `uf_cli`'s tests pin the
  table's date to the one `wrangler.json` is written with, so a bumped date
  cannot keep an old measurement.

The durable route cache's filesystem store is refused by name at the build, as
it always was.

### Why `uf test` does not run on workerd

Decided against for now, with the reasons written down. Test files are Flow and
have to load at run time, and workerd has no module hook: the nearest thing is
its module fallback service, which Miniflare exposes as
`unsafeModuleFallbackService` behind workerd's experimental flag. The test
worker also speaks its protocol over stdin and stdout, which a Worker does not
have. And what differs on this target is the built output — `worker.js`, its
`wrangler.json`, the assets binding — which is exactly what the smoke runs. A
workerd host would be a Node driver running Miniflare, answering module
requests through `uf transform`, with the worker protocol carried over a
binding rather than a pipe. That is ubugeeei-prod/uf#1029.

Tracked by ubugeeei-prod/uf#246.

## Serverless and containers

Neither needs a host binding: both run the `node` adapter's output, and
`uf build --adapter container` already writes what a container runs. What is
missing is a test that runs it *there*, which is a deployment question rather
than a runtime one. Tracked by ubugeeei-prod/uf#391.

## Permissions

Declared once, in `uf.config.js`, and translated per host:

```js
// @flow
export default defineConfig({
  permissions: {
    read: ["./fixtures"],
    write: ["./.uf"],
    net: ["registry.npmjs.org:443"],
    env: ["CI"],
    run: ["git"],
  },
});
```

No `permissions` key means no permission model — the toolchain uf has always
been. `permissions: {}` is a meaningful and legitimate thing to write: it grants
nothing beyond what uf itself needs to load and transform the project. There is
deliberately no way to spell "everything".

| Permission | Node.js | Bun | Deno |
| --- | --- | --- | --- |
| `read` | `--allow-fs-read` | — | `--allow-read` |
| `write` | `--allow-fs-write` | — | `--allow-write` |
| `net` | **refused** | — | `--allow-net` |
| `env` | **refused** | — | `--allow-env` |
| `run` | **refused** | — | `--allow-run` |

**Refused, not ignored.** Node has no network or environment dimension at all,
and `--allow-child-process` is all programs or none — so translating a `run`
list into it would be a widening, and dropping `net` would produce a run that
reads as sandboxed and lets a test post anywhere. A set a host cannot enforce
stops the run with a message naming a host that can.

### What a permission set does not restrict

**It does not narrow a run's access to the project's own files.** A host has to
read the project to run it — the module graph, `node_modules`, the loader, the
worker — so those reads are uf's own and are added to whatever the project
declared. What a permission set denies is *the rest of the machine*: `~/.ssh`,
`~/.aws`, `/etc`, the network, the environment.

`uf explain test` prints the grants uf adds alongside the ones the project
declared, because a permission model whose additions are invisible is one nobody
can check.

### A compiled binary

`uf build --compile` embeds one of these hosts and writes an executable, so the
permission set has to survive a machine uf will never see. It does, on one of
the two backends.

| backend | how the set travels | what happens when it cannot |
| --- | --- | --- |
| `node --build-sea` | `execArgv` in the SEA config, so `--permission` and its `--allow-fs-*` flags are in force from the executable's first line | a category Node cannot enforce is refused, exactly as it is for a run |
| `bun build --compile` | nowhere: Bun has no permission model and no equivalent of `execArgv` | the compile is refused, naming Node as the backend that can |

Two things about the compiled case differ from a run, and both are in the
direction that matters.

**uf adds no grants of its own.** The grants below exist because a *run* has to
load the project — the module graph, `node_modules`, the loader thread, the
`uf transform` child. A compiled binary has none of that: the bundle is one
file inside the executable and the output directory is bytes beside it. So the
declared set is the whole set, and the two holes named below are not in it.

**Nothing downstream can check it.** A run can be inspected with
`uf explain test` on the machine it runs on; a binary is a file somebody
receives. That asymmetry is why `--compile` on Bun refuses rather than warns.

### The holes, named

On Node, uf's own Flow loader needs two grants that Node itself warns about at
startup:

- `--allow-worker`, because the Flow loader compiles on a thread: on a cold
  cache the in-thread hooks hand each module to a transform thread, and a Node
  without `registerHooks` runs the hooks themselves on the loader thread
  `register()` starts. Without it the first module that has to be compiled
  fails with `ERR_ACCESS_DENIED`;
- `--allow-child-process`, because every Flow module is transformed by a
  `uf transform` child.

Neither is scoped — Node cannot say *which* thread or *which* program — so a
test running under a Node permission set can still start a program, and that
program is not bound by the set. This is why a project's own `run` list is
refused on Node rather than translated: uf can disclose the hole it needed, and
it can decline to pretend the hole has a shape. Deno's `--allow-run=<program>`
is the same grant with a name on it, which is the difference the two rows
above describe.

Deno's loader needs the second of those grants too — it compiles through a
`uf transform` child as well — and gets it with the name on:
`--allow-run=<the uf binary>`. That child is no more bound by the set than a
child is on Node, which is true of any program a sandbox lets a process start;
the difference is that it is the one program uf named rather than every
program there is.
