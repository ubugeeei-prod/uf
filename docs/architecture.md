# Architecture

## Product Shape

uniflowed is the Unified Toolchain for Flow (React): a zero-config toolchain
that gives Flow React apps a single fast native interface. The CLI surface is
intentionally broad, while each engine is isolated behind small Rust crate
boundaries so we can deepen behavior without destabilizing the user-facing
command model.

The product bar is deliberately aggressive: `uf` should beat Vite+ on Flow
React DX, framework completeness, build latency, and dev-server feedback loops,
while using Vite Task for cached task execution and beating Bun Test/Vitest on
native test throughput, runtime startup, package manager performance, and
integrated feature coverage.

## Crates

- `uf_cli`: command router for `uf`
- `uf_config`: zero-config defaults and `uf.config.js` loading
- `uf_assets`: image resizing and re-encoding, and the font metrics behind `Image` and `Font`
- `uf_bundle`: bundle size measurement and `build.budgets` enforcement
- `uf_check`: Flow type inference, driven from `upstream/flow`
- `uf_flow`: Flow parser/typechecker adapter boundary over `upstream/flow`
- `uf_fmt`: native formatter runner
- `uf_infra`: Arena, FxHash, PHF, SIMD UTF-8, SmallVec, CompactString, and the byte bound every `.uf/cache/` directory is swept to
- `uf_lib`: native standard library registry exposed to Flow
- `uf_lint`: native lint runner and framework rules
- `uf_pm`: self-hosted package manager plan, lockfile, and content-addressed store contracts
- `uf_project`: project discovery and `create` templates
- `uf_rm`: runtime manager inference, host detection, and adapter application contracts
- `uf_runtime`: Capability JS Host, WinterTC, and deploy-anywhere runtime contract
- `uf_std`: native stdlib modules for WinterTC-compatible Flow wrappers
- `uf_task`: the `uf run` task graph, its concurrency limit, and the content-hash cache under `.uf/cache/task`
- `uf_test`: native test discovery, scheduling, watch invalidation, and runner core
- `uf_transform`: Flow → JavaScript — official parser, Flow's lowering rules, the official React Compiler, oxc for JSX and code generation
- `uf_tui`: what `@uniflowed/tui` is — the renderer itself is Flow

These are the crates that survive. `uf` used to ship a Rust crate per library
surface — `uf_motion`, `uf_orm`, `uf_markdown`, `uf_temporal`, `uf_web` and ten
more — each holding a `serde` struct that described a feature rather than
implementing one, and each duplicating a `packages/*/index.js` module
that was the real thing. Effect and validator are now plain `.js` + Flow
packages rather than Rust crates. Twelve had no consumer at all; three existed so
`uf inspect` could print their `::default()`. They are gone: the JavaScript in
`uf_lib/lib/core` is the source of truth for what those modules are.

## Flow Syntax Authority

`uf` never reimplements Flow's grammar, and it does not host one either.
`uf_flow` is a thin adapter over exactly one backend: Meta's Flow Rust port at
`upstream/flow/rust_port/crates/flow_parser`, vendored through the
`upstream/flow` submodule. It is the same code Flow itself runs, and it parses
`component`/`hook`/`renders`/`match`, modern variance (`readonly`, `in`, `out`)
and `extends` bounds natively.

There is no second backend and no feature flag selecting one. A build of `uf`
that spoke a different dialect would make `uf lint` and `uf check` disagree with
the grammar uf documents, which is exactly what happened while a QuickJS-hosted
build of Flow's JavaScript parser stood in for the port on stable toolchains:

- it predated component syntax, so `uf` rewrote the user's source before parsing
  and reported every diagnostic against the rewritten text;
- its AST deserialization predated `readonly`, so writing the only spelling Flow
  accepts for a read-only property crashed `uf lint`;
- it budgeted a 256 kB stack from wherever its runtime was created, so linting a
  few hundred valid files in parallel failed as `SyntaxError: stack overflow` —
  scaling with parallelism rather than input size.

Removing it also removes an embedded JavaScript engine, and the `libquickjs-sys`
C dependency with it, from the release binary.

### What the port can and cannot give us yet

Measured against `rustc 1.100.0-nightly (5db7f4be8 2026-09-01)`:

- **The parser's own nightly requirement is expiring.** `flow_parser`'s only
  unstable feature is `never_type`, and that compiler already reports it as
  *stable since 1.100.0-nightly*. The type checker below is what keeps the
  whole workspace on a pinned nightly.
- **The type checker builds and runs, on a pinned nightly.** 23 crates in the
  port, including `flow_common` and everything under `flow_typing*`, declare
  `#![feature(box_patterns)]`, and that feature was *removed* from the compiler
  around the 2026-09-01 nightly — so the typing crates fail on the floating
  `nightly` channel. They compile on `nightly-2026-08-01` (rustc 1.99.0-nightly),
  which is what `rust-toolchain.toml` pins for the whole workspace. The pin is
  not a preference: `uf` parses and type-checks Flow with the official port, and
  no stable toolchain can build it. The `Upstream Flow` CI job builds the parser
  on the floating channel as an early warning, so the pin moves deliberately
  rather than being discovered when it breaks.

  Measured on that toolchain: Flow's builtin library definitions merge into a
  master context in **68 ms** (once, then cached), and checking a file costs
  about **4 ms**. Assigning a string to a `number`, passing a `string` where a
  `number` is declared, and dereferencing a `?string` are each reported; a
  well-typed file reports nothing.

  Two things an embedder has to know, neither of them documented upstream:
  `flow_parser::file_key::{set_project_root, set_flowlib_root}` are
  process-global and panic on first use if unset, and
  `flow_parsing::docblock_parser::Docblock` is private, so the context metadata
  has to be computed where the docblock is parsed rather than carried around.
