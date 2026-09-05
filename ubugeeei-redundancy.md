# ubugeeei Redundancy Guide — uf

This document is the continuity guide for uf. It records the product direction,
architectural constraints, quality standards, and development practices that
contributors and coding agents must follow when ubugeeei is unavailable,
temporarily or permanently.

Use it to make progress without depending on undocumented personal context.
It describes what uf must become, not a claim that every capability below is
already implemented.

## Mission

**Build the strongest React development experience with Modern Flow.**

uf is a Flow-first, React-focused, natively implemented unified toolchain and
standard library for production frontend development.

It must be runtime-agnostic, bundler-agnostic, deployment-platform-agnostic,
strictly typed, exceptionally fast, and usable as a coherent all-in-one platform.

This is not a hobby project, a demonstration, or a collection of thin wrappers.
Both the toolchain and its libraries must be complete enough to support serious
production applications.

The ambition is to outperform Vite+ with TypeScript, Next.js, Vitest, Bun Test,
EffectTS, Jotai, Valibot, Immer, React Hook Form, shadcn/ui, and React equivalents
of VueUse in the areas uf serves.

Earn that position through better type safety, inference, performance,
integration, usability, and implementation quality—not unsupported claims or
superficial feature checklists.

## Architectural Commitments

### Flow and React Are the Specialization

uf is deliberately opinionated about Flow and React. It is not intended to be
UI-framework-agnostic.

Runtime, bundler, and deployment independence must not dilute its understanding
of Flow syntax, React semantics, or end-to-end application types.

Follow React's public contracts and design principles. Do not build features
whose correctness depends on circumventing them.

### Native Tooling, Flow-Native Libraries

Implement performance-critical tooling in Rust.

Keep the native core independent of any one JavaScript runtime. Isolate
JavaScript execution behind a **Capability JS Host** abstraction with Node.js,
Deno, and Bun implementations.

Distinguish the host that executes tooling or plugins from the runtime targeted
by an application. Make required capabilities and unsupported combinations
explicit. Do not silently invoke Node.js and describe the result as runtime
independence.

Implement the standard library in ordinary **`.js` files with Flow**, not
TypeScript implementations with Flow declarations attached afterward.

In particular, **Effect and Validator must be implemented entirely in
Flow-typed JavaScript, without native bindings or corresponding implementation
crates**. Apply the same Flow-native direction to state management, immutable
updates, forms, hooks, and UI libraries.

This requirement applies to application-facing library implementations. It does
not require their surrounding build pipelines or other toolchain infrastructure
to be implemented in JavaScript.

### Native Hot Paths at Scale

**Any uf-owned processing that can become a material performance bottleneck in
large projects must be implemented in Rust or delegated to an existing
high-performance native upstream implementation.**

This is a requirement, not a preference to revisit after implementation.

Pay particular attention to build pipelines: file discovery and hashing,
dependency and module-graph analysis, cache management and invalidation,
transformation coordination, code generation, asset and media processing,
styling integration, and task and test scheduling.

Do not place repeated, repository-wide or CPU-intensive work in JavaScript
merely because its public API is written in Flow.

The execution phase determines the implementation boundary, not the package
name. Keep application-facing libraries in Flow while placing expensive
build-time infrastructure in Rust. Do not use this rule to introduce native
bindings or implementation crates for Effect or Validator.

Keep configuration and JavaScript extension points expressive, but avoid making
them the bulk-data processing layer.

Minimize repeated parsing, allocation, copying, serialization, process startup,
and cross-language calls. Prefer incremental work, precise cache invalidation,
batched host communication, and bounded parallelism where measurement
demonstrates a benefit.

Measure the complete path, including JavaScript plugins and runtime adapters,
rather than only its Rust core.

Reuse Vite, Rolldown, Vite Task, and official parsers and compilers instead of
reimplementing them.

When an upstream or JavaScript-only integration limits performance, profile it,
optimize uf's integration, and pursue upstream improvements. Document the
limitation rather than concealing it behind a native wrapper or replacing
upstream semantics with an incomplete approximation.

