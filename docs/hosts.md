# Hosts

A uf application is meant to run on any JavaScript runtime. This page says which
ones it runs on **today**, which it does not, and what each of the latter is
waiting for — because a portability claim without a table like this is
marketing, and because for a long time the table did not exist and the claim
did.

The source of truth is `uf_runtime::HOSTS`, not this page. Every row below is
the row in that table; `uf inspect --json` prints the same thing under
`engines.hostSupport`, and `crates/uf_runtime/src/tests.rs` fails if a row grades
itself `implemented` without naming the test that starts the runtime.

## The matrix

| Host | Runs a uf project | Flow loader | Permissions enforced | Checked by |
| --- | --- | --- | --- | --- |
| Node.js | **implemented** | `@uniflowed/host/register` | `read`, `write` | `crates/uf_cli/tests/testing.rs`, `crates/uf_cli/tests/permissions.rs`, and the whole library suite |
| Bun | **implemented** | `@uniflowed/host/bun-preload` | none | `crates/uf_cli/tests/bun_host.rs` |
| Deno | **experimental** | uf's ahead-of-time transform and import map | all five | `crates/uf_cli/tests/deno_host.rs` |
| Edge / workers | planned | — | none expressible | — |
| Serverless | planned | — | none expressible | — |
| Container | planned | — | none expressible | — |
| `uf` (self-hosted) | planned | — | — | — |