- **The port cannot be told which goal symbol it is reading.** ECMAScript has
  two, *Script* and *Module*, and `await` is reserved in only one of them: at
  the top level of a module `await x` is an operator, which is ES2022 and what
  Node runs. The port pins `ParserEnvFlags::allow_await` to `false`,
  `ParseOptions` has no member for it, and `with_allow_await` is `pub(crate)`;
  its own `flow_parser_wasm` says as much where it maps Hermes' `source_type`.

  `uf_flow::module` supplies the missing goal on uf's side of the boundary,
  without a second grammar. `await` and `void ` are both five bytes and both a
  *UnaryExpression*, so the port is asked about the same file with one traded
  for the other — every other byte, line and column unmoved — the tree it hands
  back says which of them it read as an operator at the module's top level, and
  the operator is put back where the author wrote it. Which `await` is an
  operator and which is a property name is never guessed: `{ await() {} }` and
  `await (x)` are the same two tokens, so the parser decides and the ones it
  did not take are offered back un-traded.

  `uf fmt`, `uf lint`, `uf check` and the transform all go through that one
  function, so one file gets one reading. A file with no `import`, `export` or
  `import.meta` is a script — Babel's `sourceType: "unambiguous"` rule, and the
  only rule every caller can evaluate, since `uf fmt` is handed source text
  with no path beside it — and `await` outside an `async` function in one is
  still refused, as it is inside a function that is not `async`.

`flow_flowlib` embeds Flow's library definitions with `include_str!` paths that
reach outside `rust_port` into `lib/`, `prelude/`, and `tslib/`, so
`tools/upstream/sync.sh` checks those out too and asserts they arrived.

The submodule costs one gate. `cargo-semver-checks` builds its baseline from a
copy of each crate, outside the workspace, where the relative path to
`upstream/flow` no longer resolves — and a path dependency is relative by
definition, so no crate can fix it from its own manifest.

Six crates are excluded from that check: `uf_flow`, `uf_check` and
`uf_transform` name the submodule, and `uf_cli`, `uf_fmt` and `uf_lint` reach
it transitively, because cargo resolves a path dependency whether or not the
feature using it is enabled. `uf_fmt` joined them when the formatter started
printing from the parser's syntax tree. That leaves most crates gated, which is worth more
than switching the gate off — but the list grows as more crates use the parser,
and it already includes three of the more interesting public APIs.

If it grows to where the gate covers little, the fix is to make the upstream
dependency a rev-pinned git dependency, which cargo resolves from any directory,
and keep the submodule for reading. That would also undo the other three costs
the submodule has charged: `cargo fmt --all` visiting vendored code, every CI job
needing `tools/upstream/sync.sh` before cargo will parse the workspace at all,
and the `include_str!` paths reaching outside `rust_port`. The trade is that the
built code and the readable checkout stop being the same bytes by construction.

## Other Upstream Sources

`upstream/flow` is a submodule because it is a cargo path dependency: the
workspace does not resolve without it, so `tools/upstream/sync.sh` materializes
it on every checkout and in every CI job.

Three more upstream repositories are pinned by commit in
`tools/upstream/repos.txt` and fetched on request:

```sh
tools/upstream/sync.sh --integrations   # uf run upstream:sync:integrations
```

They are not submodules, and they are not fetched by default, for one reason:
**nothing in the cargo graph depends on them yet**, and `sync.sh` runs in every
CI job. Vendoring them into that path would slow every job to hold source that
nothing compiles.

Each is filtered to the subtrees uf reads, which is about 45 MB of roughly a
gigabyte.

| Pinned source | What it is | What uf does today |
| --- | --- | --- |
| `upstream/react` — `compiler/crates` | The React Compiler's official Rust port: `react_compiler_validation`, `react_compiler_hir`, `react_compiler_inference` and nine more. | `uf_react_compiler` implements the *syntax-mode* checks itself, over Flow's AST. It is honest about being a subset — see its module docs for the three checks it leaves out rather than approximates — but `@uniflowed/react-compiler` already advertises `implementation: "official-rust"`, and this is the source that makes that true. |
| `upstream/relay` — `compiler/crates` | Relay's compiler, in Rust: `graphql-syntax`, `graphql-ir`, `relay-transforms`, and 44 others. | `@uniflowed/relay` re-exports the JavaScript Relay runtime and shells out to the published `relay-compiler`. Artifact generation is the hot path the redundancy guide says must be native. |
| `upstream/react-native` — `packages/react-native-codegen`, `packages/react-native/Libraries` | React Native's codegen and its Flow-typed JavaScript libraries. | `@uniflowed/react-native` is a declaration module. The codegen is what turns a Flow spec into native bindings, and the Libraries are the Flow types a React Native app is written against. |

The pins are the four repositories' `main` as of the commit that added this
section. Bumping one is a line in `repos.txt`; `sync.sh` fetches the new commit
and nothing is left behind.

The "Upstream Integrations" CI job fetches all three and checks the subtrees
are where the manifest says. That is the whole of what it asserts: a pin that
has been force-pushed away, or a directory that upstream moved, fails there
rather than the next time someone tries to build against it.

## Flow To JavaScript

Every host runs JavaScript, and `uf` projects are Flow. `uf_transform` is the
one place that turns one into the other, and it is assembled from the code
that owns each step rather than from anything `uf` invented. There is no Babel
in the pipeline.

| Step | Implementation |
| --- | --- |
| Parse | Meta's Flow Rust port (`flow_parser`), rendered as ESTree by its own translator |
| Lower `component`/`hook`, `match`, enums; erase types | Ports of `hermes-parser`'s `TransformComponentSyntax`, `TransformMatchSyntax`, `TransformEnumSyntax` and `StripFlowTypes` — the rules Flow's own toolchain applies |
| Babel AST + scopes | A port of `hermes-parser`'s `TransformESTreeToBabel`, plus a scope analysis in Babel's terms, because that is the contract the compiler consumes |
| React Compiler | The official Rust implementation (`react_compiler` on crates.io) in `syntax` mode: only `component` and `hook` declarations are memoised |
| JSX, Fast Refresh, code generation, source maps | oxc — the engine inside Vite and Rolldown — so a module is byte-identical whether Vite or `uf test` asked for it |

The output is the JavaScript Flow documents: a `match` becomes the
`typeof x === "object" && x !== null` and `"k" in x` tests Flow specifies, a
`component Foo(a: A, ...rest: R)` becomes `function Foo({ a, ...rest })`, and
an enum becomes a frozen object with the `flow-enums-runtime` contract
(`cast`, `isValid`, `members`, `getName`), with the runtime prepended to the
module so no import is needed.

