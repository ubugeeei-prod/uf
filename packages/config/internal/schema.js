// @flow
//
// Owns the Flow shape of `uf.config.js`; `index.js` keeps the public package
// entry point thin.
//
// # Every key uf reads, and nothing else
//
// The guide says a config file "is type-checked as Flow code", which is the
// whole reason to write one in Flow. That makes an undeclared key worse than a
// missing feature: `lint.files`, `lint.ignore` and `app.router.enabled` were
// documented in the configuration reference, accepted by the runtime, and
// absent from this type — so a project following the reference had to choose
// between the documented key and a config that checks
// (ubugeeei-prod/uf#481). They were not the only three.
//
// So the key *names* below are no longer a person's job to keep in step with
// the loader. `crates/uf_config`'s `the_flow_schema_declares_every_key_uf_reads`
// parses this file with uf's own Flow parser and compares the paths it declares
// against the paths `uf_config::UniflowedConfig` serializes — in both
// directions, because a key declared here and read by nothing is an option that
// silently does nothing, which is the same defect wearing the other face.
//
// The value *types* are still this file's own judgement, and deliberately so.
// A test over names cannot say whether `"biome" | "prettier" | "none"` is the
// right set, and several keys below are narrower than what the loader will
// parse — `orm.module` is `"@uniflowed/orm"` because there is one
// implementation, where the loader takes any string. Where this package means
// to be more opinionated than the parser, that is what these say.

export type RuleLevel = "off" | "warn" | "error" | 0 | 1 | 2 | boolean;

export type TaskDefinition =
  | string
  | {
      readonly command: string,
      readonly cwd?: string,
      readonly dependsOn?: $ReadOnlyArray<string>,
      readonly env?: { readonly [string]: string },
      // Everything the task reads, as paths or globs from the project root; a
      // pattern beginning `!` excludes. This is the whole of the cache key, so
      // a task that lists nothing is never cached and always runs.
      readonly inputs?: $ReadOnlyArray<string>,
      // Everything it writes. Checked rather than restored: a replayed result
      // has to still have its files on disk, unchanged.
      readonly outputs?: $ReadOnlyArray<string>,
      // `false` keeps a task with declared inputs out of the cache.
      readonly cache?: boolean,
    };

export type CapabilityJsHost = "node" | "deno" | "bun";

/**
 * The package manager uf drives, overriding what it infers from the project.
 *
 * `"auto"` reads the project itself: an explicit `"packageManager"` field, then
 * a lockfile, then the nearest workspace root, then uf's own resolver.
 */
// The eight below are the *names a project may pin*, not commands this module
// runs: `@uniflowed/config` declares a type and executes nothing at all.
// `uniflowed/no-npm-script-invocation` is a line scanner, and a string whose
// entire contents is `pnpm` reads exactly like the `spawn("pnpm", […])` the
// rule exists to catch — `crates/uf_lint/src/scan/search.rs` says so in its own
// documentation and calls what is left "rare and suppressible". This is that
// residue, and there is no spelling of these values that is not one of them.
// uf-lint-disable uniflowed/no-npm-script-invocation
export type PackageManagerPreference =
  | "auto"
  | "uf"
  | "npm"
  | "pnpm"
  | "yarn"
  | "yarn-classic"
  | "yarn-berry"
  | "bun";
// uf-lint-enable uniflowed/no-npm-script-invocation

export type RuntimeEngine = "uf" | "node" | "deno" | "bun" | "edge" | "serverless" | "container";

export type DeployAdapter =
  | "node"
  | "bun"
  | "deno"
  | "edge"
  | "serverless"
  | "static"
  | "container";

/**
 * One entry of `plugins: [...]`.
 *
 * A bare name takes the default band and applies to every pipeline; the long
 * form says otherwise. Declaration order decides within a band, so the
 * resolved pipeline is a function of this file alone.
 */
export type PluginEntry =
  | string
  | {
      readonly name: string,
      readonly order?: "pre" | "normal" | "post",
      readonly apply?: "build" | "serve" | "always",
    };

/**
 * A ceiling `uf build` fails over, and what it is measured on.
 *
 * `max` accepts a byte count or a size a person would write — `"180kb"` —
 * because a budget is written by hand and read back by a report.
 */