### One Configuration Surface

Make **`uf.config.js`** the unified, Flow-typed configuration entry point for
development, builds, type checking, linting, formatting, testing, tasks, runtime
selection, styling, and deployment integrations.

Users should not have to maintain separate configuration files for every
integrated tool.

Provide sensible defaults and documented customization through this surface,
without hiding underlying behavior or turning advanced use cases into an
ejection problem.

Defaults must be convenient. Boundaries must remain explicit. Integrations must
remain replaceable.

### Vite by Default, Not a Vite Reimplementation

Use Vite as the default development and build integration.

Reuse its bundling pipeline, plugin system, and development server rather than
reimplementing them. **Do not rebuild Vite or Rolldown inside uf.**

Keep bundler-specific behavior behind explicit integration boundaries so another
backend can be supported without rewriting the toolchain or standard library.

Vite is the default, not an architectural lock-in.

An adapter interface alone does not establish compatibility. Each supported
backend and host combination needs working integration tests.

### Prefer Upstream Parsers and Compilers

Use the official Flow parser and type-checking infrastructure, and the official
React Compiler.

Do not create approximate replacements for their language semantics or compiler
optimizations.

Integrate upstream Rust implementations where applicable. Prefer suitable
published crates; when those are unavailable, use pinned upstream source through
submodules, including `facebook/flow` and `react/react`.

Verify actual upstream interfaces rather than assuming source availability
means drop-in integration. Keep integration glue small, document local patches,
and test upstream upgrades.

### StyleX by Default, Replaceable by Design

**Use StyleX as the default styling system.**

Integrate it into development, production builds, the design system, component
stories, and documentation. Configure it through `uf.config.js` without requiring
users to assemble a separate styling toolchain.

Use upstream StyleX implementations and preserve their semantics. Keep
uf-owned integration hot paths native and incremental. When an upstream
JavaScript transform is necessary, isolate and measure it rather than treating
its cost as invisible.

Test the complete Flow, React Compiler, StyleX, and bundler pipeline together.
Include development updates, production CSS output, server rendering, and
hydration in that coverage.

**StyleX must be a default, not a mandatory dependency of the application
architecture.**

Allow projects to disable or replace the default integration through
`uf.config.js`. Keep plain CSS, CSS Modules, and alternative styling integrations
possible without ejecting from uf.

Keep headless UI behavior independent of styling. Do not expose StyleX-specific
types through otherwise styling-independent public APIs. Provide reusable design
tokens and deliberate customization boundaries.

Prebuilt UI packages should not require consumers to run the StyleX compiler
merely to import already-built components.

Switchability does not mean silently translating StyleX-authored application
code into another styling language. Document the source-level migration boundary
and test at least one non-StyleX application path.

### Platform-Agnostic Deployment

**Applications built with uf must not be tied to a particular hosting company,
cloud product, operating system, or deployment architecture.**

Treat deployment-platform independence as distinct from JavaScript runtime
independence.

Support deployment across static hosting, self-hosted servers, containers,
serverless functions, and edge environments through explicit capability
contracts and adapters.

Make self-hosting a first-class workflow, not a fallback. A Void deployment
integration may provide an excellent default experience, but it must never
become a requirement for using uf's framework.

Keep provider SDKs and proprietary services out of portable application and
framework contracts. Prefer standard Web APIs where they fit the execution
environment.

Separate portable request handling and application behavior from platform
facilities such as caches, storage, scheduling, background execution, secrets,
asset delivery, and media processing.

Native build tooling must not automatically imply a native dependency in the
deployed application. Produce artifacts appropriate to each target. Do not make
an edge deployment require a Rust binary or a Node-specific binding simply
because the build toolchain uses them.

Aim to deploy wherever the required capabilities can be provided. Do not pretend
every environment offers identical capabilities: static hosting does not become
a server merely because an adapter exists.