`uf transform` serves this as a long-lived process: newline-delimited JSON in,
replies in request order out. `@uniflowed/vite`, the Node loader hook and the
Bun preload all speak that protocol, and every one of them applies the same
module policy first (`is_flow_module`): project `.js` files and `@uniflowed/*`
under `node_modules` are uf's to transform, a third-party dependency is not,
and a build driver's own virtual modules never are.

The three hosts ask that question at different moments, and Bun's is the one
that constrains the design. Node's hooks may hand a module back untouched;
Bun's plugin API selects a module by *pattern* and then requires the hook to
answer with contents, and there is no shape that means "not mine" — so the
policy exists twice in `packages/host/transform.js`, as `isFlowModule` and as
`FLOW_MODULE_PATTERN`, and `tests/library/flow-modules.test.js` pins the two
equal path for path. It has to be equality rather than approximation in both
directions: a pattern that under-matched would leave a `@uniflowed` package's
Flow to Bun's parser, and one that over-matched would put a CommonJS
dependency through `onLoad`, where anything that comes out is an ES module and
`import dep from "dep"` stops finding a default export.

The service also unreferences its child between requests, so it holds its host
open for exactly as long as it owes an answer. Node did not need that — its
module hooks run on a loader thread and the process exits with the main
thread — which is the only reason it was never noticed that on Bun the same
service kept the process alive forever after the program had finished.

Source maps point at the Flow source. The printer records a mapping for every
node the author wrote and none for nodes the compiler or the lowering passes
invented, and oxc's map over the printed text is composed with those, so a
debugger lands on the author's line or nowhere — never on the wrong line.

The compiler's panic threshold is `none`: a function it cannot compile is left
as written and reported as a diagnostic on the reply, never a failed build.

## Type Checking

`uf_check` is the embedding of Flow's own inference, behind the
`upstream-typecheck` feature. It is a separate crate rather than a feature on
`uf_flow` for one reason: `uf_lint` depends on `uf_flow`, Cargo resolves path
dependencies whether or not the feature that uses them is on, and inference
needs sixteen path dependencies on the submodule plus a pinned nightly. Keeping
them here means exactly one crate — and, through it, `uf_cli` — carries that
weight.

| Concern | Where it lands |
| --- | --- |
| Toolchain | `nightly-2026-08-01`, pinned by the `Upstream Flow Typecheck` CI job |
| Default build | feature off; `uf check` is the linter alone and `uf` still builds on 1.98.0 |
| Diagnostics | typed: severity, Flow's error code, primary and root spans, message fragments, and every location the message references |
| Bounds | `Options::recursion_limit`, `CheckBudget`, a 4 MiB source cap, and a 1 GiB check stack |

Measured on that toolchain, optimized: builtins merge in **19 ms** cold and cost
nothing warm; a dense Flow React component file checks in **4.3 ms**
(230 files/s, one thread). Unoptimized those are 60 ms and 15 ms. Over this
repository — 319 sources, 302 of them checked — `uf check` is **4.0 s** with a
cold cache and **0.28 s** with a warm one, medians of eight alternating runs.

Modules resolve against the batch, not against a filesystem. A relative
specifier names another source `uf check` collected, and that module's
*signature* — Flow's own annotation-only description of what it exports — is
merged into the importing file exactly as `flow_services_inference` merges one
from its heap. A bare specifier is resolved through the `package.json` files in
the same batch: `@uniflowed/cell` is whatever the manifest publishing that name
says it is, honouring its `exports` map with upstream's own implementation of
it, so a package that moves a file internally does not take its consumers' types
with it. Flow's standard library answers what is left — `react` and everything
else the library definitions declare — and a specifier nothing answers resolves
to Flow's *unchecked module*, which types the import as `any` and lets the rest
of the file check. Those specifiers are reported in
`CheckReport::untyped_modules`, so the hole is stated rather than silent: on this
repository they are the third-party packages and the `node:` builtins that no
manifest here publishes.

