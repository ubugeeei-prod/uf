# Changelog

## uf@0.0.0-alpha.24

_2026-09-12_

### Added

- **ui**: add render escape hatch to avatar parts (#806)
- **ui**: add render escape hatch to alert parts (#805)

## uf@0.0.0-alpha.23

_2026-09-12_

### Added

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