For SSR, RSC, Server Actions, ISR, streaming, and other server-dependent features,
document requirements, supported targets, and explicit alternatives when a
capability is unavailable. Reject unsupported configurations clearly rather than
silently changing semantics.

Maintain a tested compatibility matrix for build hosts, application runtimes,
deployment targets, operating systems, and architectures. Distinguish verified
support from planned support.

## Toolchain Responsibilities

| Area | Required direction |
| --- | --- |
| Type checking | Integrate official Flow checking throughout development and preserve useful diagnostics across generated and transformed code. |
| Linter | Build a Rust linter and combine its diagnostics with Flow's built-in lint checks. Provide consistent severities, locations, suppressions, and reporting. |
| Linter plugins | Support plugins authored in `.js` with Flow through the Capability JS Host. Flow syntax must work without additional parser plugins, transpiler setup, or a separate plugin configuration file. Keep the plugin contract runtime-independent. |
| Formatter | Implement Flow-aware formatting in Rust using the official Flow Rust parser. Delegate other supported file types to Biome Formatter. Keep formatting native and explicitly track unsupported formats. |
| Test runner | Build uf's own high-performance Rust runner, targeting Bun Test-level speed while remaining runtime-agnostic. Rust owns orchestration; JavaScript tests execute through runtime adapters. Do not substitute a wrapper around Vitest or a Bun-only runner. |
| Task runner | Use Vite Task as the default task engine. Integrate its execution and caching through `uf.config.js`; do not build a competing task engine. |
| Package manager manager | Provide coherent package-manager selection, version management, and invocation without forcing one package manager or conflating it with the JavaScript runtime. |
| Styling | Integrate StyleX by default, with replaceable styling support and performance-conscious build integration through `uf.config.js`. |
| Deployment | Provide portable application outputs and capability-based deployment adapters without requiring a particular provider or native application runtime dependency. |

The test runner must become a complete testing product, including reliable
isolation, async behavior, mocking, snapshots, watch workflows, and coverage
integration.

Measure startup, scheduling, transformation, host communication, and execution
together. Native orchestration alone is not proof of a fast test experience.

## A Complete Flow-Native Standard Library

Provide the libraries needed to build real frontend applications as a coherent
**`std`**, with strict types, strong inference, consistent conventions, and
minimal runtime overhead.

| Domain | Required scope |
| --- | --- |
| Application framework | A full-stack React framework with RSC, Server Actions, SSR, ISR, SSG, and routing. Target Next.js-class completeness with portable deployment and optional provider integrations. |
| Data and correctness | GraphQL, state management, validation, and an effect system designed together around end-to-end Flow types. |
| Immutable updates | An Immer-class immutable-update library implemented in Flow, with ergonomic draft-based updates, structural sharing, and strong inference. |
| Forms | A React Hook Form-class form library implemented in Flow, with strict React semantics, excellent inference, validation integration, and production-grade performance. |
| UI | Accessible headless UI primitives, a serious design-system component offering, and reusable React hook utilities. Use StyleX by default without coupling headless behavior to it. |
| Testing and component development | A Flow-native testing library and component story system integrated with the framework and toolchain. |
| Content and assets | MDX working by default, plus production-oriented media optimization. |

Treat EffectTS, Jotai, Valibot, Immer, React Hook Form, shadcn/ui, and VueUse as
concrete capability and usability benchmarks for their corresponding areas.

Do not settle for small lookalikes that lack the behavior, inference,
accessibility, or performance that makes those projects useful.

All-in-one distribution must not require all-or-nothing adoption. Applications
must be able to consume individual libraries without adopting the entire
framework or toolchain.

### Immutable Updates Without Observable Mutation

Provide the ergonomics of Immer-style updates while preserving immutable
application state.

Keep mutation confined to the draft-production boundary. Never mutate the base
state or expose live drafts as React state, props, context, or Hook return values.

Preserve structural sharing for unchanged data and avoid unnecessary whole-tree
cloning. Specify and test no-op updates, replacement results, nested updates,
supported collection types, and draft lifecycle behavior.