export type SizeBudget = {
  readonly max: number | string,
  readonly metric?: "raw" | "gzip" | "brotli",
};

/**
 * What the project's own code may reach.
 *
 * Absent from `uf.config.js` means no permission model: the toolchain starts
 * its host the way it always has. Present means **deny by default** — the
 * project's code gets what is listed and nothing else — and `permissions: {}`
 * is a legitimate thing to write, meaning "nothing beyond what uf itself needs
 * to load and transform the project". There is deliberately no way to spell
 * "everything"; a project that wants everything does not declare a set.
 *
 * Every field is a list of literal strings, because `uf.config.js` is read as
 * text and parsed as JSON5 rather than executed: a computed path or a
 * `process.env` read here would not parse.
 *
 * The set is uf's, not a runtime's. Node.js enforces `read` and `write`, Deno
 * enforces all five, and Bun has no permission model at all — a host that
 * cannot enforce what is declared **refuses the run** rather than applying part
 * of it. `docs/hosts.md` is the table.
 *
 * The declared paths are *added to* the ones uf needs to run the project, so
 * what a set denies is the rest of the machine — `~/.ssh`, the network, the
 * environment — and not the project's own files.
 */
export type Permissions = {
  readonly read?: $ReadOnlyArray<string>,
  readonly write?: $ReadOnlyArray<string>,
  readonly net?: $ReadOnlyArray<string>,
  readonly env?: $ReadOnlyArray<string>,
  readonly run?: $ReadOnlyArray<string>,
};

/**
 * Percentages a coverage gate requires, as whole numbers between 0 and 100.
 *
 * A metric nobody names is not checked, which is not the same as requiring
 * zero: `{ lines: 0 }` says "every line, and I mean it" in a way that reads
 * wrong, so the absent case is the one that means nothing is required.
 */
export type CoverageThresholds = {
  readonly lines?: number,
  readonly functions?: number,
  readonly branches?: number,
};