"Checked by" is the column that matters. Node and Bun are marked implemented
because a test in this repository starts the binary, runs a Flow project through
it and watches the process finish. Bun's row said "implemented" for a long time
on the strength of a README sentence, and when a test was finally written
(ubugeeei-prod/uf#418) the preload turned out to be broken in two independent
ways at once — it could not load a single ordinary CommonJS dependency, and it
never exited. That is what an unchecked row is worth.

Deno's row is *experimental* rather than implemented, which is the distinction
the middle grade exists for: a uf project runs there, and the way it is made to
run has a gap a person can meet. See [Deno](#deno) for what the gap is.

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

Three keys in `uf.config.js` mention a runtime, and only one of them chooses
the process that starts:

| key | what it decides | what it accepts |
| --- | --- | --- |
| `app.runtime.capabilityJsHost.default` | **the host `uf dev`, `uf test` and `uf build` actually start**, tried first and then the rest of `hosts` while `autoDetect` is on | `node`, `deno`, `bun` |
| `app.runtime.default`, `app.runtime.compatibility` | what this project *says* it is written for; it reaches `.uf/install.json` and `uf inspect --json` as the hosts that must be available | the rows above that have a Flow loader — today `node`, `deno`, `bun` |
| `app.runtime.deploy.adapter` | which artefact `uf build` writes | the deploy adapters, which are a longer list and a different question |

The second row used to accept all seven names in the table, `edge`,
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

Coverage does not work here. The preload transforms with `sourceMap: false` and
Bun implements no `NODE_V8_COVERAGE`, so `uf test --coverage` says so rather
than reporting a run of zeroes.

## Deno

A uf project runs on Deno, and it gets there by a different road from the other
two. Node has `register()` and Bun has `Bun.plugin`; **Deno has no module hook
at all**, so nothing can be installed in it that transforms a module as the
runtime asks for it. The transform has to have already happened.

So `uf test` on Deno runs an **ahead-of-time pass** before the host starts. It
compiles every Flow module the run can reach — the project's own source as
`uf`'s scan reports it, and the `@uniflowed/*` packages that source reaches —
into `.uf/deno/`, mirroring the layout it came from, and writes an **import
map** beside it that points the original specifiers at the compiled copies.
`crates/uf_cli/src/commands/deno_loader.rs` is that pass.

One artefact answers both halves of the problem. A map that has to name
`@uniflowed/test`'s new location is a map that has named `@uniflowed/test` — so
the same file that fixes the Flow syntax fixes the bare specifier, and neither
needed a mechanism of its own.

Three kinds of entry are in the map, and the third is the one worth knowing
about:

* one per `@uniflowed/*` export, plus a trailing-slash key per package;
* the project root as a prefix, `file:///<root>/` → `file:///<root>/.uf/deno/`,
  which is what redirects the worker's own `import(pathToFileURL(file))`. A
  prefix rather than one key per file, because the worker appends
  `?uf-run=<generation>` to bust its module cache and an exact key would not
  match a URL carrying a query;
* **the output directory mapped to itself.** `.uf/deno` is *under* the project
  root, so the entry above would redirect an already-redirected module a second
  time, into `.uf/deno/.uf/deno/…`. An import map resolves the longest matching
  prefix and has no way to spell an exception, so an identity entry for the
  longer path is the exception.

### What is experimental about it

A hook is asked about every module; a pass compiles what it can enumerate.
That difference is the whole of this row's grade:

* **a module reached some other way is still Flow.** A path computed at run
  time, a file a test writes and then imports — the pass never saw it, and Deno
  meets it as a syntax error.
* **`uf test --watch` is refused.** The pass runs once, before the host starts,
  so a watch session would keep re-running the modules the first pass wrote. A
  loop that answers about the code as it was an hour ago is worse than one that
  will not start.
* **no coverage.** `NODE_V8_COVERAGE` is Node's switch and Deno has no
  equivalent, exactly as on Bun.

Tracked by ubugeeei-prod/uf#246.

### What a real Deno is started to check

`crates/uf_cli/tests/deno_host.rs` runs the binary rather than reasoning about
it, and records both sides of each obstacle:

| What | Deno |
| --- | --- |
| `node:` built-ins — `child_process`, `fs`, `path`, `readline` | work, so the transform client was never the obstacle |
| `import "@uniflowed/test"` with no map | **fails**. Deno 1.31 resolves no bare specifier from `node_modules` at all; a current Deno 1.x resolves it, reaches `packages/test/index.js` and cannot parse it, because the package is Flow |
| the same import, through the generated map | resolves and loads |
| Flow syntax with no pass | `SyntaxError`, against the line you wrote |
| the same file, compiled by the pass | runs |
| a global `process` | **absent** as of Deno 1.46; `node:process` has the same object, which is why `@uniflowed/test`'s worker imports it rather than reading the global |

The `process` row is a fact about a *version*, so its test gates itself on the
major version and says so when it steps aside — asserting a version's behaviour
on a version nobody ran the test against would be the unchecked claim this page
exists to end.

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
anything: the project root, the directory its packages resolve from, `.uf`, and
the environment variables uf itself set on the worker. `uf explain test` prints
them. A test that wants more than that — the network, `/etc`, a variable nobody
declared — asks for it in `uf.config.js`, which is what the permission model is
for.

## Edge and worker runtimes

Cloudflare Workers, Vercel Edge, Deno Deploy: **there is no host at all**, and
none of the machinery the other three use would apply. A worker runtime has no
loader hook, no child process to run `uf transform` in, and no filesystem, so
the transform must happen at deploy time and the permission set has nothing to
translate into — the platform's sandbox *is* the permission model, and it is not
uf's to configure.

`infra/cloudflare/` is the documentation site's own deployment and is not an
application target; it should not be read as one.

This is the same ahead-of-time question Deno's loader asks, and answering it
once serves both. Tracked by ubugeeei-prod/uf#246.

### Nothing here has been checked on a worker runtime

Worth stating in the column's own terms rather than leaving as an em dash.
`uf build --adapter edge` does write a Cloudflare Worker — `worker.js`,
`wrangler.json` and `static/` — and `tests/library/deploy.test.js` drives that
handler's answers and compares them with `uf start`'s. **None of that starts a
worker runtime.** The handler runs in Node, in process, and a `wrangler.json`
being the shape Cloudflare documents is not the same claim as Cloudflare
accepting it.

Nor could it be checked here yet. No test in this repository starts `workerd`,
`wrangler dev` or any other worker runtime; the machine uf is developed on has
none installed; and CI installs Node, Bun and Deno and nothing else — see
`.github/workflows/ci.yml`. The one place `wrangler` is run at all is
`docs.yml`, which deploys the documentation site, and a `--dry-run` there
bundles a script rather than executing one.

So this row stays **planned** with an empty "Checked by", which is what that
column is for. Grading it on the strength of an in-process handler test would
be the same move Bun's row made for a year on the strength of a README
sentence, and [the matrix](#the-matrix) says what that was worth.

Tracked by ubugeeei-prod/uf#246, which is also where the shape a real edge host
would take is written down.

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

- `--allow-worker`, because `register()` runs module hooks on a loader thread,
  and without it the very first import fails with `ERR_ACCESS_DENIED`;
- `--allow-child-process`, because every Flow module is transformed by a
  `uf transform` child.

Neither is scoped — Node cannot say *which* thread or *which* program — so a
test running under a Node permission set can still start a program, and that
program is not bound by the set. This is why a project's own `run` list is
refused on Node rather than translated: uf can disclose the hole it needed, and
it can decline to pretend the hole has a shape. Deno's `--allow-run=<program>`
is the same grant with a name on it, which is the difference the two rows
above describe.