Design Flow types to preserve useful input and output inference while exposing
the intended distinction between drafts and published values. Enforce lifecycle
constraints at runtime where the type checker cannot prove them.

Integrate naturally with React state, reducers, and uf's state-management
library, without forcing applications to use draft-based updates everywhere.

Benchmark small updates to large structures, repeated updates, allocations, and
memory retention. Convenient syntax is not permission to introduce hidden
performance costs.

### Forms That Respect React

**Build a React Hook Form-class library without relying on loopholes in React's
execution model.**

Use React Hook Form as a capability and usability reference, not as a requirement
to reproduce its internal architecture.

Follow the Rules of React, render purity, immutable snapshot semantics, and the
Rules of Hooks. Compatibility with React Compiler and ordinary memoization is
a design requirement, not an optional integration.

Do not use stable-identity mutable objects, proxies, getters, or ref reads to make
render-visible values change behind React's snapshot semantics. Do not perform
externally observable mutation during render or depend on a render being
executed exactly once.

Internal mutation is not categorically forbidden. It must remain behind correct
boundaries and must not invalidate values already exposed to a render.

When integrating an external store, use React's supported subscription APIs,
including `useSyncExternalStore` where appropriate, with correctly cached,
immutable snapshots and consistent server snapshots.

Do not disable React Compiler, suppress legitimate Hook diagnostics, or require
users to avoid memoization merely to make the library work.

Provide strongly typed field paths and values, nested objects and arrays,
controlled and uncontrolled inputs, default values, reset behavior, dirty and
touched state, synchronous and asynchronous validation, submission state,
server errors, accessible error reporting, and appropriate focus management.

Integrate deeply with uf's Validator. Preserve the distinction between raw input
and validated or transformed output, including inference across submission and
Server Action boundaries.

Handle cancellation and stale asynchronous results explicitly. A slower
validation result must not overwrite newer state.

Achieve performance through narrow subscriptions, efficient data structures,
incremental validation, and reduced unnecessary work—not semantic shortcuts.

Test Strict Mode, interrupted rendering, Suspense, memoized consumers, React
Compiler, server rendering, hydration, and large dynamic forms.

## Use Modern Flow Fully

Make first-class use of **`match`, `component`, `hook`, and `renders`** wherever
they express the intended semantics.

Build APIs around Flow's strengths instead of mechanically translating
TypeScript patterns.

Preserve inference through composition. Model errors, state transitions,
component contracts, and server/client boundaries explicitly.

Avoid `any`, unchecked casts, and broad suppressions as shortcuts to a green
build. Keep necessary trust boundaries narrow and documented.

Type quality is part of the API. Test both what must compile and what must be
rejected, including inferred result types and invalid compositions.

Do not claim guarantees stronger than the checker and runtime actually enforce.

## Engineering and Production Standards

### Architecture and Documentation Comments

Keep responsibilities, data ownership, execution phases, and integration
contracts explicit. Prefer small, testable boundaries over global state and
layers of special cases.

Documentation comments must explain public contracts, invariants, failure modes,
and non-obvious tradeoffs—not merely paraphrase the implementation.

Record architectural decisions and compatibility assumptions in the repository
rather than leaving them in conversations.

Temporary workarounds need tests and a tracked path to a structural fix. Do not
let them become the permanent architecture by accident.

### Tests Are Required Evidence

Every bug fix needs a regression test that fails before the fix and passes
afterward. Preserve minimal reproductions and investigate adjacent cases, not
only the original symptom.

Test real applications and complete workflows, not just isolated functions.

Cover false positives and false negatives in diagnostics, formatter stability,
plugin execution across supported hosts, package installation, generated output,
styling, deployment adapters, and framework rendering and hydration.

For server-facing features, test authorization boundaries, input validation,
serialization, cache behavior, and error handling.

For UI and form libraries, include keyboard behavior, accessibility, React
semantic correctness, and interactions with the official React Compiler.

A feature that succeeds only on the happy path is not production-ready.