export type UniflowedConfig = {
  /**
   * The runtime accessibility audit: `expect(el).toHaveNoAxeViolations()` in a
   * test, and the page `uf dev` is serving.
   *
   * One block for both on purpose. A rule a project has decided cannot be
   * judged here — `color-contrast` against a DOM with no layout is the
   * standing example — has to be the same rule in CI and in the loop somebody
   * is working in, or the audit that finds a violation while the component is
   * being written disagrees with the one that blocks the pull request.
   *
   * Inert without axe-core, which uf does not install: add it and both halves
   * start working.
   *
   * Not the same thing as `lint.rules`' `a11y/*`, which read JSX that was
   * never rendered.
   */
  readonly accessibility?: {
    readonly devAudit?: boolean,
    readonly axe?: {
      // Run only rules carrying one of these axe tags; every rule when absent.
      readonly tags?: $ReadOnlyArray<string>,
      readonly disabledRules?: $ReadOnlyArray<string>,
      readonly minImpact?: "minor" | "moderate" | "serious" | "critical",
    },
  },
  readonly app?: {
    // Whether a component with no directive is rendered on the server or
    // shipped to the browser.
    readonly componentDefault?: "server" | "client",
    readonly framework?: "uniflowed" | "react" | "react-native",
    readonly react?: {
      // React 19's Strict Mode, on by default in `uf dev`: it double-invokes
      // render and effects so an impurity shows up in development rather
      // than in production. Off has to be a choice a project makes.
      readonly strictMode?: boolean,
      readonly version?: string,
      readonly asyncReact?: boolean,
      readonly suspense?: boolean,
      readonly useHook?: boolean,
    },
    // Whether the project builds React Server Components at all, and whether
    // a `"use server"` export is wired to an endpoint.
    readonly rsc?: boolean,
    readonly serverActions?: boolean,
    // The runtimes the build must satisfy.
    readonly targets?: $ReadOnlyArray<"web" | "react-native" | "server" | "hermes">,
    readonly orm?: {
      readonly enabled?: boolean,
      readonly module?: "@uniflowed/orm",
      readonly native?: true,
      readonly generatedFlowTypes?: true,
      readonly preparedByDefault?: true,
    },
    readonly builtins?: {
      readonly data?: "uniflowed-query",
      readonly effect?: "uniflowed-effect",
      readonly fetch?: {
        readonly module?: "@uniflowed/fetch",
        readonly overrideGlobalFetch?: false,
      },
      readonly cell?: boolean,
      readonly frameworkLints?: boolean,
      readonly nativeTestRunner?: boolean,
      readonly reactTestingLibrary?: boolean,
      readonly relay?: boolean,
      readonly style?: "style-x",
      readonly reactCompiler?: {
        readonly enabled?: boolean,
        readonly implementation?: "official-rust",
        readonly mode?: "syntax",
      },
      readonly graphql?: {
        readonly module?: "@uniflowed/graphql",
        readonly relayBase?: true,
      },
      readonly loader?: {
        readonly module?: "@uniflowed/loader",
        readonly stateModule?: "@uniflowed/state",
        readonly cache?: "opt-in",
      },
      readonly markdown?: {
        readonly module?: "@uniflowed/markdown",
        readonly engine?: "ox-content-wasm",
        readonly mdx?: {
          readonly enabled?: boolean,
          readonly extensions?: $ReadOnlyArray<".mdx">,
          readonly jsxImportSource?: "@uniflowed/jsx-runtime",
          readonly pipelinePlugin?: "built-in",
          // Colours are computed during the build and written into the HTML,
          // so nothing ships to the browser to do it. Both themes are emitted
          // together as CSS variables, because a build cannot know which the
          // reader prefers.
          readonly highlight?: {
            readonly enabled?: boolean,
            readonly themes?: {
              readonly light?: string,
              readonly dark?: string,
            },
            // Grammars beyond the ones a uf project uses by default.
            readonly langs?: $ReadOnlyArray<string>,
          },
        },
        readonly cache?: "opt-in",
      },
      readonly images?: {
        readonly enabled?: boolean,
        // The widths a layout asks for. uf emits one variant per width that is
        // not wider than the source, plus one at the source's own width.
        readonly widths?: $ReadOnlyArray<number>,
        readonly quality?: number,
        readonly placeholder?: boolean,
      },
      readonly fonts?: {
        readonly enabled?: boolean,
        readonly display?: string,
        // The local face uf scales into a metric-matched fallback. One of the
        // faces `uf_assets::font::LOCAL_FACES` knows the metrics of, because
        // the scaling is a ratio against real numbers rather than a guess.
        readonly fallback?: string,
        // How an imported family is cut down. `"none"` is the default and the
        // argued position: subsetting is lossy, and a build cannot see the
        // text a server will render or a user will type. `"ranges"` splits the
        // font's own coverage into script buckets with exact `unicode-range`
        // values and loses nothing.
        readonly subset?: "none" | "ranges",
        // Whether `Font` preloads the primary face. Exactly one file is ever
        // preloaded, however many buckets a split produced.
        readonly preload?: boolean,
      },
      readonly icons?: {
        readonly enabled?: boolean,
        // Where `uf:icon/<name>` looks for `<name>.svg`, from the project
        // root. A directory rather than a claim on `.svg`, which stays Vite's.
        readonly dir?: string,
      },
      readonly og?: {
        readonly enabled?: boolean,
        // The typeface every `*.og.json` is drawn with unless it names its
        // own. uf embeds none, so a project that draws cards points at one.
        readonly font?: string | null,
      },
      readonly motion?: {
        readonly module?: "@uniflowed/motion",
        readonly engine?: "uf-native",
        readonly compilerSafe?: true,
        readonly serverComponentSafe?: true,
        readonly reducedMotionDefault?: true,
      },
      readonly tui?: {
        readonly module?: "@uniflowed/tui",
        readonly stdModule?: "@uniflowed/std/tui",
        readonly standard?: "open-tui",
        readonly nativeRenderer?: true,
        readonly beatReactInk?: true,
        readonly richMedia?: true,
        readonly inMemoryTests?: true,
      },
      readonly pwa?: {
        readonly module?: "@uniflowed/pwa",
        readonly enabledByDefault?: false,
        readonly cache?: "opt-in",
      },
      readonly temporal?: {
        readonly module?: "@uniflowed/temporal",
        readonly lite?: true,
      },
      readonly web?: {
        readonly module?: "@uniflowed/web",
        readonly typedRoutes?: true,
        readonly linkPrefetch?: "off" | "intent" | "render",
        readonly cache?: "opt-in",
      },
    },
    readonly runtime?: {
      readonly default?: RuntimeEngine,
      readonly compatibility?: $ReadOnlyArray<RuntimeEngine>,
      readonly capabilityJsHost?: {
        readonly default?: CapabilityJsHost,
        readonly hosts?: $ReadOnlyArray<CapabilityJsHost>,
        readonly autoDetect?: boolean,
      },
      readonly deploy?: {
        readonly enabled?: boolean,
        // The target `uf build` writes an artefact for when none is named on
        // the command line; `--adapter` beats it.
        readonly adapter?: DeployAdapter,
        readonly adapters?: $ReadOnlyArray<DeployAdapter>,
      },
    },
    readonly router?: {
      // `false` says this project is not a uf application, and is the only way
      // to say it: a library has no routes to scan for.
      readonly enabled?: boolean,
      readonly entry?: string,
      readonly root?: string,
      readonly manifest?: string,
      readonly convention?: "file-system",
      // Turning the file-system router off is what makes a project a
      // **library** rather than an application, and `uf build` reads it: see
      // `build.lib` below and docs/app/reference/config.
      readonly enabled?: boolean,
    },
    readonly rendering?: {
      // `"csr"` is the one value that cannot share the list: it renders every
      // route in the browser from one shell, where the others write a document
      // per route, so there is no per-route choice left for the list to hold.
      // `["csr"]` is a single-page application; see docs/app/guide/rendering.
      readonly modes?: $ReadOnlyArray<"ppr" | "ssr" | "ssg" | "isr" | "csr">,
      // What the browser does when a visitor follows a link, which is a
      // different question from `modes` rather than a fifth value in it:
      // `modes` says where a document comes from, per route, and this says
      // what happens once the browser has one. `"document"` is a full document
      // request — the client router is not installed and `Link` renders an
      // ordinary anchor. See docs/app/guide/routing/navigation.
      readonly navigation?: "client" | "document",
      readonly cache?: {
        readonly actions?: boolean,
        readonly data?: boolean,
        readonly fetch?: boolean,
        readonly route?: boolean,
        // Where cache entries live, and the only thing here that is a *name*
        // rather than a switch: `"memory"` (the default) is one process,
        // `"filesystem"` is uf's built-in durable provider, and anything else
        // is a module specifier exporting `createCacheProvider` — the same
        // shape `builder.module` has, and for the same red line. Turning this
        // on caches nothing new: a route still has to state a lifetime.
        readonly store?: string,
        // Where `"filesystem"` keeps them. Defaults to `.uf/cache/route` under
        // the project. A deployment that has one writable directory — `/tmp` on
        // a Lambda, a mounted volume in a container — names it here.
        readonly storeDir?: string,
      },
    },
  },
  readonly build?: {
    // Size ceilings that fail the build. Unset by default: failing a build
    // nobody asked uf to police is worse than reporting the size.
    readonly budgets?: {
      readonly total?: SizeBudget,
      readonly initialJs?: SizeBudget,
      readonly perRoute?: SizeBudget,
      readonly perAsset?: SizeBudget,
    },
    readonly entries?: $ReadOnlyArray<string>,
    // Commands to run around the build, in the same shape as `tasks`.
    readonly hooks?: { readonly [string]: TaskDefinition },
    readonly outDir?: string,
    // Prerender every route and leave no server bundle behind. Read together
    // with `app.rendering.modes`; see docs/app/reference/config.
    readonly staticBuild?: boolean,
    readonly sourcemap?: boolean,
    /**
     * What a **library** build writes, for a project whose
     * `app.router.enabled` is false.
     *
     * Never the switch. `app.router.enabled` decides which of the two builds
     * `uf build` runs, and declaring this key in a project whose router is on
     * is refused where the config is read rather than resolved by precedence.
     * Every field has a default that builds what `uf new --lib` scaffolds, so
     * a library ordinarily writes none of this.
     *
     * `"umd"` and `"iife"` are deliberately absent from `formats`: each needs
     * a global name per entry, and what a Flow library's global should be is
     * not a decision uf has made.
     */
    readonly lib?: {
      // The modules to build, relative to the project root. Each names its own
      // output by its path — `internal/parse.js` is written to
      // `dist/internal/parse.js` — so two entries cannot collide.
      readonly entries?: $ReadOnlyArray<string>,
      readonly formats?: $ReadOnlyArray<"es" | "cjs">,
      // Package names to leave as imports *beyond* the ones the manifest
      // declares. A library build already externalises `dependencies`,
      // `peerDependencies`, `optionalDependencies` and the host's built-in
      // modules; this is for what a manifest cannot say.
      readonly external?: $ReadOnlyArray<string>,
    },
  },
  // Which builder uf drives. Vite is the default, not a dependency: any module
  // satisfying the contract in docs/architecture.md can be named here.
  readonly builder?: {
    readonly module?: string,
  },
  readonly dev?: {
    readonly host?: string,
    readonly port?: number,
    readonly strictPort?: boolean,
    // Which files the dev server may serve. `deny` is evaluated on the
    // canonical path and beats `allow`; see docs/security.md.
    readonly fs?: {
      readonly allow?: $ReadOnlyArray<string>,
      readonly deny?: $ReadOnlyArray<string>,
    },
    // `--host` refuses to bind a routable address while this is empty: a dev
    // server reachable from the network with no host allow-list is a file
    // server for your source tree.
    readonly allowedHosts?: $ReadOnlyArray<string>,
    readonly allowedOrigins?: $ReadOnlyArray<string>,
  },
  readonly docs?: {
    readonly enabled?: boolean,
    readonly app?: string,
    readonly source?: string,
    readonly outDir?: string,
    readonly staticBuild?: boolean,
    readonly deploy?: "void",
  },
  /**
   * The `.env` cascade, the mode it is read for, and the pinned toolchain.
   *
   * `active` empty means the command decides — `development` for `uf dev`,
   * `production` for a build, `test` for `uf test`. `files` empty selects the
   * conventional cascade rather than no files at all.
   */
  readonly env?: {
    readonly active?: string,
    readonly files?: $ReadOnlyArray<string>,
    // Runtimes and package managers by exact version — `{ node: "24.14.0" }`.
    // Exact, because a range is not an environment.
    readonly toolchain?: { readonly [string]: string },
  },
  readonly fmt?: {
    readonly indentWidth?: number,
    readonly lineWidth?: number,
    readonly maxBlankLines?: number,
    readonly flow?: {
      readonly parser?: "official-flow-rust",
      readonly printer?: "uf-rust",
    },
    readonly nonFlow?: {
      readonly formatter?: "biome" | "prettier" | "none",
      /**
       * Extra arguments, passed to that formatter verbatim.
       *
       * Strings rather than a shape, so that reaching one of biome's or
       * prettier's own options never waits for a uf release — a Tailwind 4
       * project writes `["--css-parse-tailwind-directives=true"]` and needs no
       * second configuration file. An argument that would turn `uf fmt
       * --check` into a write is refused where the config is read.
       */
      readonly arguments?: $ReadOnlyArray<string>,
    },
    readonly quotes?: "single" | "double",
    readonly semicolons?: boolean,
  },
  /**
   * Paths no command walks into.
   *
   * Top level because every command that walks the project reads it: `uf fmt`,
   * `uf lint`, `uf check`, `uf test` and `uf doc`. A bare name — `dist` — names
   * a kind of directory and matches at any depth; a path — `src/generated` —
   * names one place. `.uf` and `.git` are uf's and git's and are not a
   * project's to opt back into.
   *
   * Absent takes uf's own list, `["node_modules", "dist", "target"]`. An empty
   * list is a different instruction: it is a project that has looked at that
   * list and wants none of it.
   */
  readonly ignore?: $ReadOnlyArray<string>,
  readonly lint?: {
    readonly engine?: "rust",
    // Globs to lint.
    readonly files?: $ReadOnlyArray<string>,
    /**
     * The old spelling of the top-level `ignore`.
     *
     * **Deprecated**, and read only when `ignore` is absent. It was never
     * `uf lint`'s alone: `uf fmt`, `uf check`, `uf test` and `uf doc` walk the
     * project through the same code and have always obeyed it, so the key was
     * named after one of the five commands that read it. It keeps working for
     * as long as alpha lasts, and every one of those commands says so once.
     * See ubugeeei-prod/uf#575.
     */
    readonly ignore?: $ReadOnlyArray<string>,
    readonly flow?: {
      readonly builtins?: "mixed",
      readonly parser?: "official-flow-rust",
    },
    // Changes to uf's rule table, not the whole of it: a rule you did not
    // mention keeps the level uf ships. Say `"off"` to switch one off.
    readonly rules?: { readonly [string]: RuleLevel },
  },
  readonly package?: {
    readonly generator?: "napi-rs",
    readonly targets?: $ReadOnlyArray<
      "node-napi" | "bun-napi" | "deno-napi" | "edge-wasm" | "serverless-napi",
    >,
    readonly typescriptDeclarationsToFlow?: true,
  },
  readonly permissions?: Permissions,
  // Plugins the project adds, appended to uf's own and resolved in the order
  // they are written. A name that names a file is code to run, so `uf_plugin`
  // refuses any that reaches outside the project root.
  readonly plugins?: $ReadOnlyArray<PluginEntry>,
  readonly pm?: {
    readonly module?: "@uniflowed/pm",
    readonly resolver?: "uf-native",
    readonly lockfile?: "uf.lock",
    readonly storeDir?: string,
    readonly allowLifecycleScripts?: false,
    readonly packageManager?: PackageManagerPreference,
    /**
     * The registry uf *reads* from: packuments, provenance attestations, and
     * the versions `uf update` reports against.
     *
     * Unset means `publish.registry`, which is where this lived until it
     * turned out to be answering two questions with one value. A project that
     * publishes to a company registry and installs through a read-through
     * mirror sets both; one that has only ever set `publish.registry` keeps
     * working and is told, once, which key to move to.
     */
    readonly registry?: string,
    /**
     * Which registry answers for which scope: `{ "@company": "https://…" }`.
     *
     * A scope named here resolves from that registry **and nowhere else**.
     * There is no fallback to the public registry, deliberately: publishing
     * `@company/internal-thing` to npmjs and waiting for a resolver to fall
     * back to it is the dependency-confusion attack, so the fallback is the
     * vulnerability rather than a recovery from it. A name the bound registry
     * does not have is an error that names the scope and the registry.
     *
     * uf refuses an install whose lockfile resolves a bound scope from
     * somewhere else. It does not rewrite the project's `.npmrc`: the manager
     * that resolves is the manager that has to be told, in its own
     * configuration.
     */
    readonly scopes?: { readonly [scope: string]: string },
    /**
     * How hard `uf install` looks at npm provenance attestations.
     *
     * `"report"`, the default, reads the attestation of every package the
     * install brought in or moved: an attestation that is not about the
     * tarball being installed stops the install, and a package with none is a
     * line in the summary. `"off"` reads none, for a machine with no route to
     * a registry.
     */
    readonly provenance?: "report" | "off",
  },
  readonly rm?: {
    readonly module?: "@uniflowed/rm",
    readonly inferFromConfig?: true,
    readonly version?: "node@system" | string,
    readonly autoSwitch?: boolean,
    readonly acquisition?: "auto",
    readonly apply?: "config-and-host",
    readonly doctor?: boolean,
  },
  readonly server?: {
    readonly engine?: "native-rust",
    readonly native?: {
      readonly streaming?: boolean,
      readonly zeroCopyHttp?: boolean,
      readonly adapters?: $ReadOnlyArray<
        "uf" | "node" | "bun" | "deno" | "edge" | "serverless" | "container",
      >,
    },
  },
  // Where the built site is served from, and what `uf build` may therefore
  // write for a crawler. `url` is the switch: without it no `sitemap.xml` and
  // no `robots.txt` are written at all, because a build cannot guess the host
  // it will be deployed to and a wrong `<loc>` is worse than a missing one.
  readonly site?: {
    readonly url?: string,
    readonly sitemap?: boolean,
    readonly robots?: {
      readonly enabled?: boolean,
      readonly allow?: $ReadOnlyArray<string>,
      readonly disallow?: $ReadOnlyArray<string>,
    },
  },
  readonly std?: {
    readonly module?: "@uniflowed/std",
    readonly wintertcAligned?: true,
    readonly nativeBindings?: boolean,
    readonly modules?: $ReadOnlyArray<
      | "vfs"
      | "fs"
      | "types"
      | "pipeline"
      | "effect"
      | "env"
      | "format"
      | "stdio"
      | "hash"
      | "debug"
      | "defs"
      | "lock"
      | "colors"
      | "qs"
      | "equality"
      | "http"
      | "buffer"
      | "ws"
      | "sql"
      | "json"
      | "yaml"
      | "toml"
      | "collections"
      | "crypto"
      | "dotenv"
      | "math"
      | "os"
      | "net"
      | "dns"
      | "path"
      | "stream"
      | "url"
      | "wasm"
      | "glob"
      | "motion"
      | "tui"
      | "cron"
      | "s3"
      | "sigv4"
      | "functions"
      | "uuid"
      | "zip"
      | "import-meta"
      | "defer",
    >,
  },
  readonly publish?: {
    readonly registry?: string,
    readonly dryRun?: boolean,
    readonly firstPublish?: {
      readonly mode?: "local",
      readonly localBootstrap?: true,
    },
    readonly trustedPublish?: {
      readonly enabled?: true,
      readonly provider?: "github-actions-oidc",
      readonly tokenless?: true,
      readonly trigger?: "tag-push",
    },
  },
  readonly release?: {
    readonly tagPrefix?: "uf@",
    readonly command?: "uf release alpha" | string,
    readonly publish?: true,
  },
  readonly story?: {
    readonly enabled?: boolean,
    readonly module?: "@uniflowed/story",
    readonly mocks?: {
      readonly module?: "@uniflowed/mock",
      readonly mswCompatible?: boolean,
    },
    readonly browser?: {
      readonly module?: "@uniflowed/browser",
      readonly playwrightCompatible?: boolean,
    },
  },
  readonly taskRunner?: {
    readonly engine?: "vite-task",
    readonly allowPackageScripts?: false,
  },
  readonly test?: {
    readonly module?: "@uniflowed/test",
    readonly runner?: {
      readonly runtime?: "vite-task" | "capability-js-host" | "uf-self-hosted",
      readonly jsHosts?: $ReadOnlyArray<CapabilityJsHost>,
      readonly scheduler?: "vite-task-cache" | "native-work-stealing",
      readonly performanceTarget?: "vite-task" | "faster-than-bun",
      readonly officialFlowParser?: true,
    },
    readonly reactTestingLibraryNative?: true,
    /**
     * What `uf test --coverage` measures, writes and fails on.
     *
     * The thresholds live here rather than on the command line because a
     * coverage gate is a property of the project: the number CI fails on has to
     * be the number a laptop fails on. They are checked whenever coverage was
     * collected, so `enabled: false` plus `--coverage` still gates.
     */
    readonly coverage?: {
      readonly enabled?: boolean,
      readonly directory?: string,
      readonly reporters?: $ReadOnlyArray<"text" | "lcov" | "cobertura">,
      readonly include?: $ReadOnlyArray<string>,
      readonly exclude?: $ReadOnlyArray<string>,
      readonly thresholds?: CoverageThresholds,
      readonly perFileThresholds?: CoverageThresholds,
    },
  },
  readonly tasks?: { readonly [string]: TaskDefinition },
  /**
   * Vite's own configuration, merged over the one uf generates.
   *
   * uf reads none of it, which is the point: an option Vite adds tomorrow
   * works in a uf project tomorrow rather than after a uf release that names
   * it. Deliberately unshaped for the same reason — a Flow type over Vite's
   * options would be the re-declaration this key exists to avoid.
   */
  readonly vite?: { readonly [string]: mixed },
  readonly vrt?: {
    readonly enabled?: boolean,
    readonly module?: "@uniflowed/vrt",
    readonly baselines?: string,
    readonly threshold?: number,
  },
};

/**
 * Identity function that pins `uf.config.js` to `UniflowedConfig`.
 *
 * This is the one binding in the package that is not a native call: a config
 * module evaluates `defineConfig({...})` at its top level, so raising here would
 * make every config file unloadable. Its whole job is to give Flow a type to
 * check the literal against.
 */
export function defineConfig(config: UniflowedConfig): UniflowedConfig {
  return config;
}
