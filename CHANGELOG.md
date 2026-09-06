# Changelog

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