### Performance Is Non-Negotiable

**Performance is a defining product requirement, not a final optimization pass.**

Design for large repositories, substantial dependency graphs, repeated local
development, and sustained CI use.

Being fast on a small demonstration project is not sufficient. Treat avoidable
overhead in critical paths as engineering debt to eliminate, not as an acceptable
cost of an all-in-one experience.

Profile early and continuously. Track end-to-end latency, throughput, CPU usage,
peak memory, and scaling behavior as project size grows.

Benchmark cold starts, clean builds, warm-cache builds, no-op rebuilds,
incremental edits, watch responsiveness, styling pipelines, and large test suites
across supported hosts.

Use realistic applications alongside focused microbenchmarks.

Require reproducible before-and-after evidence for changes to
performance-critical paths. Record hardware, versions, workload, cache state,
and variance.

Establish explicit regression budgets and investigate reproducible regressions
before merging. Compare equivalent work, including required transforms, plugin
execution, correctness guarantees, and test isolation.

A Rust implementation or a faster isolated benchmark is not proof of a faster
product.

Hold Flow-native libraries to the same seriousness: measure runtime cost,
allocations, bundle size, and type-checking impact while preserving strict types
and strong inference.

Their JavaScript-only implementation requirement is not permission to accept
inefficient algorithms or unnecessary work.

Restore correctness before optimizing a broken path, then make the correct path
fast. Never obtain a benchmark win by weakening type guarantees, violating React
semantics, skipping required work, or silently changing behavior.

### Be Honest About Readiness

Keep **Implemented**, **Experimental**, and **Planned** capabilities distinct.

A stub, an exported API, or a passing toy example is not a completed feature.

Production readiness requires verified behavior, compatibility, diagnostics,
documentation, packaging, deployment, and a maintainable upgrade path.

Early alpha releases are a delivery mechanism, not an excuse to lower the
engineering standard.

## Build uf with uf

**uf must be a real user of its own toolchain and libraries.**

Build and maintain the project's Flow packages, application surfaces,
documentation site, examples, component stories, and integration fixtures with
uf itself.

Use uf's public commands and `uf.config.js` for the project's development,
checking, linting, formatting, testing, builds, and task orchestration. Integrate
release workflows through the same documented task system.

Use uf's framework, standard library, UI primitives, forms, state management,
validation, and styling defaults in real project surfaces where those
capabilities are needed.

Do not create artificial usage merely to check a box. Dogfooding must exercise
the same workflows and public APIs that external users depend on.

Keep the bootstrap path explicit and reproducible. Use a pinned, known-good uf
release or a minimal documented bootstrap to build the current toolchain, then
exercise the newly built version against the repository and representative
applications.

Rust compilation still belongs to Cargo and the Rust toolchain; uf should
orchestrate that work rather than pretending to replace them.

Document unavoidable bootstrap exceptions and track their removal or permanent
rationale. Do not maintain a hidden, more capable internal pipeline that bypasses
the product's limitations.

Make dogfooding failures visible in CI and treat regressions as product defects.
Do not bypass a broken public workflow just to keep the repository green.

Continue testing external projects and deployment targets. Self-use is required,
but it is not sufficient evidence of ecosystem compatibility.

## Documentation and Product Design

Build the documentation with React and `.js` with Flow, using uf, its framework,
its design system, and StyleX as the default styling integration.

Examples must demonstrate idiomatic Modern Flow and be checked or exercised in
CI. Provide real application guides as well as API documentation.

Document alternative runtimes, styling systems, bundlers, and deployment
targets where supported. Show how to self-host without proprietary services.

Develop the visual identity from a clear concept. Make deliberate choices about
typography, spacing, color, interaction, and information hierarchy.

Avoid generic AI-generated visual patterns and decorative complexity that does
not support the product.

The documentation, story system, and UI libraries must demonstrate the same
design quality that uf promises to application developers.

## Do Not Become Another Create React App

**All-in-one must never mean opaque, inflexible, or difficult to leave.**