Which makes *what is in the batch* the whole question, and it is the caller's to
answer. `uf_check::module_closure` is how: given the files a reader asked about
and every file the scan found, it walks the module graph with the same
`ModuleIndex` and `WorkspacePackages` the checker resolves through, and returns
the files that are reachable together with the specifiers nothing answered.
`uf check <path>` then checks the closure rather than the selection — before it
did, a batch of one file resolved nothing, and every `import type` in it was an
`any`-typed value the annotations were written against and never read
([#403](https://github.com/ubugeeei-prod/uf/issues/403)). Diagnostics are still
reported only for the files that were asked about: a dependency is in the batch
to be typed against, not to be reported on.

A specifier the closure could not answer is looked for under `node_modules`,
which is what makes a project that merely *uses* uf check against uf's real
types rather than against `any` — the package is read through the symlink a
workspace makes, and its `exports` map decides which of its files anything
resolves to. Two conditions bound that, and both are Flow's rather than uf's.
A package Flow's own library definitions describe is never read: a file in the
batch outranks a `declare module`, so reading React's shipped JavaScript would
replace Flow's description of React with a bundle that has no types in it. And
a package that declares no `@flow` anywhere is not read either: it exports `any`
whether it is in the batch or not, so reading it would buy a parse of every byte
it ships and nothing else.

Nothing is checked twice. A run keeps one record per file under
`.uf/cache/check/`, keyed by the identity of the `uf` that wrote it — its path,
size and modification time, the discipline `.uf/cache/transform` already
holds — together with the limits the check ran under and the file's own path and
text. A record also carries a *dependency digest*, over the packed signature of
every module the file reaches and how each of those modules' specifiers
resolved; the diagnostics in it are believed only while that digest still
describes the batch. So editing one file re-checks that file and the files that
reach it, and nothing else — and editing a function body, which moves no
declaration and changes no exported type, re-checks only the file itself. A
process that cannot name its own binary caches nothing in either direction, and
an entry that is unreadable, out of date, or about another file is a miss rather
than an error.

All three disk caches — `.uf/cache/check`, `.uf/cache/transform` and
`.uf/cache/task` — are bounded by one policy in `uf_infra::cache`: 128 MiB per
directory, swept to 96 MiB, coldest entry first, where "coldest" is the later
of a file's access and modification times. Every key names the `uf` that wrote
it, so a rebuild orphans a whole generation at once and none of them could
give a byte back before this existed. The sweep runs once at the start of the
command that is about to add to a cache — `uf check`, `uf run`, and
`uf transform`, which a host starts only when something actually has to be
compiled — and it never rewrites a file, only unlinks one, and never unlinks
one used in the last minute, which is what makes it safe beside the twelve
workers of `uf test`. Evicting by *compiler identity* instead was considered
and rejected: it is the smallest cache and it would retire the guarantee that
a bisect walking back over a rebuild finds its entries still there.

Errors are never flattened into strings. `flow_common_errors`'s accessors give
the code, kind, and primary location directly; the message tree itself is private
to that crate, so `json_output`'s v2 rendering is walked once to recover the
message fragments and the locations they point at, and each fragment is mapped
back onto a typed segment. Two embedding details worth knowing: Flow's renderer
panics on a relative path and reads a location's file from disk to build a
codepoint offset table, so `uf_check` resolves every path against a synthetic
absolute root that cannot exist — the read always misses, and the columns stay in
the **bytes** that `uf_term`'s code frames and `uf_lint` both measure in.

## Formatting

`uf fmt` prints Flow from the official parser's syntax tree. There is no
token-stream formatter left: the previous one rewrote whitespace between
tokens, which cannot decide where a line should break, and misformatted
valid Flow it could not tell apart from JavaScript.

The pipeline is four stages, one module each in `uf_fmt`:

| Stage | Module | What it owns |
| --- | --- | --- |
| Parse | `uf_flow::parse` | the port's `ast::Program`, its comments, and the size and nesting ceilings |
| Index | `flow::text` | byte offsets from the parser's line/column positions, and the questions layout asks of the source: is the next line blank, what is the next character that is not a comment |
| Attach | `flow::comments` | which node each comment belongs to, and whether it prints before it, after it, or inside it |
| Print | `flow::print` → `doc` | a Wadler document, then a width-driven layout pass |
| Embed | `flow::print::embed` → `graphql` | the contents of a template whose tag names GraphQL, printed into the same document |

**The document IR** is Prettier's, ported: `text`, `line`, `softline`,
`hardline`, `literalline`, `group`, `conditionalGroup`, `indent`, `align`,
`ifBreak`, `indentIfBreak`, `fill`, `lineSuffix`, `breakParent` and `trim`.
Node printers never measure a line; they say which pieces belong together
and where a break is allowed, and `doc::printer` decides group by group
whether the flat form fits. Documents live in a `Bump` arena, so building
one never clones a subtree, and every node carries the "contains a hard
break" flag that Prettier computes in a separate `propagateBreaks` pass.

**Comment attachment** is the part a printer usually gets wrong. A comment
is not in the syntax tree, so before printing, each one is given an
enclosing, a preceding and a following node, classified by the newlines
around it — own line, end of line, or code on both sides — and handed to
the same list of special cases Prettier keeps (`if (a /* c */)`, a comment
before `else`, a comment in an empty argument list). What is left falls to
the defaults: own-line comments lead the following node, end-of-line
comments trail the preceding one, and a comment with code on both sides is
a *tie*, broken by the whitespace between it and what follows.

**Four guarantees**, each a test rather than an intention:

- **Prettier-compatible.** The fixtures under `crates/uf_fmt/tests/fixtures`
  pair an input with the output of `prettier --parser hermes
  --plugin prettier-plugin-hermes-parser`, and are compared byte for byte.
  Prettier's defaults, `embeddedLanguageFormatting: "auto"` included — with
  the one qualification named below, which is the embedded languages uf
  does not read.
- **Idempotent.** `format(format(x)) == format(x)`.
- **Tree-preserving.** The output re-parses to the same tree as the input,
  compared as JSON with locations, comments, `raw` spellings and the other
  things a formatter may rewrite normalised away.
- **Comment-preserving.** Every comment appears exactly once in the output.
  The printer marks each comment as it emits it, and a comment left
  unmarked fails the whole run rather than shortening the file.
- **Total.** Invalid syntax is a typed error and the file is left alone;
  no input panics. The parser recurses, so `uf_flow` refuses sources past
  8 MiB, 300 levels of bracket nesting, or 10,000 links of an operator
  chain that nests without brackets *before* parsing, and the work runs on
  a thread with the stack those ceilings were measured against.

Every one of these but the first is also checked over third-party Flow —
React, Metro, Relay, React Native, Recoil, Flux, Parcel, Yarn, Prepack,
StyleX, fbt, react-native-web, react-motion, DataLoader and redux-form,
about 8,100 modules — by `crates/uf_fmt/tests/upstream_corpus.rs`. That
corpus is where most of the printer bugs this repository has fixed came
from: a hand-written fixture is written by somebody who already knows what
the printer does.

Parentheses are not in the port's tree, so every pair in the output is
recomputed from precedence and position. That is what makes the
tree-preserving test meaningful: a pair the grammar needs is always
printed, one it does not is dropped, and `(a?.b)()` keeps the parentheses
that end its optional chain because dropping them would change what the
program does.

Non-Flow files (JSON, JSONC, CSS, TypeScript) go to the formatter named by
`fmt.nonFlow.formatter`, as a subprocess: `uf fmt` collects them, hands them
to that command, and reports what it did. The default is Biome, resolved
from the project's `node_modules/.bin` before `PATH` so a project gets the
version it installed. `"none"` turns it off.

A formatter that is not installed is never a silent skip: it is named, once,
with the files nobody looked at listed under it. Whether it ends the run
depends on who chose it. A project that wrote `fmt.nonFlow.formatter` down
stated a requirement, and an unmet requirement is an error. uf's default is a
suggestion, and a suggestion that is not installed is a warning: the exit code
goes on answering the question `uf fmt --check` is asked in CI, which is
whether the files uf can format are formatted. Until that split existed, the
first `uf fmt` in a project uf had just scaffolded exited 1 over a JSON file
nothing was wrong with, because uf's own default and uf's own scaffold
disagreed — see ubugeeei-prod/uf#441. A project that wants CI to insist on the
non-Flow half says so by naming the formatter.

Not linked in, because linking Biome would make uf's release depend on
Biome's — red line 5. Running the binary a project already has means the
project upgrades its formatter without waiting for uf, and can name one uf
has never heard of.

### Embedded GraphQL

Prettier formats the code *inside* a tagged template when it recognises the
tag — `graphql`, `css`, `html`, `sql`, `markdown` — and re-indents it to the
code around it. **uf reads one of those: GraphQL.** It is the one this
toolchain has a reason to know, and re-indentation is the visible half of
the feature: GraphQL that keeps the indentation the author left it at is
GraphQL at the wrong indentation as soon as anything around it moves. A
`css` or `sql` template goes out byte for byte, as before.

`crates/uf_fmt/src/graphql` is a GraphQL lexer, parser and printer — the
executable grammar and the type-system grammar, no third-party crate — and
its printer is Prettier's `printer-graphql` over the same document IR as the
Flow printer, so the layout is reproducible arm for arm rather than
approximated. `flow::print::embed` decides which templates hold one and
splices the result back in. The rules there are Prettier's, established by
running it:

- **The tags.** `graphql`, `gql`, `graphql.experimental`, a template passed
  to a call of `graphql(…)`, and a template behind a `/* GraphQL */` block
  comment. Exact and case sensitive: `Relay.QL` and `gql.experimental` are
  not GraphQL to Prettier and are not to uf.
- **`${…}`.** Not one document with holes in it: each run of text *between*
  the holes is parsed as a whole document of its own. So a trailing
  `${Fragment}` works, and a hole in the middle of a selection does not —
  and when any run fails, the whole template is left alone.
- **Indentation.** One level in from the line the template starts on, with
  the closing backtick back at it.

**What uf declines, it does not touch.** A template it will not reprint
comes out byte for byte as the author wrote it — the author's indentation
included — rather than approximately right. Four things are declined:

| Declined | Why |
| --- | --- |
| A document holding a `#` comment | Prettier places a GraphQL comment with its generic comment-attachment pass, and uf does not reproduce that pass. A comment on the wrong node is a worse answer than an untouched template. |
| A string value holding a control character | Prettier prints a string value with only `"`, `\` and a newline escaped, so a `\r` goes out raw — and the formatter normalises line endings on the way in, so the next run would read a different string. |
| Anything that is not GraphQL | Including a quasi that is not a document on its own. |
| More than 128 levels of nesting | A template is untrusted input like any other source text. |

Of the 1,138 `graphql` templates in Relay's test suite, uf reproduces
Prettier byte for byte on 1,092 and declines 46, every one of them for a
comment.

Formatting the inside of a template rewrites a string literal's contents,
so the tree-preserving guarantee had to learn what a template's text
*means* rather than compare it byte for byte. For GraphQL that is its token
sequence — commas and whitespace are nothing to the grammar, and a string
value is reprinted from its value rather than from its source spelling — so
a quasi that lexes as GraphQL is compared by
`uf_fmt::graphql::token_signature` and one that does not is compared as
text. A dropped directive, a reordered selection or a changed number is
still a different token sequence and still fails.

## Flow And React

The default app preset is Flow-first React. New app templates use Flow component
syntax, `app.js`, file-system routes, server actions, StyleX,
query/effect APIs, Relay, `@uniflowed/cell`, headless UI, hooks, and React
Native-compatible entry files.

Server Components are the default. Client Components must opt in with
`"use client";`, and server action modules must opt in with `"use server";`.
Caches are off by default. React 19, Suspense, `use`, and Async React are
assumed.

### The cache

`rendering.cache` has four switches and had, for a long time, no cache behind
any of them: all four were read once, copied into `dist/uf-build-manifest.json`
and read by nothing, so setting one to `true` changed one field of one JSON file
and no behaviour anywhere. Two of them mean something now, and two of them are
refused rather than ignored — `rendering.cache.data: true` and
`rendering.cache.actions: true` fail the config load by name, because a key that
accepts `true` and does nothing is indistinguishable from a cache that is off.
See ubugeeei-prod/uf#277.

The store is `packages/server/internal/cache-store.js`, and its header answers
the five questions every cache bug is one of: what a key is, what an entry is,
when an entry is stale, who evicts, and what happens to a request that arrives
while an entry is being filled. In short — a key is a list of strings; an entry
is a value with the two instants that end it, its tags and its path; time makes
an entry *stale* and a tag makes it *expired*, which are deliberately different;
eviction is least-recently-used, bounded by count, with no background sweep; and
a request that arrives during a fill joins it rather than starting a second one.

The key departs from both of the disk caches described above, and the departure
is the reason the store is in memory. `.uf/cache/check` and `.uf/cache/transform`
each put the identity of the `uf` that produced the entry into the key, because
both outlive the process that wrote them. This one cannot, so that identity is a
constant rather than an input — and the day a durable store exists behind
`resolve`, the key gains a build id before anything is written to it.

Nothing is cached without a stated lifetime: a route says `cacheLife` and
`cacheTag` from inside its own render, a request says `cache` at the call, and a
page or a call that says nothing behaves exactly as it did. A rendered document
is refused outright if the render read `cookies()`, `headers()` or `draftMode()`
— counted across the whole document rather than up to the shell, because a
component inside a `<Suspense>` boundary renders long after the shell resolved.
That is a runtime refusal; `uf_rsc` already answers the reachability question
that would make it a build error, and does not answer it for cached scopes yet.

One page whose loader takes 50 ms, served twice (`uf run bench:route-cache`,
Node 24, twenty pairs, medians):

| | first | second | renders per pair | renders for 10 at once |
| --- | --- | --- | --- | --- |
| no cache | 52.35 ms | 52.41 ms | 2 | 10 |
| route cache | 52.45 ms | 0.16 ms | 1 | 1 |

The cold request is fractionally slower because a document has to be whole
before it can be an entry, so a cached route buffers where an uncached one
streams. It is in memory and in one process, which means four server processes
hold four caches that disagree and a restart empties one. `docs/app/guide/cache`
says all of that to a reader rather than to a maintainer.

That analysis is load-bearing at the route level, and only there. `uf_rsc`
resolves the module graph and marks every module a `"use client"` boundary is
reachable from; `uf build` and `uf dev` write the result to
`.uf/rsc/uf-rsc-manifest.json` and name it in `UF_RSC_MANIFEST`, and
`@uniflowed/vite` generates the browser's copy of the route table without the
page of any route that reaches no boundary. No `import()` in that table reaches
the route's page, so Rollup emits no chunk for it and none for anything only it
reached; `hydrate` returns without mounting the route, because the document the
server wrote is the whole of it, and a link into it is a document navigation
rather than a client render.

Below a route, one unit is split: a `"use server"` module. `@uniflowed/vite`
answers the browser's copy of that file with one `createServerReference` per
callable export — an id and a `fetch`, and none of the module's body, its
imports, or anything only they reached. The graph colours such a module server
for the same reason, so the analysis and the bundle agree rather than each
describing the other. What is *not* split is a Server Component above a
boundary: uf's client hydrates by re-rendering the matched tree from the same
modules the server used, so dropping one needs a Flight-shaped payload uf does
not have. See ubugeeei-prod/uf#252.

A call is a `POST` to the page's own URL carrying `uf-action: <id>`, so the
middleware guarding that path runs above it and no path is reserved. Every host
runs it between the guard and the route handlers — `uf dev`, `uf preview`,
`uf start`, a compiled binary, and all four deploy adapters, which share one
`handler.js`. What may cross in either direction is a closed grammar of plain
JSON data, applied by `packages/router/internal/action-wire.js` on both sides
and by Flow at build time; `docs/security.md` has the boundary and what is
deliberately outside it.

One thing does still come back. A uf build links the stylesheets it finds in
the *client* graph, so a route removed from that graph outright loses its rules
— from every page of the site, because the linked sheets are the whole graph's.
Each dropped module is therefore imported for its side effects, with nothing
read from it: the stylesheet survives and the components, helpers and data it
declared are unused exports that do not.

What still ships is everything *above* a boundary. uf's client hydrates by
re-rendering the matched tree from the same modules the server rendered it
from, so a Server Component that renders a Client Component is a module React
needs in the browser in order to reach the boundary at all; dropping it needs a
Flight-shaped payload uf does not have. A route that keeps its page therefore
keeps its whole subtree, and every application whose root layout imports one
client component — this documentation site included — ships exactly what it
shipped before. Server actions are scanned, keyed, typed and manifested, and
none of them is callable. Both remainders are ubugeeei-prod/uf#252.

The linter starts with framework rules that guide teams away from legacy React
function component typing and toward Flow component syntax. React Native support
starts with platform split diagnostics for generic files that branch on
`Platform.OS` or `Platform.select`.

## Runtime Agnostic Direction

`uf_lib` follows the Bun-style shape for builtin modules, but the user project
runtime is deliberately host-agnostic. Native Rust owns toolchain work —
configuration, linting, Flow parsing/type checking, Flow formatting, test
scheduling, package metadata, and builtin binding contracts — while ordinary
JavaScript execution is delegated to a Capability JS Host.

The zero-config host set is Node.js, Deno, and Bun. `uf.config.js` names the
default host and the accepted host set once, and `@uniflowed/rm` detects and
applies that host instead of installing a bespoke runtime. The self-hosted
Hermes-backed `uf` runtime is still documented as a later line, but it is no
longer the default direction for app execution.

User-authored Flow source uses `.js` files with `// @flow`, and so do the
published `@uniflowed/*` packages: there are no `.js.flow` declaration files.
A shipped module owns its own declarations, raises only when a native binding is
actually called, and runs nothing at import time, so `"sideEffects": false` and
per-subpath exports let a bundler drop everything an application never touches.

Implemented native slices already cover:

- zero-config `.js` + `// @flow` project generation without npm scripts
- router discovery and generated `router.js` route types
- `uf build` metadata emission through `.uf/build/meta/uf-build-manifest.json`
- Rust-native `uf dev` HTTP state and health endpoint
- `uf install` workspace discovery, `uf.lock`, store manifest, and
  content-addressed package entries
- `uf use` and `uf self-update` runtime acquisition and activation, through the
  installer `curl -fsSL https://setup.uniflowed.dev | sh` runs and the store it
  unpacks into
- `uf install` package/runtime plan generation into `.uf/install.json`
- `ufx` native execution for known `@uniflowed/*` package entrypoints
- `uf publish` and `uf release` metadata generation for trusted publishing
- source-level native `uf test` execution for the first assertion subset,
  scheduled longest-first across a rayon pool from durations recorded in
  `.uf/test-timings.json`, with `--watch` re-running only the test files an edit
  transitively invalidates
- stdio JSON-RPC `uf lsp` initialize capabilities

Native engines being deepened:

- query cache and mutation scheduler, aimed at replacing TanStack Query for
  Flow React applications
- generator/yield EffectSystem inspired by Redux-Saga but typed for Flow
- explicit fetch clients without global fetch override
- Relay-based GraphQL client primitives
- Valibot-class validator utilities exposed as `@uniflowed/validator` with
  `v.pipe`
- Jotai-class atom state primitives exposed through `@uniflowed/state` and
  `@uniflowed/cell`
- DOM and React Native testing utilities compatible with Testing Library habits
- self-hosted `@uniflowed/test` runner targeting faster-than-Bun execution
- ORM schema/runtime with Flow opaque types at module boundaries
- StyleX compiler/runtime integration
- React Compiler syntax-mode integration
- Relay integration
- headless UI components with preset styles, validator-backed form contracts,
  RSC split metadata, and `renders` type utilities, aimed at replacing shadcn's
  copy-and-edit workflow with typed imports
- MSW-compatible mocks, Playwright-compatible browser automation, story system,
  and VRT baseline planning
- React Compiler-safe motion primitives with reduced-motion defaults
- React hook utilities that preserve render idempotency and cover the practical
  VueUse-style browser/state/async hooks a React app reaches for
- OpenTUI-aligned terminal UI primitives with cell-diff rendering and in-memory
  tests, in Flow rather than native — see `packages/tui/index.js`
- stdlib contracts for OS, net, DNS, path, streams, URL, WebAssembly, glob, TUI,
  cron, S3, SigV4, worker/lambda functions, UUID, and ZIP utilities
- host-provided event loop and IO capability mapping for Node.js, Deno, and Bun
- deferred WinterTC-aligned Flow runtime backed by Hermes
- deploy-anywhere adapters in a Nitro-like model: `node`, `container`, `edge`
  (Cloudflare Workers) and `serverless` (AWS Lambda) are written against one
  `@uniflowed/server/fetch` handler and none has been deployed to a real
  platform; Deno, Bun and static are not written

## Testing

`uf test` is two halves that never overlap. `uf_test` in Rust owns discovery,
ordering, concurrency, bounds, retries, watch invalidation and the report;
`@uniflowed/test`'s worker owns executing the code. Nothing about a test body
is decided in Rust, because Rust cannot run one — an earlier version of this
crate evaluated a "native assertion subset" by reading source text, and a
runner that decides `expect(a).toBe(b)` without evaluating `a` is a runner that
can be wrong about what passed.

A run works out which files declare tests (by reading, so a module is never
imported to find out whether it is a test), orders them longest-expected-first
from durations `.uf/test-timings.json` recorded, and fans them across worker
processes on the project's Capability JS Host. Each worker takes one file at a
time — two files sharing a process share globals, and a suite that passes alone
but fails beside another is the worst failure a runner can produce — imports it
through the host's Flow loader, and streams one JSON line per case back. Each
request is numbered and each line says which request it came from, because "one
file at a time" bounds what a worker *starts* and not what a finished file left
running.

| Concern | Decision |
| --- | --- |
| A test that never settles | Raced against a per-case budget in the worker, *and* a wall-clock deadline in Rust that kills the process, because a wedged event loop would never run its own timer |
| A module that throws while importing | A file result, not a test result: there were no tests to fail, and "0 tests" for a module that could not load would be a lie |
| A worker that dies | The file is named with what went wrong and the run continues on a fresh worker |
| An event from a file that has already finished | Dropped, with a note naming the file it came from. The worker stamps every event with the generation of the request whose asynchronous context produced it, so a `setTimeout` a file left behind is not read as the next file's — and the file it does belong to has already been reported, because the report is streamed |
| A retry | Re-runs the *file* with a filter naming one case, so the retry sees the module state a first run would |
| `.only` | Decided per file after the module body has run, because a file's `.only` can appear after the tests it excludes |
| A failing assertion's position | The matcher's own message, and the line from the stack — which points at the Flow source because the transform emits a source map and the worker runs with it enabled |

### Coverage

`uf test --coverage` instruments nothing. V8 counts execution on its own, Node
writes those counts out when a worker exits (`NODE_V8_COVERAGE`), and `uf_test`
maps them back through the same source map the transform already produces. The
alternative — rewriting the Flow to count itself — would mean the suite tests a
program that is not the one being shipped.

Three properties of the existing design made that the cheap answer. The Node
host is already started with `--enable-source-maps`, which is what makes Node
keep a source-map cache; Node writes that cache into the coverage document
beside the counts, with the generated file's line lengths, so an offset into
JavaScript is addressable without reading the JavaScript back. The loader
already attaches an inline map to every module it transforms. And the worker
already ends by closing its stdin, so there is an exit to flush at — a run that
collects coverage waits for its workers rather than killing them, which is the
one behaviour this feature added to the runner.

The counting rule is the source map invariant above, used as a filter: **a
generated position that maps to nothing the author wrote is not counted at
all.** That is what keeps a `match` lowering's guards, a `component`'s props
preamble, the enum runtime and the React Compiler's memo blocks out of both the
numerator and the denominator — they are not the author's branches, so they are
not branches. A line is counted when a generated position maps back to it and
covered when the innermost V8 range over that position ran; a function is one
per generated function whose body holds a mapped position; a branch is one per
V8 block range whose first mapped position exists.

Every count is keyed by a position in the author's source, never by a generated
offset or a script id, which is what makes merging several workers' documents
addition. Reports are LCOV, Cobertura and a terminal table; thresholds live in
`uf.config.js` and fail the run. It is Node-only: Bun implements no
`NODE_V8_COVERAGE` and Deno has no Flow loader.

### Measured

50 files, 1,000 tests, 2,000 assertions, on an 8-core M-series Mac, best of
five, each tool running its own idiomatic input:

| Runner | Time |
| --- | --- |
| `bun test` | **0.06 s** |
| `uf test` (Node or Bun host, warm transform cache) | 0.20 s |
| `uf test` (cold transform cache) | 0.30 s |
| `vitest run` | 1.96 s |

So `uf test` is about **nine times faster than Vitest** and about **three times
slower than Bun's built-in runner**. The stated product bar is to beat Bun, and
this does not meet it yet. The gap is process start-up and inter-process
messaging: Bun runs everything in one process with a runner written into the
engine, while `uf` spawns a worker per core and each one loads the test API
before it can do anything. `-j 4` is faster than `-j 8` on this suite for the
same reason. The way to close it is a worker pool that survives between runs
and a worker whose imports are pre-bundled, neither of which is done.

## Build And Dev

`uf.config.js` mirrors the Vite style because it replaces the user-authored
`vite.config.ts`. Vite *is* the dev server, the bundler and the plugin system;
`uf` owns the Flow-specific config surface, the generated route and RSC data,
the Rust lint/typecheck/format/test work, and the transform every module goes
through. Users never write `vite.config.*`.

`uf dev` and `uf build` start `@uniflowed/vite`'s driver on the project's
Capability JS Host — Node.js, Bun or Deno, whichever `uf.config.js` names and
the machine has — and keep the terminal: the driver writes one JSON event per
line and `uf` renders them. The driver loads `uf.config.js` (through `uf
transform`, since the config is Flow), builds Vite's inline config from it,
and registers uf's plugins:

- `uf:flow` pipes every Flow module through `uf transform`, adds the React
  Fast Refresh wiring in development, and serves the virtual modules that make
  a directory of pages an application — the route table generated from `app/`,
  the client entry that hydrates it, and the server entry that renders it. In
  development it renders every document request on the server, so `uf dev`
  serves the markup `uf build` writes.
- `uf:mdx` is `@mdx-js/rollup` with GitHub-flavoured markdown, front matter and
  heading ids, so `_uf.page.mdx` works with no configuration.

A build is three passes: the client bundle (with a manifest, so the renderer
knows which script and stylesheet tags to write), the server bundle (kept
under `.uf/build/server/`, never in `dist/`), and every static route
prerendered to `dist/<route>/index.html` — with `generateStaticParams` on a
page enumerating a parameterised route. `uf` then measures `dist/` and
enforces `build.budgets`.

`@uniflowed/router` is the runtime the virtual modules call into: matching
(`[param]`, `[...rest]`, `(group)`, most specific wins), nested layouts,
`loader` data embedded for hydration, `metadata` hoisted into `<head>`,
client-side navigation with `Link` prefetching on intent, and `notFound()`/
`redirect()`. A page or layout exports its component as `default` or as the
named `Page`/`Layout` that `uf new` scaffolds.

Generated projects do not use npm scripts. Tasks are declared in
`uf.config.js` and executed by `uf run` through Vite Task.

Editor integrations live under `editors/` and stay thin. VS Code, Neovim,
Emacs, Vim, Helix, Zed and Cursor all connect to `uf lsp`; the Rust workspace
remains responsible for parsing, linting, formatting, route type generation and
diagnostics. None of the integrations implements a language feature — each one
is a client, and what it may offer is exactly what `initialize` advertises:
diagnostics, `textDocument/formatting`, `textDocument/codeAction` (`quickfix`
and `source.fixAll.uf`) and `textDocument/hover`, and nothing else.

Only VS Code is a package, and Cursor installs that same package because it is
the same extension format. The rest are configuration — a `languages.toml`, a
Lua module, a `uf.el`, a `uf.vim` — small enough to copy, and short because the
protocol does the work. Zed is the exception: it can only take a language server
from a Rust/WASM extension, so `editors/zed` carries the manifest and its README
says what the missing half must do rather than shipping a file nothing can
build. `tests/library/lsp.test.js` drives the real `uf lsp` binary over framed
messages and asserts every capability those READMEs claim, including that the
ones they disclaim are absent.

The constraint every client has to satisfy is the working directory. `uf lsp`
calls `load_config(".")` once, at start-up, so the process's own directory is
the only channel a project's `fmt` options and lint levels travel through, and
starting a server anywhere else silently gives it uf's defaults. The VS Code
extension starts one server per workspace folder that has a `uf.config.js`, with
that folder as `cwd`, and restarts it when the file changes — the server has no
way to be told about a change. `uf lsp --cwd` is not an alternative: `--cwd` is
a global option, so the command line accepts it and `Commands::Lsp` then ignores
it, reading `.` instead of the directory it resolved. That is a bug in the
server rather than in the clients; until it is fixed, `cwd` is the only thing
that works, and every README under `editors/` says so.

Native package output follows a napi-rs-style target model. The generated
TypeScript declaration files are converted into Flow declaration files so the
repository and published library surface remain Flow-first.

`uf publish` writes the local/trusted publishing manifest used to bootstrap the
first release locally. After trusted publishing is configured from the CLI,
`uf release alpha` computes the next `uf@*` tag metadata and GitHub Actions
publishes through OIDC without a long-lived npm token.

`@uniflowed/pm` owns package resolution, `uf.lock`, a content-addressed store,
and script-free install policy. `@uniflowed/rm` reads `uf.config.js`, infers the
required Capability JS Host, applies host adapters, and feeds `uf env doctor`
with runtime checks. Explicit `uf use uf@...` remains available for the
postponed self-hosted runtime line, but zero-config apps do not depend on it.

Runtime manager paths follow the XDG Base Directory layout: config in
`XDG_CONFIG_HOME`, runtime data and versions in `XDG_DATA_HOME`, cache in
`XDG_CACHE_HOME`, durable state in `XDG_STATE_HOME`, runtime sockets under
`XDG_RUNTIME_DIR` when available, and the `uf` shim under the user-local bin
directory. The target POSIX installer is
`curl -fsSL https://setup.uniflowed.dev | sh`; shell targets include sh, bash,
zsh, and ush, and platform targets include Windows, macOS, and Linux.

## Performance Defaults

The default Rust toolbox is centralized in `uf_infra`:

- `Bump` arenas for short-lived parse/lint/format work
- `FxHashMap` and `FxHashSet` for hot maps
- `phf` for static keyword and rule tables
- `memchr` and `simdutf8` for text scanning fast paths
- `SmallVec` for short diagnostic/export vectors
- `CompactString` for small identifiers and module specifiers

## Libraries, Measured

The `@uniflowed/*` packages are Flow, not Rust, and each one names a library it
is meant to replace. A replacement that is slower than what it replaces is a
worse version of it, so the ones that have been measured are recorded here with
the command that produced the number. The ones that have not are not listed —
an unmeasured claim is the thing this section exists to avoid.

### @uniflowed/effect against Effect-TS

Effect-TS 3.22.1, node 25.8.1 on an M-series Mac, each library in its own
process so Effect-TS runs untransformed, build-and-run per iteration, best of
five rounds, 200k iterations (20k for `all`), median of three runs:

| Workload | Effect-TS | `@uniflowed/effect` | |
| --- | --- | --- | --- |
| `map` → `flatMap` chain, `runSync` | 1,303,165/s | **8,496,929/s** | 6.5x |
| four-step generator, `runSync` | 745,988/s | **1,316,405/s** | 1.8x |
| `fail` + `catchAll`, `runSync` | 1,337,061/s | **9,139,358/s** | 6.8x |
| `all` of twenty, `runPromise` | **432,608/s** | 396,232/s | 0.92x |

The benchmark found a bug rather than confirming a design. The first `yield*`
implementation gave every effect its own `[Symbol.iterator]` closure, which
makes each one escape at construction and stops V8 proving the intermediate
effects in a chain are dead: 1.07M/s on the first row, *losing* to Effect-TS.
One shared function with an annotated `this` took the same workload to 8.4M/s
with nothing else changed. Both numbers are in the source beside
`iterateEffect`, because the fast version looks arbitrary without the slow one.

`all` is the row that is slower, and it is slower for a reason worth keeping:
it opens a child fiber per entrant so a failure can interrupt its siblings, and
Effect-TS's unbounded `all` does less bookkeeping per element.

## Testing Strategy

Every crate should keep focused unit tests close to the behavior it owns. CLI
tests should verify that the public interface remains coherent. As engines become
real, CI should prefer GitHub Actions for full verification because it is faster
and closer to the merge gate.

Benchmark coverage should exist for every hot path: config loading, router
discovery, parser diagnostics, lint scanning, formatting, and test discovery.
