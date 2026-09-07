# Changelog

## uf@0.0.0-alpha.10

_2026-09-07_

Four packages caught up with what people compare them to. `packages/form` reads
a field at its own type instead of at `mixed` — the position its own header took,
that Flow has "no way to say the type at this path", turned out to be stronger
than what the checker refuses. `packages/state` gained six of `jotai/utils` and
an asynchronous storage, and declined seven more with a reason for each.
`packages/effect` gained the plumbing five shapes were missing: a queue, a
scope-tied fork, a runtime built once, a stream that can be written to, and a
schedule that carries a value. `@uniflowed/ui` gained the four built on `Dialog`
and the three that replace something the browser already does.

`rendering.cache` stopped being four booleans that reached a manifest and
changed nothing: the route cache and the fetch cache are real, with time and tag
revalidation, and the two that are not implemented now fail the config load by
name rather than loading cleanly and meaning nothing. And `uf lint --fix` writes
the fixes a catalogue had been holding for the language server alone, with a
safe/unsafe line that is a field on the fix rather than a decision each caller
makes.

### Added

- **server, config**: the cache the four booleans were describing (#469)
- **ui**: the four built on Dialog, and the three that replace the browser (#462)
- **lint, prepare**: `uf lint --fix` writes the fixes the catalogue already knew (#455)
- **state, hooks**: six utility names, an asynchronous storage, one guard kept twice (#461)
- **effect**: the plumbing five shapes were missing (#463)
- **form**: a field read at its own type, and values that arrive later (#460)

### Fixed

- **test**: the stub formatter is run once before it is relied on (#468)

### Internal

- **release**: a build budget a cold cache fits inside (#465)

## uf@0.0.0-alpha.9

_2026-09-07_

> **This version reached npm and not the release page.** Its
> `x86_64-apple-darwin` build was cancelled at the sixty-minute job timeout, so
> no binaries were published and `curl -fsSL https://setup.uniflowed.dev | sh`
> skips it. The seventeen `@uniflowed/*` packages are on the registry at
> `0.0.0-alpha.9`. See #464; the budget is raised in alpha.10.

The release where several things uf claimed became things uf does. `Image` and
`Font` had been the markup and none of the pipeline; they now resize, re-encode,
`srcset`, self-host a font and match its fallback metrics. `uf test --coverage` measures the author's Flow lines rather than the JavaScript
they compile to. And `@uniflowed/ui` grew a positioning engine and the three
overlay components that need one.

Two of the fixes are worth reading as a pair, because both were a value decided
once and then wrong for everyone after. `packages/router` latched "is there a
document?" at module scope, so a process that loaded the router before a DOM
existed got a router whose navigation silently did nothing — which is what made
one test fail eight times in twenty and pass alone. `uf_term` assumed every
terminal was 72 columns, so `uf install`'s ladder wrapped on a narrower one and
walked down the screen.

### Added

- **test**: coverage in the author's Flow lines, with thresholds and a reporter CI can read (#456)
- **assets, web, vite**: Image and Font get the pipeline they were markup for (#454)
- **tui, term**: a scroll box, a real terminal size, and React Ink measured (#449)
- **ui**: an overlay that flips and slides, and the three components on it (#421)
- **cli, server, vite**: three more deploy adapters through the one handler (#447)
- **config, cli, vite**: `.env` files are read, and only the prefixed half reaches the browser (#423)

### Fixed

- **lint**: the facts that decide which rules run are read from code (#450)
- **router**: decide there is a document by the navigation, not by the import (#453)
- **install**: the banner fits the terminal it is printed to (#452)
- **fmt**: a formatter uf chose and nobody installed is a warning, not an error (#446)
- **flow, fmt, check, transform**: a module may await at its top level (#204) (#434)
- **release**: the release verification parses under the shell that runs it (#444)
- **test**: the packaging check brings its own npm cache (#429)

### Documentation

- sharpen Modern Flow React tagline (#448)
- a README for somebody who has never heard of uf (#442)

## uf@0.0.0-alpha.8

_2026-09-07_

The release that made the published tool work. `uf test` could not run in any
project installed from npm — `@uniflowed/host` exported a file it did not
publish — and `uf create` pinned `latest`, which on npm still pointed six
releases back. Both are fixed, and both now have a test that fails before the
next one can happen: every published package is checked against what npm would
actually pack, and the release preflight reports where each dist-tag points.

`uf check` also stopped re-doing its work. A second run over an unchanged
project is answered from a cache keyed on the compiler, the limits, the
transitive signatures and how each specifier resolved — measured on this
repository at 5.26s to 0.54s, reporting byte-identical diagnostics.

And the Server Components analysis became load-bearing for the first time: a
route no `"use client"` boundary reaches keeps its page out of the browser
bundle. On a four-route application with no client boundary anywhere that is
2,886 raw bytes and five chunks fewer.

### Added

- **rsc**: keep a route with no client boundary out of the client bundle (#438)
- **pm**: the four package-manager commands the operation table already named (#422)
- **test, host**: `uft.mock` replaces a module, and seven bindings stop throwing (#415)
- **install**: the install says what it is doing while it does it (#399)

### Fixed

- **create, release**: pin the scaffold to the uf that wrote it, and give `latest` a way to move (#416)
- **test**: the packaging check reads code, not the prose beside it (#414)
- **host**: the file `@uniflowed/host` exports is one it publishes (#410)
- **test**: the four `any` casts that make `main` red (#412)
- **lint**: uf lints its own repository, and CI runs it (#404)
- **router, build, exec**: three commands that did nothing and said nothing about it (#353)
- **lib**: the registry names exactly what each package exports, and a test says so (#397)
- **build, dev, rsc**: the server-component analysis says what it found, everywhere it runs (#394)
- **release**: a release that went out is not a release that failed (#395)

### Performance

- **check**: `uf check` keeps what it already worked out (#407)

### Documentation

- **site**: the heading says what uf is for (#413), sharpened to "The best React
  experience, typed by Flow." ([#440](https://github.com/ubugeeei-prod/uf/pull/440))
- six claims the site made that the source does not (#384)

## uf@0.0.0-alpha.7

_2026-09-06_

The release that made the toolchain servable: a build can now be previewed,
started, and compiled into one file that serves itself — and `uf check` types
across a package boundary, which is the first hop of everything else.

### Added

- **cli**: a build can be served, by `uf preview` and by `uf start` — Vite's
  own preview server with uf's handler behind it, and uf's own `node:http`
  server that imports Vite nowhere (#343)
- **build**: `uf build --compile` writes one file that serves the application —
  prerendered documents, hashed assets, route handlers and a fresh render, with
  Bun's runtime embedded. Producing one needs Bun; running one needs nothing
  (#356)
- **tui**: `@uniflowed/tui` renders, and sends only the cells that changed — a
  React renderer whose host is a terminal, following OpenTUI. 1,920 cells for
  the first frame at 80×24, and one cell in seven bytes for a keystroke (#334)
- **check**: Flow declarations for Vite's client API, so `import.meta.glob`,
  `import.meta.hot` and `import.meta.env` are typed rather than errors (#355)
- **check**: Flow declarations for the files Vite resolves that are not
  JavaScript — a stylesheet, a CSS module's class map, `?raw`, `?url`,
  `?worker` (#379)
- **effect**: a fiber owns what it forks, plus layers, schedules, sharing and
  streams. `fork` was `forkDaemon`; a fiber now owns its children in both
  directions (#332)
- **ui**: the three roving-focus sets, the disclosure pattern, and arrows that
  read right to left (#333)
- **ui**: select, table, slider, toast — the nine components a data-heavy page
  needs, and the reason a native `<select>` is better than the one shipped
  (#362)

### Fixed

- **check**: a package resolves through the manifest that publishes it —
  `value-as-type` 145 → 14, and 489 errors the `any` was covering are now
  visible (#352)
- **create**: the first command a reader types is one word. `uf create app
  my-site` was rejected; a lone argument is the template when it names one and
  the path when it does not (#369)
- **router**: the nearest `_uf.not-found.js` answers, not the one at the root,
  and a page that throws is one route's problem (#354)
- **vite**: the stylesheet reaches `dist/`, and a React Compiler finding names
  its file. `TransformService` dropped the `css` field, so uf's default styling
  system emitted class names and no CSS at all (#380)
- **vite**: a build that works and complains is a build people stop reading —
  the deprecated `envFile` spelling is gone (#363)
- **fmt**: the same file, formatted twice, is the same file (#361)
- **state**: an abandoned load stops, and a persisted atom is read on mount so
  a server render hydrates — the read moved out of module evaluation, where it
  raced the first client render (#344)
- **form**: the validation rules reach the element, and a form can be switched
  off. A disabled field behaves like an absent one, including through a
  resolver (#344)
- **react-testing**: the queries stop lying to the test — `getByRole` no longer
  returns hidden elements, `level` and `current` are honoured, and an option
  the query does not take is refused rather than ignored (#381)
- **cli**: `main` does not compile, because two pull requests were right apart
  (#366)
- **plugin**: a scroll button is not a bundler — `ScrollUpButton` contains
  `rollup` (#364)

### Documentation

- what uf is for, and the four places it is not that yet (#251)
- the four pages a stranger reads before they try uf (#311)
- a tutorial that was actually run, and a guide for every library package
  (#331)

### Internal

- **ci**: one check that is true only when every other one is. A failing
  `Toolchain` reported every required check as skipped, and GitHub merges on
  skipped (#368)
- three tests that were untrustworthy are deterministic (#382)

## uf@0.0.0-alpha.6

_2026-09-06_

### Added

- **dev**: `uf dev --port` binds that port or says why it cannot (#239)

### Fixed

- **release**: the npm check waits for the registry to catch up (#243)

### Documentation

- uf@0.0.0-alpha.5 contains twenty-one pull requests, not twelve (#244)

### Internal

- **check**: keep the reproduction for a match over a generic in the repository (#226)

## uf@0.0.0-alpha.5

_2026-09-06_

### Added

- **lsp**: code actions and hover, and the two bugs that finished them (#209)
- **story**: the story runner, and four wrong answers the tests found (#211)
- **prepare**: the command does the five things it lists
- **flow**: say what is wrong with top-level await, and point at it (#222)
- **fmt**: the GraphQL inside a tagged template is formatted and re-indented —
  1,402 of Relay's 1,463 templates byte-identical to Prettier, and the 61 that
  are not are declined rather than approximated (#238)

### Fixed

- **ui**: a part's rest props no longer make React's `key` mixed — 32 of
  `@uniflowed/ui`'s 73 type errors, one per element it renders (#220)
- **test**: a line a finished test's callback printed is still that test's (#221)
- **test**: an event that outlives its file is not the next file's (#228)
- **host**: the transform cache key says which `uf` compiled the entry, so a
  rebuilt compiler stops serving what the previous one produced (#219)
- **vite**: the compiled config is written where a second command can read it,
  so `uf dev` and `uf build` in one project stop corrupting each other (#241)
- **flow**: a parse tree is freed where there is room, not where it is held (#230)
- **lint**: the ceilings that protect `uf fmt` protect `uf lint` too — a
  generated file no longer aborts the process (#232)
- **lint**: a rule about what code does no longer reads strings (#227)
- **lint**: `fetch/no-global-override` is about the assignment, not the name (#236)
- **react**: a `useX` name is a hook only where the module says React (#237)
- **router**: `_uf.not-found` is a reserved name, and the linter says so (#224)
- **fmt**: the container that breaks a JSX arrow body is the one above the
  call (#233)
- **types**: a shipped package says what it knows — 45 of the 58 `any`s in
  published packages are real types now, and `uf check` gained none (#242)

### Internal

- one set of parse options, and the test that keeps it one (#216)
- a package somebody implemented has to be on its way to npm (#229)
- check that `package-lock.json` still describes the manifests, and the two
  mistakes the version bump made on its second run
- match the changelog heading literally, and run in `uf run ci` what the
  pipeline runs — the two had drifted eight tasks apart
- the dev-server test says what the server did, and tries again (#235)
- say which Prettier `uf fmt` is compatible with (#223)

## uf@0.0.0-alpha.4

_2026-09-05_

### Added

- uf check reads across files, and four libraries stop being contracts (#201)

### Fixed

- **test**: the test runner brings the loader it is started with (#168)

### Internal

- one cycle for the night's work (#198)

## uf@0.0.0-alpha.3

_2026-09-05_

### Added

- **explain**: say who does the work for every command that delegates (#167)
- **web**: the primitives a page is built from, and three rules that were wrong (#119)
- **test**: a clock a test controls, and the namespace `uft` (#115)
- **cli**: add Flow API docs command (#121)
- **test**: snapshots, and the discipline that makes them a test (#118)
- **test**: the matchers that stand in for a value instead of being one (#114)
- **relay**: Relay, re-exported by name — and the environment nobody should rewrite (#110)
- **server**: the request a server function is inside (#109)
- **stylex**: the compiler was written and nothing ever called it (#108)
- **check**: the platform globals, and a mark drawn at its own proportions (#107)
- **cli**: completion that knows this project, and errors that name the fix (#101)

### Fixed

- **fmt**: an object's spread keeps its parentheses (#160)
- **project**: a file that cannot be read does not stop the project (#165)
- **fmt**: a comment type declaration stays a comment (#154)
- **fmt**: a comment after a class's type parameters ends its line too (#156)
- **fmt**: a comment type stays a comment (#148)
- **fmt**: refuse a chain that nests without brackets (#149)
- **fmt**: a comment before `extends` stays where it was written (#144)
- **fmt**: a right-nested logical chain lays out like a left-nested one (#139)
- **packages**: declare the validator @uniflowed/ui imports (#150)
- **fmt**: an inferred predicate keeps the colon it stands after (#140)
- **fmt**: JSX that runs out at end of input is refused (#132)
- **vite**: decide a Flow keyword by the line, not by the token (#123)
- **pm**: a submodule is not one of this project's packages (#124)
- **fmt**: a contextual keyword at the start of a statement keeps its parens (#122)
- **core**: the root entry point re-exported two files that were never written (#95)

### Performance

- **fmt**: print each call argument once (#145)

### Documentation

- say that formatter fixtures are data (#158)
- everything after one bootstrap line is a `uf` command (#116)
- what closing each red line would take, and the failure none of them prevent (#102)

### Internal

- reformat the packages the spread-parentheses fix changed (#182)
- **release**: read back what actually reached npm (#147)
- **upstream**: pin React, Relay and React Native beside Flow (#141)
- **fmt**: eleven more Flow codebases in the corpus (#138)
- **fmt**: keep the reproductions for three filed bugs in the repository (#129)
- **fmt**: the formatter's guarantees, over Flow nobody here wrote (#127)
- **release**: see a half-sent release before tagging, not during (#117)
- **publish**: drop a preflight that could never pass (#94)

### Other

- rename (#131)
- Add ubugeeei Redundancy Guide for uf project
- Four fixes: a replaceable formatter, three wrong lint rules, star re-exports, and a bundler `uf test` never loaded (#112)
