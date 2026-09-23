# Changelog

## uf@0.0.0-alpha.47

- fix(router): read a missing chunk off a resolution in one helper, back under the lint budget (#1343) (72187f8b)
- fix(server): spell the adapter contract's fields `readonly` (#1342) (8e30e4f6)
- ci(bench): commit the first CI baseline and fix the gate and the comparison on a runner (#1339) (2c45e91c)
- ci(deploy-parity): run the version-skew fixture where a socket can be bound (#1340) (8010f8d1)
- feat: prerender a static shell and stream its holes per request (PPR) (#1337) (32acd930)
- feat(lsp): answer hover, definitions and completion from Flow inference (#1334) (a1af6df5)
- fix(bench): reach client components in the HMR stage, and commit the first CI baseline (#1338) (6ac309da)
- feat(deploy): version-skew protection, the adapter contract, and a deploy-parity workflow (#1335) (bcbd3663)
- feat(images): optimise remote images at request time behind an allow-list (#1336) (e2ac2bac)
- feat(bench): compare uf with Vite+, Next.js, Bun and the rest, gate it nightly, and publish it (#1333) (275f7425)
- feat(tui): add Select, TabSelect, Textarea, flex wrapping and absolute positioning (#1332) (ba665dbc)
- ci(release): check that every npm version has a GitHub release with binaries (#1331) (a65b16a3)
- fix(editors): publishable VS Code versions, a tested Zed extension and an LSP4IJ template (#1330) (8a5187fd)

## uf@0.0.0-alpha.46

- ci: validate releases through maintainer pull requests (#1326) (18c0b1d4)
- docs: simplify the README and add clear entry points (#1325) (18332c9f)
- feat(scaffold): demonstrate task dependencies with clearer guides (#1324) (62a34ae2)

## uf@0.0.0-alpha.45

_2026-09-22_

### Fixed

- **installer**: decode Windows version responses served as either text or binary content, completing the native Windows release introduced in alpha.44 (#1323).
- **cli**: reserve enough main-stack space for unoptimized Windows CLI argument dispatch; verify all three installed launchers exit successfully.

## uf@0.0.0-alpha.44

_2026-09-22_

### Added

- **ui**: styled registry components with examples and accessibility checks for every new UI family, available through `uf ui add` (#1315).
- **ui**: locale-aware ListBox, GridList, Tree and TagGroup; NumberField; segmented date/time fields and range pickers; ColorPicker; keyboard/native drag-and-drop; shared locale and RTL support (#1315, #1322).
- **platform**: native Windows release artifacts, immutable versioned installs, stable command launchers and self-update verification (#1315).
- **test**: `uf test --host node|bun|deno`, JSON discovery with `--list --json`, and required Deno library coverage with original reports and named runtime exceptions (#1315).
- **editors**: reproducible VSIX packaging and documented Zed/JetBrains setup. Marketplace and Open VSX publication still require repository credentials (#1315).

### Fixed

- **host**: prefer an installed React compiler runtime, guard platform detection in browser bundles and normalize Windows module paths without changing POSIX paths (#1315).
- **ui**: expose every public component part and keep the runtime registry synchronized with the shipped modules (#1315).

### Performance

- **test**: run the transform cache's two loaders in two files (#1320).

### Internal

- **deps**: update GitHub Actions dependencies (#1321).

## uf@0.0.0-alpha.43

_2026-09-21_

### Added

- **examples**: show native clips one to a screen, like short video on a phone (#1308)
- **examples**: write the native Commonplace with Async React, match and renders (#1307)
- **examples**: bring the native Commonplace to the web examples' screens and design (#1305)

### Fixed

- **test**: end a run whose node is a forking shim, and halve the suite (#1317)
- **examples**: keep clip captions legible and show times in the device's zone (#1306)
- **router**: write the Flight payload only among the children of React's root (#1304)

### Performance

- **check**: walk the module graph once per command, not once per round (#1318)
- reuse test discovery plans (#1316)

### Internal

- **examples**: preload per island in the Relay SNS instead of one client screen (#1303)

## uf@0.0.0-alpha.42

_2026-09-20_

### Added

- **server**: add `app/$instrumentation.js` startup and request-error hooks, a browser counterpart, and OpenTelemetry spans for requests, middleware, routes, loaders, rendering, server actions and fetches. Streaming work preserves request context and W3C trace propagation; the application chooses its SDK and exporter (#1302).

### Fixed

- **examples**: align native dependencies with Expo SDK and declare its Babel preset (#1299, #1300).

### Internal

- **examples**: improve spacing across REST, GraphQL and native examples without changing their design or behavior (#1301).
- **release**: include `@uniflowed/react-native-testing` in the 23-package OIDC publishing closure after configuring its GitHub trusted publisher with `npm trust` (#1301).

## uf@0.0.0-alpha.41

_2026-09-19_

### Added

- **examples**: add a native SNS app with typed StyleX and colocate route modules (#1297)
- **mcp**: expose live development errors and request logs (#1295)
- **cli**: migrate projects and versioned configuration changes (#1294)
- **native**: integrate StyleX, navigation and component testing (#1293)
- **test**: support RSC integration tests and Chromium visual assertions (#1291)
- **server**: cache public functions and authenticate native clients (#1290)
- integrate Relay RSC and add a GraphQL SNS example (#1289)
- complete native app links and Expo development workflows (#1287)
- complete native builds and routing workflows (#1286)
- **lint**: add import/no-unused-modules (#1274)
- **lint**: add import/no-deprecated (#1273)
- **lint**: add import/no-named-as-default (#1272)
- **lint**: add local import rules (#1242)
- **config**: add test target key (#1233)
- **lint**: report `this` in a component and a prop nothing reads (#1223)
- **lint**: a stray character in JSX text, and two controls missing a prop (#1214)

### Fixed

- **dts**: preserve tuple conditional false branches for scalar inputs (#1296)
- **dts**: infer nested indexed conditionals (#1284)
- **router**: intercept RSC flight navigations (#1282)
- **dts**: avoid date-fns declaration findings (#1279)
- **check**: make Intl relative time options inheritable (#1278)
- **dts**: avoid method access in ReplaceReturnType (#1277)
- **dts**: loosen OmitKeyof key constraints (#1276)
- **dts**: preserve OmitKeyof method properties (#1275)
- **dts**: print implemented function properties as fields (#1244)
- **build**: compile with the declared runtime (#1243)
- **fmt**: reuse the formatting worker stack (#1237)
- **examples**: set explicit button types (#1232)
- **transform**: apply React Compiler binding renames (#1209)
- **lint**: resolve React JSX APIs by binding (#1212)

### Documentation

- **test**: clarify workerd is not a test host (#1283)
- **migrate**: record vite migration check (#1281)
- **migrate**: confirm vite proxy placement (#1249)
- **migrate**: confirm async pages use loaders (#1248)
- **test**: explain non-node coverage limits (#1245)

### Internal

- **lint**: restore React Compiler rule levels (#1241)
- **docs**: fail on hydration errors (#1240)
- **deps**: pin fuzz dependabot to fuzz deps (#1239)
- **vite**: guard installed package prebundles (#1236)
- **ui**: document sidebar forwarded refs (#1230)
- **deps**: migrate oxc crates to 0.149 (#1235)
- **deps**: scope fuzz dependabot updates (#1234)
- **ui**: document remaining small ref effects (#1231)
- **ui**: name hover intent refs (#1229)
- **ui**: document date picker refs (#1228)
- **ui**: document select refs (#1227)
- **ui**: document combobox refs (#1226)
- **ui**: document popover refs (#1225)
- **ui**: document menu tree refs (#1224)
- **ui**: document effect state timing (#1222)
- **ui**: document calendar and toast refs (#1221)
- **ui**: document drawer gesture refs (#1220)
- **ui**: document slider gesture refs (#1219)
- **ui**: document resizable and scroll refs (#1218)
- **ui**: document overlay callback refs (#1217)
- **ui**: document more commit-phase refs (#1215)
- **ui**: document commit-phase ref callbacks (#1213)
- **pm**: cover dedupe check duplicate rows (#1211)

## uf@0.0.0-alpha.40

_2026-09-17_

Two things in this release change what uf can be trusted with. A uf library now
publishes TypeScript declarations, so the people who install a Flow library —
most of whom write TypeScript — stop seeing `any` (#1200). The translation
refuses rather than widens: twenty Flow constructs with no TypeScript meaning
are named in the build report and emitted as `unknown`, and the refused name
survives, so a consumer's build stops instead of quietly accepting something the
compiler never checked. An `opaque type` is published as a branded type, because
nominality is what `opaque` means from outside the module. And `uf build
--adapter bun` now declares the Bun a deployment needs (#1181): React's own
published server build carries a construct Bun could not parse before 1.3.14, so
a deployment onto an older Bun died at start-up with a syntax error and nothing
said why. CI now runs the adapter test on the declared minimum as well as the
newest, because a floor no job exercises is a floor nobody has checked.

`uf lint` answers the whole of `eslint-plugin-jsx-a11y` — 32 rules of 32 — with
the last six reading one attribute each (#1188), four weighing a role against
the element it was put on (#1178), and seven about focus and the keyboard
(#1176). Three `security/*` rules join them (#1197): a `javascript:` URL in a
prop, a `target="_blank"` without `noreferrer`, and an `iframe` with no
`sandbox`. Four more read a name or a value that is not a thing (#1203) — a DOM
attribute React does not bind, a namespaced element name, a `style` prop that is
not an object, and an unusable `rel` keyword. Both sets stop short of guessing:
uf keeps no table of every DOM property or `rel` keyword, because those sets grow
and a stale table starts reporting markup that has become correct. Alongside those, five overlays stopped closing on the touch that
starts a page scroll (#1193) — a `pointerdown` alone is no longer read as a
press that landed outside, so scrolling with a menu open leaves it open. This
release also carries a batch of accessibility and test-library fixes that landed
together (#1180), the largest being that `@uniflowed/react-testing` now reads
implicit roles from the same generated ARIA table `uf lint` uses, instead of a
second hand-maintained copy that had already drifted.

Two corrections to what uf said about itself. `uf build` now reports a
prerendered document served under a header rule that mints a per-request nonce
(#1179): the document carries no nonce at all, so its own client entry can never
be admitted and the page never hydrates. And `uf test` was re-measured on one
machine over twenty-five interleaved rounds: it is about 2.2 times slower than
`bun test` and about ten times faster than Vitest, where the guide had claimed
three and nine. The remaining difference is the fixed cost of starting a
JavaScript host — Node needs roughly fifteen milliseconds before uf runs a line,
and Bun pays none of it because its runner is in the engine — so no optimisation
was written for it.

### Added

- **build**: TypeScript declarations for a Flow library (#1200)
- **lint**: four rules about a name or a value that is not a thing (#1203)
- **lint**: three security rules about a dangerous attribute value (#1197)
- **lint**: six a11y rules that read one attribute (#1188)
- **lint**: four a11y rules that weigh a role against its element (#1178)
- **build**: say when a prerendered route is served under a {uf.nonce} policy (#1179)
- **lint**: seven a11y rules about focus and the keyboard (#1176)

### Fixed

- **ui**: six accessibility fixes — menubar menus take their name from the trigger, calendar weekday headings are no longer empty to axe, a disabled pagination control is no longer an `<a>` with no `href`, a `multiple` toggle group drops `aria-orientation`, the table sort announcement is announced rather than shown beside the table, and a narrow-screen sidebar panel and collapsed tooltip take their styles (#1180)
- **react-testing**: implicit roles are read from the generated ARIA table rather than a second hand-maintained map, a name built from content ignores `aria-hidden` descendants, and `screen` queries and `userEvent` agree on element types (#1180)
- **test**: `uft.useFakeTimers` and `uft.useRealTimers` no longer read as hooks to `uf check` (#1180)
- **test**: a readiness guard now fails instead of returning, so a test cannot pass having asserted nothing when its host is absent (#1204)
- **vite**: a new project's first build no longer warns `INEFFECTIVE_DYNAMIC_IMPORT` about the router's internals (#1180)
- **ui**: an overlay is dismissed by the gesture that ends outside it (#1193)
- **deploy**: declare the Bun a `--adapter bun` deployment needs (#1181), and narrow that floor to 1.3.14 once the first version that parses the artefact was measured rather than assumed (#1206)
- **check**: Flow's library definitions gain Intl's option types, and a `.d.ts` that re-exports another package keeps it under pnpm's layout (#1206)

### Documentation

- **assets**: the codecs and font work uf does not do are recorded as refusals rather than gaps — AVIF, lossy WebP, WOFF2 for a subsetted face, and text subsetting (#1199)
- **testing**: re-measure against Bun and Vitest, and ship the suite behind it (#1198)
- **release**: hand-installed `@uniflowed/*` packages are documented with the `@alpha` tag, held there by a test that rejects untagged install examples (#1180)

## uf@0.0.0-alpha.39

_2026-09-16_

This is the release alpha.38 was supposed to be. alpha.38 was tagged and its
GitHub release built, but the publish job failed before it reached npm, so not
one of its thirteen changes ever reached a user; alpha.39 carries all of them
plus everything since. The headline is unchanged because it has still never
shipped: uf no longer carries a React Compiler of its own. The hand-written
validator is deleted, and every React rule `uf lint` reports is now a diagnostic
from the official `react_compiler` crate, with each remaining category exposed
as a rule at its preset's level (#1147, #1158). Read the breaking note in
alpha.38's section below before upgrading: a project that set
`react/no-derived-state-effect` to `off` was silencing uf's own check, and the
rule that replaces it, `react-compiler/set-state-in-effect`, does not inherit
that setting — it reports at `error` until you say otherwise. Two changes
underneath make the compiler path honest rather than merely present: #1166 gives
every compile in uf a single entry, which fixed `react/no-redundant-memo`
compiling without the `@uniflowed/react` provenance the build uses, and #1170
repairs the publish gate that swallowed alpha.38 — the registry setup now runs
after the test suite, so a token scoped to the publish step can no longer be
mistaken for coverage. And how faithfully uf feeds that compiler is now measured
rather than asserted: with #1172, 1,731 of the official compiler's 1,809
fixtures reproduce its expected output, up from 1,420, with no fixture
regressing. Alongside that, `uf lint` gains an ARIA table and seven rules that
read it (#1159) — the table is generated from `aria-query` 5.3.2, the same
encoding of WAI-ARIA 1.2 that `eslint-plugin-jsx-a11y` reads, rather than
transcribed by hand. Finally, uf now mints a fresh CSP nonce for every request
and puts it on every inline script it writes, exposed as `nonce()` and as
`{uf.nonce}` inside `app.router.headers` so the header and the markup agree by
construction (#1168); a project that never asks for one gets byte-identical
documents. One caveat is worth reading before you turn a strict policy on: a
*prerendered* route is served from a file while the header mints a new nonce per
request, so uf's own client entry is refused and the page never hydrates —
server-rendered routes on the same deployment are unaffected. That gap is #1171.

### Added

- **server**: a per-request CSP nonce on every inline script uf writes (#1168)
- **lint**: the ARIA table, and seven rules that read it (#1159)
- **lint**: jsx-key, no-array-index-key and four more JSX rules from eslint-plugin-react (#1151)

### Fixed

- **transform**: give the compiler the positions and the name Babel gives it (#1172)
- **ci**: run the publish gate's suite before the registry is configured (#1170)

### Internal

- **transform**: one entry for every React Compiler run (#1166)

## uf@0.0.0-alpha.38

_2026-09-16_

Thirteen changes, and the largest is what uf stopped doing. `uf lint`'s React
rules used to be decided by a validator uf wrote itself — some 4,700 lines that
answered, in its own way, the questions the React Compiler answers. It is gone
(#1147), and the rules are now the official compiler's own diagnostics, run with
the options `eslint-plugin-react-hooks` uses and skipping only what that plugin
skips. #1158 finishes the job: every remaining diagnostic category is a rule at
the preset's level, and `react/no-derived-state-effect` is a deprecated alias of
the compiler's `react-compiler/no-deriving-state-in-effects` rather than uf's own
check. **Breaking:** a project that set `react/no-derived-state-effect` to `off`
was silencing uf's rule, and the compiler reports state written in an effect
through `react-compiler/set-state-in-effect` at `error`, which that setting does
not reach — set that rule to `off` to keep the old silence, or fix what it says.
Three compiler rules also stand at `warn` in uf's own repository until its
packages are fixed; the shipped levels are unchanged.

Saying uf runs the official compiler is only worth as much as the input it hands
it. #1149 added a conformance run over the compiler's own 1,809 fixtures,
compared against what `babel-plugin-react-compiler` produced for each, held by a
baseline that fails when any fixture's result changes; #1149, #1154 and #1155
then fixed three ways uf's Babel-shaped AST differed from Babel's — a declaring
identifier that was never recorded as a reference to its binding, optional chains
nested inside another chain arriving as a different program, and the brackets the
printer wrote around them. 1,420 of 1,809 fixtures now match, up from 1,291.

Elsewhere: `uf unlink` undoes `uf link` on every manager (#1156), `uf dedupe
--check` says what a dedupe would collapse without changing anything (#1160), and
`uf link` says what it linked and fails when nothing was — it had been reporting
a package as unregistered because npm redacts a UUID in its own output (#1146).
The router keeps what a navigation fetched for `app.rendering.staleTime`, so a
revisit costs 4 ms instead of 302 (#1135). Eight accessibility rules arrived for
the names of links, headings, frames and media (#1142), Vite's dependency cache
now stays inside the project it belongs to (#1145), and CI runs uf against real
pnpm, Yarn 1, Yarn 4 and bun (#1143).

### Breaking

- **lint**: the rest of the React Compiler's categories as rules, and react/no-derived-state-effect onto the compiler (#1158)
- **lint**: report the official React Compiler's diagnostics, and delete uf's validator (#1147)

### Added

- **pm**: uf dedupe --check says what a dedupe would collapse (#1160)
- **pm**: uf unlink undoes uf link on every manager (#1156)
- **lint**: a11y rules for the names of links, headings, frames and media (#1142)
- **router**: keep what a navigation fetched for app.rendering.staleTime (#1135)

### Fixed

- **transform**: keep the links of one optional chain out of brackets (#1155)
- **transform**: convert the optional chains nested inside another chain (#1154)
- **transform**: give the React Compiler each declaring identifier's binding, and check uf against its fixtures (#1149)
- **pm**: uf link says what it linked, and fails when nothing was (#1146)
- **vite**: keep Vite's dependency cache inside the project (#1145)

### Internal

- **pm**: run uf against real pnpm, Yarn 1, Yarn 4 and bun (#1143)
- **release**: name #1136 in uf@0.0.0-alpha.37 (#1139)

## uf@0.0.0-alpha.37

_2026-09-15_

Eleven changes. `uf@0.0.0-alpha.36` never reached npm. Its publish job installed
Deno 1.x, and every Deno test there failed before a package was sent. This
release installs Deno 2 in that job, and CI now refuses workflows that install
different Deno versions (#1133). That makes this the first release to publish
`@uniflowed/ui`, `@uniflowed/state`, `@uniflowed/cell` and `@uniflowed/temporal`
from the workflow. **Breaking:** `@uniflowed/ui` is imported through its barrel
alone (`import { Switch } from "@uniflowed/ui"`), and its per-component subpaths
are gone (#1130). A project that copied components with `uf ui add` before this
release has to replace those imports, or re-run `uf ui add --overwrite`.
Bundles do not grow: a barrel import is rewritten at build time to the module
that defines each name, so a one-component page builds to the same bytes as
before (#1127). `uf dev` no longer loads a second copy of a component in a
project that installs the packages, which had split its React context. It also
hydrates Server Components in a project that installs `@uniflowed/router`
(#1134). `uf self-update` gains `--check`, `--rollback` and a named version,
and it swaps the binary in a way that killing the process cannot break (#1128).
`uf self-uninstall` removes uf's runtimes, links and store after listing them
(#1131). The router takes a `basePath` and a `trailingSlash` policy, applied
the same way by every server (#1129). aarch64 Linux binaries are now linked
with GNU ld instead of wild, so they carry the Cortex-A53 erratum 843419
workaround wild skipped (#1125). A CI test run that crashes now keeps its core
dump (#1124). When Deno 2.9.6 aborts inside V8 as it starts, a Deno host test
gets one more run instead of failing the job, and CI reports every retry
(#1136).

### Breaking

- **ui**: import @uniflowed/ui through its barrel alone (#1130)

### Added

- **self-uninstall**: remove uf's runtimes, links and store, after saying what goes (#1131)
- **router**: basePath and trailingSlash, asked of every front door (#1129)
- **self-update**: --check, --rollback, a named version, and a switch a kill cannot break (#1128)

### Fixed

- **test**: run a Deno host test a second time when V8 aborts inside Deno (#1136)
- **vite**: pre-bundle React's Flight client so uf dev hydrates an installed router (#1134)
- **release**: install Deno 2 in the publish job, and hold every suite job to one Deno (#1133)
- **fmt**: read comments in the guarantee helpers on the parser's stack (#1123)

### Performance

- **vite**: rewrite @uniflowed/ui barrel imports to the modules that define them (#1127)

### Internal

- **test**: keep a core, and the binary that wrote it, when the suite crashes (#1124)

### Other

- ci, release: link Linux binaries with rustc's default linker instead of wild (#1125)

## uf@0.0.0-alpha.36

_2026-09-15_

Thirty-three changes. `@uniflowed/ui`, `@uniflowed/state`, `@uniflowed/cell`
and `@uniflowed/temporal` now publish from the release workflow (#1119, #1120),
so `uf ui add` can install what its components import. The registry now has all
forty headless components (#1049, #1095). The reference renders each one live
(#1098), and `ui.directory` sets where `uf ui add` writes them (#1114). The
router installs beside the React 19.2.3 that Expo SDK 57 ships and checks for
React 19.3 only where Flight loads. `hydrateFlight` and `createDocumentRenderer`
moved to `@uniflowed/router/rsc/client` and `@uniflowed/router/rsc/ssr` (#1107).
The router also serves redirects, rewrites and response headers (#1108), and
writes the Flight payload only between elements (#1110). `uf test` can run a
suite on `bun test` (#1061), run only what a change reaches with `--changed`
(#1094), split a suite across machines with `--shard` (#1104), and measure
`bench()` against a baseline (#1111). `uf run` runs tasks across a workspace
(#1075). `uf prepare` installs a committed git hook with tasks over staged files
(#1090). `uf build --analyze` reports each route's modules and the imports
behind them (#1096). `uf lint` runs a project's own JavaScript rules (#1078).
`uf install --prod`, `uf dedupe`, `uf link` and `uf info` join the
package-manager commands (#1117). A prerendered page regenerates once its
lifetime passes (#1079), and an invalidation survives a restart (#1105). A
prerendered or cached route that reads the request is refused at build time
(#1115). Flow loads on Deno through `registerHooks` (#1023). `uf new` gains a
monorepo template, plus remote templates pinned to a commit or digest (#1099,
#1103). `uf check --explain-any` names every place a translated dependency
falls back to `any` (#1077).

### Added

- **pm**: dedupe, link, info, install --prod, and --filter/-w on add, remove and update (#1117)
- **router**: redirects, rewrites and response headers, and rewrite() from middleware (#1108)
- **rsc, build**: refuse a route written once whose render reads the request (#1115)
- **ui**: name where `uf ui add` writes components with `ui.directory` (#1114)
- **new**: remote templates, pinned to a commit or a digest and never run (#1103)
- **test**: declare benchmarks with `bench()`, and run them against a baseline with `uf test --bench` (#1111)
- **new**: a monorepo template, and every template built and checked in CI (#1099)
- **test**: split a suite across machines with `--shard`, and report it as one run with `--merge-shards` (#1104)
- **ui**: the last sixteen components in the registry (#1095)
- **build**: `uf build --analyze`, each route's modules and the imports behind them (#1096)
- **run**: run tasks across a workspace with -r, --filter and pkg#task (#1075)
- **test**: run only the test files a change since a ref reaches, with `--changed` (#1094)
- **lint**: run a project's own JavaScript rules from `plugins` in `uf lint` (#1078)
- **server, vite, cli**: regenerate a prerendered page once its lifetime passes (#1079)
- **prepare**: a committed git hook, and staged tasks over what is staged (#1090)
- **test**: run a suite with `bun test` when `test.runner` names Bun (#1061)
- **check**: name every place a translated dependency is any (#1077)
- **ui**: fourteen controls, indicators and disclosures in the registry (#1049)
- **host**: load Flow on Deno through registerHooks, and grade Deno implemented (#1023)

### Fixed

- **docs, ci**: remove a conflict marker from the testing guide, and refuse one in CI (#1116)
- **router**: write the Flight payload only between elements (#1110)
- **router**: install beside React 19.2.3 and check React 19.3 where Flight loads (#1107)
- **vite**: name a module's StyleX stylesheet by its path from the project root (#1109)
- **server**: keep an invalidation where a restarted process reads it (#1105)
- **rsc**: see through the `@uniflowed/ui` barrel to the client modules a name reaches (#1101)
- **mcp**: hold every tool's paths to the project (#1086)

### Documentation

- **ui**: render every registry component live in the components reference (#1098)
- link uf lsp and uf mcp to their guides, and close the UI guide like the others (#1106)
- describe Server Components after the Flight payload, and cite open issues for open gaps (#1093)
- **migrate**: say what moves today, and keep only the walls that are real (#1087)

### Internal

- **release**: publish @uniflowed/temporal from publish.yml (#1120)
- **release**: publish @uniflowed/cell, state and ui from publish.yml (#1119)
- **release**: name #1085 in the uf@0.0.0-alpha.35 section (#1092)

## uf@0.0.0-alpha.35

_2026-09-14_

Thirty-one changes. The tool keys alpha.34 added to `uf.config.js` now take
effect, which completes #940: each command runs on the runtime named for it
(#1040), `uf install` and `uf run` use the package manager and tools the file
names (#1050), the language server completes tool names and versions (#1031),
and the environment guide teaches the keys first (#1051). Routes render as React
Server Components through React's own Flight payload, and the docs site's
client page chunks go from 49 to 0 (#1037); when server-only code reaches the
client graph, the diagnostic names the import chain that put it there (#1082).
With that, `@uniflowed/router` needs React 19.3 and `react-server-dom-parcel`
as peers, so for now it does not install beside the React 19.2 that Expo SDK 57
and React Native 0.87 ship (#992). `uf check` translates a dependency's
TypeScript declarations into Flow and types the dependency from them instead of
`any` (#1034, #1056). The edge target serves server actions, assets and the
access log under workerd (#1032). For React Native, `uf dev` runs the project's
own Expo or React Native CLI (#1042), the router writes the route table Metro
bundles, one module per platform (#1055), and `uf install` refuses only the
install-time lifecycle scripts in a project's manifest (#1066). `@uniflowed/ui`
gains its interactions layer (#1013), and the registry gains alert-dialog,
sheet, drawer, popover, tooltip and hover-card (#1043). `uf lint --rules` lists
every rule with its level and its fix (#1058), and the lint guide maps
eslint-plugin-react, react-hooks, jsx-a11y and import onto uf (#1067). A new
project no longer fails `uf audit` on toml 3.0.0 (#1080), the MCP server holds
tool arguments to the schemas it publishes (#1072), and the package manager
decides whether a registry answered by its HTTP status rather than curl's exit
code (#1076). Flow transforms under Bun are cached on disk (#1068), and
`Temporal` from `@uniflowed/core/temporal` keeps its constructors on Node 26
and prints and refuses what the specification does (#1053, #1069).

### Added

- **rsc**: name the import chain that put server-only code in the client graph (#1082)
- **check**: type a dependency from its TypeScript declarations (#1056)
- **router**: write the route table Metro bundles, one module per platform (#1055)
- **ui**: add the interactions layer (#1013)
- **pm**: run the package manager and uf run on the tools uf.config.js names (#1050)
- render routes as React Server Components through React's own Flight payload (#1037)
- **lint**: `uf lint --rules` lists every rule, its level here and its fix (#1058)
- **dev**: run a native target's own Expo or React Native CLI from `uf dev` (#1042)
- **edge**: serve a server action, an asset and the access log under workerd, and name the Node built-ins a Worker stubs (#1032)
- **ui**: alert-dialog, sheet, drawer, popover, tooltip and hover-card in the registry (#1043)
- **cli**: run each command on the runtime uf.config.js names for it (#1040)
- **lsp**: complete tool names and versions in uf.config.js (#1031)
- **check**: translate TypeScript declaration files into Flow declaration modules (#1034)

### Fixed

- **pm**: decide whether a registry answered by its HTTP status, not curl's exit (#1076)
- **test**: wait out an axe-core audit the matcher did not start (#1073)
- **mcp**: hold tool arguments to the schemas tools/list publishes (#1072)
- **core**: make Lite Temporal print and refuse what the specification does (#1069)
- **vite**: read YAML front matter without a package that installs toml 3.0.0 (#1080)
- **pm**: refuse only install-time lifecycle scripts in the project's manifest (#1066)
- **completion**: read every parent's subcommands from clap (#1054)
- **run**: keep a digest of each task variable, never its value, in the --why note (#1057)
- **core**: hand on a native Temporal's constructors, which a spread cannot see (#1053)
- **react-testing, test**: hand the next file the window a fresh worker would (#1041)
- **ci**: order changelog versions by SemVer precedence and fetch the tags the check compares (#1039)

### Performance

- **host**: cache Bun's Flow transforms on disk, in the bytes Node reads (#1068)

### Documentation

- **lint**: map eslint-plugin-react, react-hooks, jsx-a11y and import onto uf (#1067)
- **env**: teach the tool keys first and moving from env.toolchain second (#1051)
- **editors**: `uf lsp --cwd` is read, so stop saying it is ignored (#1045)
- **guide**: give answering a request its own page, out of Routing (#1044)
- **guide**: open each guide with what it teaches, and close it with the next page (#1030)

### Internal

- **deps**: bump the actions group with 3 updates (#1085)

## uf@0.0.0-alpha.34

_2026-09-14_

Twenty changes, the same day as alpha.33, and the first release of the
push tracked in #951. `uf.config.js` can now say which tool each command uses —
`runtime: "node@26"`, `packageManager`, `build.runtime` and `build.builder`,
`test.runtime` and `test.runner` — a version prefix such as `node@26` resolves
against what its publisher has released and is locked in `uf.lock`, and
`uf env install` installs every declared tool; the commands do not run the
named tools yet, and that is the next part of #940. The language
server completes keys and values in `uf.config.js`. `uf ui add`, `list` and
`diff` copy styled components from a registry into a project. The router renders
an intercepted navigation into the slot it came from, and the manual is
reorganised into sections by reader, with new guides for type checking,
dependencies, tasks, editors and agents. On Node, Flow now loads on the
in-thread module hooks — one isolate fewer in every test worker's start-up, and
no deprecation warning on Node 26 — and `uf test` starts only the workers a
suite can keep busy.

### Added

- **env**: lock version prefixes in uf.lock and install every declared tool (#1014)
- **router, vite, lint**: an interception is a route inside a slot (#1016)
- **cli**: uf ui add, list and diff over a registry of styled components (#1025)
- **lsp**: complete keys and values in uf.config.js (#1002)
- **router**: render an intercepted navigation into the slot it came from (#1001)
- **env**: read what each publisher has released (#999)
- **stylex**: let a condition select the state a headless part announces (#995)
- **config**: declare each tool where it is used (#980)

### Fixed

- **react-native**: load the Metro helper where Metro runs, and compose with real configs (#1011)
- **server**: keep the request store one per process, whichever copy reads it (#1000)

### Performance

- **test**: start only the workers a suite can keep busy (#1018)
- **host**: load Flow on Node's in-thread module hooks (#1007)
- **bench**: time every uf command on a generated application (#1003)

### Documentation

- **guide**: write readonly where the samples wrote a variance sigil (#1026)
- **readme**: make the README a front door that cannot go stale (#1028)
- **router**: intercepting routes (#1022)
- **guide**: give type checking, dependencies, tasks, editors and agents a guide (#1020)
- **why-uf**: bring the comparisons and the gap list up to date with main (#1004)
- **nav**: organise the manual into sections by reader (#978)

### Internal

- **check**: count allocation budgets on the measuring thread, not the process (#1021)

## uf@0.0.0-alpha.33

_2026-09-14_

Seventy-six changes, two days after alpha.32, and the release that was held
back for most of them. The router grew slots all the way through — error
boundaries, templates and loading boundaries inside a `@slot`, streamed — and
the React Native target gained its contracts: a navigator, native screen
manifests derived from the route table, a Metro transform contract, and
queries over native renderer trees. `uf check` and `uf lint` got faster across
eleven changes, each held by an allocation budget so it stays fast, and the
edge adapter's worker is now smoke-tested under `wrangler dev` rather than only
driven in Node. `@uniflowed/temporal` is not in this release: #869 put it in
the published closure before npm could accept the name, which blocked every
release behind it, and #952 took it back out — import Temporal from
`@uniflowed/core/temporal` until its first publish (#560).

### Added

- **router**: derive native screen manifests (#937)
- **router**: summarize resolved route state (#936)
- **react-native-testing**: render native trees with test renderer (#932)
- **tui**: support OSC 52 clipboard writes (#821)
- **router**: add native screen router helper (#930)
- **router**: support slot error boundaries (#929)
- **react-native**: add Metro transform contract (#928)
- **router**: map native routes to screens (#923)
- **react-native-testing**: query native trees by label (#922)
- **test**: report reasoned host skips (#902)
- **router**: stream slot loading boundaries (#903)
- **react-native**: filter native queries by accessibility state (#897)
- **ui**: add toggle group render escape hatch (#899)
- **ui**: add slider render escape hatch (#895)
- **host**: support Bun module mock redirects (#875)
- **react-native**: add native testing tree queries (#892)
- **ui**: add Input OTP separator render hatch (#891)
- **router**: add native navigator contract (#884)
- **ui**: add radio group render escape hatch (#882)
- **target**: define react native test target (#877)
- **inspect**: expose test runner host support (#876)
- **router**: support templates inside slots (#874)
- materialize Bun module mock stand-ins (#845)
- **tui**: support flex auto margins (#815)
- **ui**: add hover card body render hatch (#867)

### Fixed

- **release**: keep @uniflowed/temporal out of the published closure until npm binds it (#952)
- **router**: name deferred payload rows in stream inspector (#926)
- **release**: target npm bootstrap packages (#925)
- **test**: skip host startup for empty schedules (#924)
- **rsc**: keep slot loading boundaries in client routes (#921)
- **rsc**: keep slot templates in client routes (#906)
- **rsc**: refresh action references on manifest change (#904)
- **release**: gate npm publish on existing names (#894)
- **router**: refuse slot boundary convention files (#893)
- **router**: refuse helper-built interception paths (#888)
- **router**: validate server module route grammar (#879)
- **release**: make temporal front door publishable (#869)

### Performance

- **check**: skip non-flow reads during lint scan (#938)
- **test**: scan selected paths directly (#935)
- **transform**: reduce ESTree JSON allocations (#905)
- **transform**: pre-size ESTree JSON maps (#890)
- **check**: reuse module resolution candidates (#887)
- **lint**: skip comparison-like memo names (#886)
- **check**: defer closure builtin environment (#881)
- **lint**: skip hook-like member calls (#873)
- **check**: reuse single-source facts parse (#872)
- **check**: flatten graph resolution storage (#868)
- **lint**: skip prose tree markers (#865)

### Documentation

- **architecture**: clarify root loading streaming (#931)
- **framework**: clarify quality contracts (#927)
- **ui**: enrich render hatch guidance (#920)
- **react-native**: add native target guide (#919)
- **rendering**: explain route deployment decisions (#918)
- **form**: explain server action submissions (#916)
- **ui**: refresh headless component guide (#911)
- **test**: enrich runner workflow guide (#917)
- **compare**: refresh framework parity (#915)
- **format**: explain tooling observability (#914)
- **router**: explain loading stream boundaries (#913)
- **test**: expand native accessibility queries (#912)
- **ui**: document dialog and tabs primitives (#910)
- **router**: document slot default file (#908)
- **architecture**: describe performance ratchets (#901)
- **ui**: document switch and checkbox guarantees (#889)

### Internal

- **edge**: smoke generated worker under wrangler dev (#939)
- **lib**: segment native module catalogue (#934)
- **react-native**: filter native queries by accessibility value (#907)
- **check**: stabilize resolve allocation budget (#909)
- **router**: guard root loader streaming without layout (#900)
- **lint**: ratchet router runtime allocation budget (#898)
- **check**: guard router runtime allocation budget (#896)
- isolate browser password store in CI (#885)
- **cli**: report test host support in json (#883)
- **lint**: guard router runtime allocation budget (#880)
- **router**: cover rejected payload rows after hydration (#878)
- **router**: cover repeated interception markers (#870)

## uf@0.0.0-alpha.32

_2026-09-12_

### Added

- **ui**: add popover render escape hatch (#862)

### Fixed

- **router**: preserve late payload row watches (#860)

### Performance

- **check**: defer environment setup for parse misses (#863)
- **lint**: gate derived state on setter bindings (#864)
- **lint**: skip memo tree path outside compiler boundaries (#859)
- **rsc**: cache slot client route checks (#856)

### Internal

- disable browser background startup services (#861)
- widen cold browser startup budget (#857)

## uf@0.0.0-alpha.31

_2026-09-12_

### Added

- **react-native**: expose Metro config helper (#853)

### Internal

- extend browser startup budget on CI (#854)
- **release**: check pending package tarballs (#851)

## uf@0.0.0-alpha.30

_2026-09-12_

### Added

- **ui**: add table and pagination render escape hatches (#848)
- **ui**: add field status live region (#847)
- **router**: add pure routing subpath (#844)
- **build**: record native Metro platform contract (#843)

### Fixed

- **router**: stream root loading fallbacks (#841)
- refuse Deno coverage before AOT (#842)

### Performance

- **check**: report batch allocation split (#852)
- cache repeated transform service replies (#846)

### Internal

- extend browser startup budget (#849)

## uf@0.0.0-alpha.29

_2026-09-12_

### Fixed

- **router**: tighten payload row ids (#836)
- **router**: reject ignored template file spellings (#835)
- **runtime**: guard runtime manager host manifests (#834)

### Performance

- **check**: reuse cache key strings (#837)
- **lint**: scan react hook gates once (#838)

## uf@0.0.0-alpha.28

_2026-09-12_

### Added

- **build**: expose native target contract (#831)

### Fixed

- **vite**: keep rsc boundary importers in client routes (#832)
- **router**: pass params to slot defaults (#830)

### Performance

- **check**: reuse dependency digest hex buffer (#829)
- **lint**: require call-shaped react tree gates (#828)

### Internal

- cover Bun module mock aliases (#827)

## uf@0.0.0-alpha.27

_2026-09-12_

### Performance

- **lint**: skip non-code react tree hook gates (#822)
- **check**: reuse dependency digest buffers (#824)

### Internal

- **rsc**: cover package client boundary routing (#823)

## uf@0.0.0-alpha.26

_2026-09-12_

### Added

- **ui**: add field render escape hatch (#820)

## uf@0.0.0-alpha.25

_2026-09-12_

### Added

- **ui**: add accordion render escape hatch (#814)
- **ui**: add tooltip body render escape hatch (#817)

### Performance

- **check**: skip environment build on full cache hits (#816)

### Other

- **tui**: mark key release support implemented in the roadmap (#818)

## uf@0.0.0-alpha.24

_2026-09-12_

### Added

- **ui**: add collapsible render escape hatch (#811)

### Performance

- **lint**: avoid rendering ESTree for derived-state effects (#812)

## uf@0.0.0-alpha.23

_2026-09-12_

### Added

- **ui**: add render escape hatch to skeleton parts (#809)
- **ui**: add render escape hatch to avatar parts (#806)
- **ui**: add render escape hatch to alert parts (#805)

## uf@0.0.0-alpha.22

_2026-09-12_

### Added

- **ui**: add render escape hatch to breadcrumb parts (#803)
- **ui**: add render escape hatch to small parts (#802)
- **tui**: report key release events (#801)

## uf@0.0.0-alpha.21

_2026-09-12_

### Added

- **server**: generate openapi from route handlers (#800)
- **deploy**: add deno adapter (#799)
- **std**: ship tar archive (#795)
- **std**: ship zip container (#794)
- **std**: ship text protocol headers (#793)
- **std**: ship buffered scanners (#791)
- **std**: ship byte io adapters (#790)
- **std**: ship cancellable timers (#789)
- **std**: ship non-cryptographic hashes (#788)
- **std**: ship slash glob helpers (#787)
- **std**: ship slash path helpers (#786)
- **std**: ship binary cursor and varints (#785)
- **std**: ship csv codec (#782)
- **std**: ship list container (#781)
- **std**: ship base32 codec (#780)
- **std**: ship slice search helpers (#779)
- **examples**: rebuild Commonplace with scoped server data and clips (#772)
- **router**: resolve routes for native targets (#769)
- **react-native**: re-export React Native runtime (#771)

### Fixed

- **rsc**: classify project hook wrappers (#798)
- **rsc**: keep package client boundaries (#797)
- **check**: keep package resolver on pinned Flow APIs (#796)
- **config**: evaluate task config modules (#784)
- **check**: resolve Flow export conditions (#783)
- **check**: prefer libdef declarations over package manifests (#778)
- **project**: read package.json workspaces (#777)
- **check**: report host-conditional package exports (#775)
- **deno**: scope package imports in loader map (#774)

### Performance

- speed up tests and type checks (#773)

### Internal

- **config**: split evaluated projection parsing (#776)
- warn when library exports are not published (#768)
- deny Rust warnings at workspace level (#767)

### Other

- Document Async React primitives (#770)

## uf@0.0.0-alpha.20

_2026-09-10_

### Internal

- **ci**: harden publish browser wrapper (#761)
- **ci**: give npm publish tests more room (#760)
- **ci**: deny clippy warnings with all features (#759)

### Other

- release: uf@0.0.0-alpha.20 (#766)
- env: read exact tool pins from package engines (#765)
- upstream: silence remaining Flow deref warning (#764)
- check: resolve package imports aliases (#763)
- release: keep untrusted packages out of the OIDC publish closure (#762)

## uf@0.0.0-alpha.19

_2026-09-10_

### Added

- **test**: the browser as a host, so a component test runs where a component runs (#728)
- **router, vite, lint**: a slot is a route into a named place, and the layout that receives it (#749)
- **infra, ci**: a front door for uniflowed.dev, and the redirect it must not break (#747)
- **router, docs**: the rows a payload arrives in, and the boundary each one resolves (#742)
- **std, lib, cli, docs, release**: a status per std specifier, and the six that are code (#743)
- **router, vite, dev**: what left the server, in what order, and what each chunk built (#745)
- **tui, lib, docs**: a drag that selects the cells it crossed, and the one default preventDefault now has (#741)
- **ui**: the escape hatch a copy step is for, on ten components rather than one (#740)
- **lib, rsc**: the ui subpaths that are client modules, and the guard that reads the files (#731)
- **tui**: a scrolling box that costs its window, not its content (#721)
- **cli, task**: the command a task names, started rather than handed to `sh` (#725)
- **cli, test**: a Deno Flow loader that is a transform and a map, not a hook (#720)
- **rsc**: whose `useState` it is, and which name an import bound (#726)
- **server, vite, cli**: the targets that keep a process run their schedules too (#724)
- **server, vite, cli**: the `scheduled()` a cron trigger needs, and the trigger beside it (#719)
- **ui**: the five that look like a class list and are not (#714)
- **std**: the six Go standard library modules JavaScript does not have (#711)
- **upstream, check**: patches the sync applies, and the inference fix one was waiting for (#709)
- **config, router**: one shell, and the routes a browser cannot render (#706)
- **cli**: a schedule declared in a route handler, and the build that refuses to ignore it (#712)
- **server**: a durable route cache, so a restart does not empty it (#704)
- **router**: show which DOM subtree each boundary owns (#703)
- **config, router**: a link the browser follows, and the key that says so (#702)
- **server**: schedules, and the targets that may not hold one (#696)

### Fixed

- **vite**: resolve @uniflowed/react to the React peer in browser modules (#752)
- **ui**: store toast queue in @uniflowed/state (#753)
- **check**: prefer module package entry over main (#751)
- **nix, ci, docs**: the toolchain the flake was missing, the build nothing ran, and the page that never had Nix on it (#738)
- **cli, deploy**: the schedule an artefact declares and the entry that runs it, checked both ways (#737)
- **fmt**: a member chain is its own fixed point (#746)
- **integrations, ci, docs**: the install a `uf check` is worthless without, and the recipes read back against the binary (#739)
- **config, cli, rm**: the four runtime names that are not hosts, and the key that is (#744)
- **release**: a package that is never published says so in its manifest (#730)
- **check, lint**: a library definition is merged, not linted (#732)
- **term, tui**: `NO_COLOR` takes the colour and leaves the characters (#727)
- **upstream, check**: the elements before a tuple pattern's spread bind what they were handed (#716)
- **config**: the refusal names the expression it refused, and its line (#715)
- **cli**: no cron trigger without a handler to receive it (#717)

### Performance

- **check, upstream**: the set an SSA normal form costs, once instead of once per node (#748)
- **lint**: one file, one reading — `uf lint` parsed each module three times (#708)
- **transform**: `json!` deep-copies what it is handed, so stop handing it subtrees (#705)
- **check**: where the checker's two million allocations go (#713)
- **lint, transform**: put the profiler in the hot path it was written for (#697)

### Documentation

- **contributing**: the exclusion this repository is waiting for does not exist (#734)
- the server half is a BFF, and "full-stack" is not what uf is aiming at (#733)
- **rsc**: the server action and the boundary it crosses (#701)
- **cli, mcp**: `uf check --json`'s `errors` counts findings `diagnostics` does not hold (#694)

### Internal

- warm Blacksmith caches (#754)
- exercise React 19.3 defaults (#755)
- **packages**: move the JavaScript suite beside the code it tests (#729)
- **server**: the fifth front door, inside the comparison that keeps them one (#723)
- **packages**: close the allowlists that would publish a co-located test file (#722)

### Other

- Adopt dollar routes and add SSR social example (#757)
- release: bootstrap pending package names (#756)
- release: twelve waiting packages are on the published list (#750)

## uf@0.0.0-alpha.18

_2026-09-08_

Nine changes, and the thread running through most of them is a gap between
what uf said and what uf did.

`uf dev` streams a page now instead of collecting it. That was the one place a
developer would notice streaming and the one place it did not happen, so a slow
page showed nothing until it was finished and `$loading.js` looked broken.
`uf mcp` serves the read-only commands over a protocol an agent speaks, instead
of leaving one to shell out and parse `--json`. And `uf build --adapter bun`
writes a directory `bun server.js` runs — the second host, and it exists
because somebody ran the benchmark the seam had been waiting on rather than
assuming its result. Bun answers a dynamic route 32% faster and a small static
asset 54% faster, and the whole of the static win is `Bun.file` rather than the
handler around it, which is why the traversal and cookie policy stayed shared
and only one line differs per host.

The RSC check decides 57 hooks it used to ask about. `@uniflowed/hooks` has
carried `server_component_safe` for every hook it exports since before
`uf_rsc` existed, and nothing read it: a Server Component calling
`useMediaQuery` got a warning saying uf could not say, from a binary that
could. It is an error now, and it names the package. The hooks uf still cannot
decide — `useRoute`, and anything a project wrote — say so exactly as before.

The rest is the release pipeline and the notes about it. The Intel macOS target
is built on Apple silicon and verified natively, which is the answer to the job
that cost four times every other target and shipped alpha.9 to npm with no
binaries behind it. Two of the three doors Bun shut on module mocking have
opened on 1.3.13, so what is left there is writing an implementation rather
than waiting for Bun — the opposite of what that file said. And three release
notes named a command uf does not have, one of them in the release before this:
what they called uf profile, uf rm and uf approve-builds are `uf_profiler`,
`uf self-update` and `uf pm approve-builds`. A test asks clap now, and walks a
nested path to its end, so there will not be a fourth — and the three wrong
names are unquoted right there because that test reads this file, and a
backticked `uf <name>` in it is a thing a reader will type.

### Added

- **rsc, lib**: the hooks the registry already knew about are decided, not asked about (#690)
- **server, vite, cli**: the bun deploy adapter, and the one line that makes it one (#689)
- **cli, ui**: the commands an agent can call over MCP, and the stdout a protocol needs (#688)
- **vite, router**: `uf dev` streams a page instead of collecting it (#685)

### Documentation

- **cli**: three release notes named a command uf does not have (#692)
- **host**: two of Bun's three shut doors are open on 1.3.13 (#693)
- **roadmap, config**: say that uf runs its own tasks, because it does (#687)

### Internal

- **deps**: Bump the fuzz group in /tools/fuzz with 2 updates (#691)
- **release**: build the Intel macOS target on Apple silicon, verify it natively (#686)

## uf@0.0.0-alpha.17

_2026-09-08_

Two changes, and both are a thing this repository already knew and had no way
to check.

`uf check` now has the allocation report `uf lint` and `uf transform` have had
since alpha.16 — and running it settles a question that had been open since the
checker's cost was first counted. The cache saves 93% of a re-check of an
unchanged file, and one allocation of a re-check after a keystroke. The first
is the caller nobody was worried about and the second is the caller everybody
was: a record's key is over the file's own text, so an editor holding a buffer
asks for a key nothing has been filed under. That is not a defect in the cache,
it is what content addressing means — and it is now the measured reason an
editor needs something the cache is not, rather than a suspicion about it.

The other is a table that had fallen six names behind the package it describes.
`hook_descriptors()` says it names every hook `@uniflowed/hooks` exports and
named 52 of the 58, so `uf inspect` under-reported and anything asking what a
compiler may assume about `useRenderedAt` was told nothing at all. The registry
entry beside it was complete, and held there by a test; this one had none. It
does now, and it compares the two lists both ways round.

### Fixed

- **lib**: the hook table had fallen six behind the package it describes (#683)

### Performance

- **check**: count what a check allocates, and answer whether the cache avoids it (#682)

## uf@0.0.0-alpha.16

_2026-09-08_

Twenty-one changes. The one to read first is the one that made a check pass by
checking nothing: `uf check` in an npm workspace resolved a hoisted dependency
to nothing, so an app in a monorepo — where the dependency is installed at the
root and not beside the project — was typed as `any` throughout and told it was
fine. Three separate causes had to be fixed for the fix to do anything, and the
last of them was a path resolver refusing a `..` it could not cancel, which is
why the first two had looked like they worked.

Beside it, a diagnostic now counts the columns a path *draws* rather than the
ones its scalars would draw alone. A name built out of emoji-presentation
sequences rendered at twice the cap meant to keep it inside one row — the same
family as alpha.15's lead, and the same input: a name out of a clone uf did not
write.

The rest is spread evenly. A form the browser submits reaches a server action,
and `uf dev` says which module put a component in the client bundle and why.
Every route has a typed link, including the ones that take no parameters. A
test can live beside the code it tests, and an accessibility audit reads the
tree while you edit it. And `uf_profiler` is a profiler rather than a
wall-clock number — hierarchical spans, a counting allocator, and a report
saying where the time and the memory went, for the benches and the
`alloc_report` examples that link against it. Its first findings are in this
release: building
a Babel tree costs 31% fewer allocations, the effects rule stopped asking for a
tree it never used, and the formatter lost two more passes. `uf lint` over a
111 KiB module went from 839,000 allocations to 442,000.

### Added

- **test, transform, vite**: a test beside the code it tests, and an audit that reads the tree (#643)
- **router, rsc, dev**: a form the browser submits to a server action, and the chain that puts a module in the client bundle (#636)
- **profiler**: where uf's time and memory actually go (#660)
- **router, check**: a typed link for every route, including the ones that take no parameters (#655)
- **build, cli**: the runtime a compiled binary embeds is the project's own, and the machine it is for (#638)

### Fixed

- **vite**: a dev 404 for a page says which header decided it (#677)
- **term**: the columns a path draws, not the ones its scalars would draw alone (#672)
- **server, vite, infra**: one rule for a prerendered document, and a policy on the site that argues for them (#652)
- **check**: a hoisted dependency is read, so a workspace app has types at all (#667)
- **config, testing, react-testing**: a config type a test holds, and three surfaces that named nothing (#644)
- **config, vite**: the MDX highlighter reads the keys the reference documents (#664)
- **term, cli, security**: a diagnostic prints four things out of a checkout, not one (#663)

### Performance

- **lint**: react/derived-state needs a setter, so a module with no useState is not parsed (#670)
- **lint, transform**: the effects rule reads the lowered tree, not the Babel one (#676)
- **transform**: 31% fewer allocations building the Babel tree (#673)
- **fmt**: build a concat straight into the arena rather than through a Vec (#665)
- **fmt**: compute a node's sorted children once rather than once per comment (#661)

### Internal

- **std**: hold the form/ui edge to a type, and say why it points that way (#674)
- **vite**: two dev-server tests ask the operating system for their port (#669)
- **release**: name #650 in the alpha.15 section (#662)
- **release**: the changelog date is UTC, held by two commits an hour apart (#647)

## uf@0.0.0-alpha.15

_2026-09-08_

Twenty changes. The one to read first is the one that was wrong in a way
nothing could see: a diagnostic printed a module path exactly as it came off
the filesystem, and a repository is attacker-authored input — `uf` is run
against a clone, and a file in it can be named `src/\x1b[2Jgotcha.js`. Registry
text has been stripped of control characters since the progress display existed;
filenames were not, in any reporter that named one.

The rest is spread evenly. `uf check` reads a project's `.flowconfig` `[libs]`
when it has one, resolves a package to the copy Node would load, and knows the
half of `SubtleCrypto` a module that signs anything needs — it had `digest` and
no `sign`, `importKey` or `CryptoKey`, so correct code was told it was wrong. Four
parsers now share one option set, so a syntax error says the same thing
whichever of them found it. `uf build --adapter static` refuses what a static
host cannot serve rather than writing output that 404s. `OgImage` is a template
uf draws, with fonts it subsets and icons it sprites. And `uf self-update` is
the command that replaces `uf`, rather than the one that only said so.

### Added

- **build, project**: the other build — a library, and what one ships (#650)
- **i18n, cli, test**: the five refusals a message's type owes, and the catalogue a translator gets (#633)
- **vite, router, query**: React DevTools on purpose, Strict Mode by default, and the request it was cancelling (#631)
- **lib, cli, ui, stylex**: a readiness per component, the parts list a test reads, and the constraint a select group states (#628)
- **cli, rm**: the command that replaces uf, and the one that only said so (#627)
- **assets**: OgImage as a template uf can draw, fonts it subsets, and icons it sprites (#625)
- **cli, config, router**: the static target refuses what a static host cannot serve (#621)

### Fixed

- **pm, exec**: the root pnpm refused, the links the delta dropped, and the shim Windows runs (#637)
- **check**: the half of SubtleCrypto a signing module needs, and the CryptoKey it names (#651)
- **term, cli, security**: a filename is text a terminal draws, not a command it runs (#649)
- **test, server, host, tui, cli**: five defects, and the seam behind the worst of them (#642)
- **fmt, transform, corpus, pm**: `for await` only over `of`, and one manifest for the corpus (#629)
- **check, flow, transform**: one option set for four parsers, and uf's words for a syntax error (#645)
- **check**: the `[libs]` a project declares, and the copy of a package Node would load (#626)

### Documentation

- two claims that are ahead of what uf does (#639)

### Internal

- **merge-queue**: the checks a batch is merged on, and the trigger they report to (#648)
- **semver**: compute the crates that cannot be compared, rather than listing them (#657)
- **publish**: the verify job reads the version the publish job published (#641)
- **publish**: the runtimes the workspace suite starts, in every job that runs it (#635)
- **config**: the whole rule table survives naming one rule, walked entry by entry (#618)

## uf@0.0.0-alpha.14

_2026-09-07_

Twenty-one changes, and the ones worth reading first are the two that were
wrong in a way nothing could see.

`@uniflowed/test` could not be imported on Bun at all. `ref()`/`unref()` set a
flag on Node and *count* on Bun, and the transform service said what it wanted
on every request rather than on the transition — so two sibling imports left
the host referenced for ever, and every program that imports the test package
is fifteen modules. This lands a week after the Bun host was made to load at
all, which is the argument for the test that now starts a real Bun process
rather than a claim in a README.

Every React Compiler finding pointed at the wrong line, and a quarter of them
did not exist. The position fell back to a field this compiler never populates,
so all 173 findings named their `component` or `hook` keyword; the plugin's
de-duplication then collapsed them to 119 lines, and the 54 it dropped were
distinct sites — five different ref reads in one component. Findings also could
not name their function, so all 173 said `a function`. They name 74 distinct
functions now.

`uf check` gives the same answer twice. The wall-clock inference budget is
gone: a file the clock aborted was written to no record, so the next run
inferred it again, off the clock that time, and reported errors the first run
had thrown away. What bounds the checker now is work — recursion depth, type
expansion, source bytes — and four cold runs over the same tree that varied by
1.77× from machine load are why.

And a build that emits no server refuses a callable server action instead of
shipping a wired button and a 404; `uf build --adapter static` says which route
handler, which middleware, which unprerendered route and which action stand in
the way. Draft mode survives the request it was enabled in. `lint.rules` merges
into uf's table instead of replacing it, so naming one rule no longer switches
the other fifty-three off. The manual's "next page" reaches the end.

### Added

- **router, cli**: the files a route is, and a render anchor nobody has to wire (#616)
- **lint, config**: accessibility and markup rules, a guard that does not refine, and a rule table that stopped being one (#623)
- **runtime, config, test**: a permission set uf owns, and a host table that is checked (#617)
- **server, router**: draft mode that survives the request, and every refusal said out loud (#615)
- **ui, form**: the other three menus, the Enter a checkbox owes its form, and one place that computes aria-describedby (#613)
- **build, config**: what a build produces, the two settings that decide it, and a builder that is a provider (#609)
- **tui**: the mouse, routed by a grid the painter records (#608)
- **lint, hooks, router, state**: an effect that is not one, a memo the compiler already did, and a literal that was never conditional (#606)
- **pm, config, release**: provenance, a scope bound to one registry, and an origin the release host cannot forge (#604)
- **vite, dev, router**: one renderer, a watched environment, and a channel from the browser (#602)

### Fixed

- **test**: a tick over work that did not happen, and four more the runner told itself (#605)
- **cli**: one list of what `uf explain` describes, rather than two that drifted (#624)
- **test, cli, host**: `expect`'s last any, the port `uf dev` bound, and a Bun host that could not load the test package (#622)
- **transform, vite**: a React Compiler finding that names its function and points at itself (#611)
- **hooks**: `useHash` hears a `pushState` it did not make (#610)
- **check**: one record per file, an answer per batch, and no clock in the answer (#603)
- **prepare, lint, test**: a path a run names survives `.gitignore` (#601)
- **router**: every extension the build runs is a route, and a name it cannot run is not (#580)
- **docs**: the manual lists State once, so "next page" reaches the end (#587)

### Documentation

- **security**: the threat model's rows are inside its table again (#592)

### Internal

- **docs, security, vite**: every link the build wrote resolves, and a scan that is uf's own (#595)

## uf@0.0.0-alpha.13

_2026-09-08_

The pipeline stops merging things it did not check. Every job in `ci.yml`
depends on `Toolchain`, and a job whose dependency fails is not run — it is
*skipped*, which GitHub counts as a required check being satisfied. So a
`main` that did not compile was merged on 2026-09-06 with five green ticks,
and again on 2026-09-07. The gate job that fails unless every job actually
succeeded is now a required context, and `ci:gate` checks the gate itself:
a job added to the pipeline and left out of the gate's `needs:` is refused by
name, because a hand-maintained list of what to check is the same shape as the
bug it was written to fix.

Bun runs. `@uniflowed/host`'s preload returned `undefined` from Bun's
`onLoad`, which Bun rejects, so no Bun project could load it at all — and the
naive repair breaks every CommonJS dependency, because anything leaving
`onLoad` is an ES module and there is no value that means "not mine". The
filter carries the policy now, which also closed a divergence nobody could
see: the hand-written list never gained `.cjs`, so Bun and Node disagreed
about which files were Flow. There is a test that starts a real Bun process
against the real binary, which is the first time any Bun claim in this
repository was checked rather than asserted. The answer caches are bounded
too — `transform` had reached 767 entries and was growing by about 330 a
build, with nothing that ever removed one.

A server action is not the only thing a browser can reach any more: `QUERY`,
server-sent events, an upgrade a handler can accept, and a queue; OAuth as a
contract rather than a provider; and a logger whose request id reaches a
render. The router streams its loaders instead of awaiting them, keeps the
site around a 404, and recognises `@slot` and `(.)segment` — refusing each by
name rather than serving it as a literal URL, which is the first half of
parallel and intercepting routes and not the whole of them. A message's
arguments are in its type. And `uf release` can no longer lose a
commit whose subject carries no pull request number, or rewrite a changelog
section that has already been published — both of which had happened.

### Added

- **ui, temporal**: the sets adopt the positioner, and the grid it does not fit (#597)
- **server**: OAuth as a contract, and a logger with a request id a render can read (#585)
- **hooks, router**: the address bar, and a hydration error that names the node (#582)
- **router**: a wrapper that remounts, and two spellings refused by name (#472)
- **i18n**: the arguments a message takes, in the type (#567)
- **router, docs**: a navigation that is a transition, and the rest of what a page says about itself (#546)
- **fmt**: the formatter's own options, without uf re-declaring any of them (#577)
- **server**: QUERY, event streams, an upgrade a handler can accept, and a queue (#563)
- **router**: the loader streams, the head is a head, and a 404 keeps the site (#566)
- **ui**: groups in a combobox, a drag on a splitter, and the claim nobody was holding (#558)
- **web**: the five numbers a page is judged on, read from the browser (#555)
- **run**: a task graph, a concurrency limit, and a cache keyed on declared inputs (#466)
- **rsc**: a `"use server"` export the browser can call (#473)
- **temporal, web, hooks**: Temporal on every host, and the two values a render has to fix (#554)
- **pm**: `uf pm approve-builds`, and the sentence npm gets instead (#545)
- **pm**: `uf update` reports what the ranges hold back, and rewrites them (#541)
- **cell, state**: state and derived, and no way back to cell and computed (#544)
- **env**: one .uf, and the toolchain links out of the project (#539)

### Fixed

- **release**: a commit with no pull request number, and a binary older than the tree (#589)
- **host, infra**: a bound on the answer caches, and a Bun host that runs (#594)
- **test, query**: a stub that does not outlive its file, and a clock the collection test owns (#579)
- **project**: a file git is told to ignore is not the project's to format (#576)
- **lint**: a property key is a key however its variance is written (#572)
- **react-testing, test**: an accessible name, a summary, and two surfaces that were any (#571)
- **server, docs**: a symlink cannot leave a served directory, and the CVE (#556)
- **lint**: four false positives, and the line scanner behind the worst of them (#548)
- **release**: `verify-npm` failed a release that had worked (#538)
- **release**: `promote-latest` handed npm the plan file as stdin (#537)

### Documentation

- **config**: a condition rather than a count, and a key that does not promise what it does not do (#578)

### Internal

- **gate**: the gate that covers every job, checked rather than trusted (#590)

### Other

- Update GitHub Sponsors username in FUNDING.yml

## uf@0.0.0-alpha.12

_2026-09-07_

The command line says what it does. `uf create app` took one optional argument
that was a template when it named one and a directory when it did not, so
`uf create app react` and `uf create app my-site` did two different things and
neither spelling said which; it is `uf new <path>` and `uf init` now, with
`--lib` for a library, and the old spelling still works and is no longer
listed. `uf ls`, `uf audit` and `uf search` reach three more of the things a
person does to a dependency tree — and where a manager has no such
command, which is Yarn and bun for `search`, uf says so rather than running a
manager the project did not choose. `uf clean` removes what a rebuild writes
again, with the line drawn at the network: never `node_modules` unless asked,
and never a lockfile at all.

And a link to the documentation shows a card rather than a cropped logo.
`og:title` and `og:description` fall back to the page's own title and
description, so a page that has said what it is called has said what its card
is called; `og:type` and the image's alt text are there too. `brand/og.png` is
the mark and the name on the dark ground — 1200×630, rendered from
`brand/og.html`, which is CSS somebody can read.

### Added

- **router, docs**: the share card a page already had the words for (#533)
- **cli**: uf clean, and a boundary at the network (#532)
- **cli**: uf init and uf new, and one fewer way to install in uf info (#489)
- **pm**: `uf ls`, `uf audit`, `uf search` and `uf uninstall` (#525)

## uf@0.0.0-alpha.11

_2026-09-07_

`uf check` reads what a file imports. It used to check the files it was handed
and nothing else, so `uf check <path>` resolved nothing:
`import type { Control } from "@uniflowed/form"` was an `any`-typed value and
every annotation written against it went unread — fifteen `value-as-type`
errors on `packages/form/watch.js` about types that were correct. The batch is
now the closure of what the requested files import, assembled with the
checker's own resolution rules, and a specifier the project cannot answer is
looked for under `node_modules`. So a project that merely *uses* uf is checked
against uf's real types: a scaffold types its eight files against thirty-one
modules read from the install, where before it typed them against `any`. Two
conditions bound what is read, and both are Flow's — a package Flow's own
library definitions describe is left to them, and a package that declares no
`@flow` exports `any` whether it is read or not. Diagnostics are still reported
only for the files that were asked about.

And uf installs itself in CI in one step. `integrations/` holds a GitHub
composite action, a GitLab `include:` template and a CircleCI orb, all three
running the same installer a human runs — which is why they are thin, and why
none of them skips the checksum. They agree on the awkward parts: pin the
version, never cache `latest`, and never believe a restored cache without
running the binary in it.

### Added

- **build, router**: a sitemap and a robots.txt, and three manifests out of dist/ (#471)

### Other

- uf check reads what it imports, and three CI setups that install uf (#474)

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
  experience, typed by Flow."
  ([#440](https://github.com/ubugeeei-prod/uf/pull/440), commit `f3f743c`, whose
  subject carries no pull request number)
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
- **router**: the nearest `$not-found.js` answers, not the one at the root,
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
- **router**: `$not-found` is a reserved name, and the linter says so (#224)
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