Keep upstream integrations current, expose deliberate extension points, and
document escape hatches without requiring ejection.

Preserve independently usable libraries, understandable configuration,
replaceable defaults, portable deployment, and practical migration paths.

Do not hide architectural limitations behind defaults. The objective is to
remove repetitive setup, not remove the user's control or leave applications
dependent on one maintainer's private knowledge.

## Development, Issues, Pull Requests, and Releases

Use **GitHub Issues and `gh`** as the operational task system.

Search for existing reports before opening new ones. Record newly discovered
bugs, missing capabilities, performance bottlenecks, and design work as focused
issues with expected behavior, reproductions where applicable, and concrete
acceptance criteria.

Ensure **`p0`, `p1`, and `p2`** labels exist.

**ubugeeei assigns those priority labels; contributors and agents must not assign
them on his behalf.** Follow existing priorities and dependencies when selecting
work.

Work in small, reviewable pull requests linked to issues. Use clear Conventional
Commit-style scopes.

Include the implementation, regression coverage, documentation, and relevant
performance evidence together.

Enable auto-merge for eligible pull requests once required checks and repository
merge requirements are satisfied.

Never bypass protections, suppress failures, or weaken tests to maintain
throughput.

Release usable progress frequently. Use versioned **`v0.0.0-alpha.N`**
prereleases for the initial alpha series, with clear release notes and explicit
limitations.

Verify published packages and binaries, not just the source checkout. Exercise
installation and representative workflows from the actual release artifacts.

Do not defer releases until the entire vision is complete.

The normal loop is:

**discover → issue → implement → test → review → auto-merge → release → repeat**

Do not stop at scaffolding, contract definitions, or documentation-only progress
when implementation work remains.

When one task is blocked, record the blocker and continue independent work
without pretending the blocked capability is finished.

## Continuity

When ubugeeei is unavailable, continue from this guide, recorded decisions,
issue priorities, upstream contracts, tests, benchmarks, and real-user reports.

Routine engineering work should not require repeated permission, but uncertainty
must be made visible rather than replaced with invented intent.

Preserve operational knowledge in the repository. Record important edge cases,
minimal reproductions, compatibility requirements, release procedures, and the
reasoning behind non-obvious decisions.

The goal is sustained, verifiable progress toward a production-grade Flow and
React platform—not activity for its own sake, and not dependence on a single
person.

## Reference Points

### Project and Upstream Infrastructure

- [Vize continuity guide](https://github.com/ubugeeei-prod/vize/blob/main/ubugeeei-redundancy.md)
- [Flow source](https://github.com/facebook/flow)
- [Flow documentation](https://flow.org/)
- [React Compiler source](https://github.com/react/react/tree/main/compiler)
- [Vite JavaScript API](https://vite.dev/guide/api-javascript.html)
- [Vite+ configuration](https://viteplus.dev/config/)
- [Vite Task](https://viteplus.dev/guide/run)
- [Biome Formatter](https://biomejs.dev/formatter/)
- [Biome language coverage](https://biomejs.dev/internals/language-support/)
- [MDX documentation](https://mdxjs.com/docs/)

### React Semantics

- [Rules of React](https://react.dev/reference/rules)
- [Components and Hooks must be pure](https://react.dev/reference/rules/components-and-hooks-must-be-pure)
- [useSyncExternalStore](https://react.dev/reference/react/useSyncExternalStore)
- [React Compiler documentation](https://react.dev/learn/react-compiler)

### Styling, Immutable Updates, and Forms

- [StyleX documentation](https://stylexjs.com/)
- [StyleX installation and integrations](https://stylexjs.com/docs/learn/installation)
- [Immer introduction](https://immerjs.github.io/immer/)
- [Immer produce](https://immerjs.github.io/immer/produce/)
- [Immer performance](https://immerjs.github.io/immer/performance/)
- [React Hook Form](https://react-hook-form.com/)
- [React Hook Form useForm API](https://react-hook-form.com/docs/useform)
