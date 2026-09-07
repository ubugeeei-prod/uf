// @flow
//
// Owns the Flow shape of `uf.config.js`; `index.js` keeps the public package
// entry point thin.

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
  readonly app?: {
    readonly orm?: {
      readonly enabled?: boolean,
      readonly module?: "@uniflowed/orm",
      readonly native?: true,
      readonly generatedFlowTypes?: true,
      readonly preparedByDefault?: true,
    },
    readonly builtins?: {
      readonly fetch?: {
        readonly module?: "@uniflowed/fetch",
        readonly overrideGlobalFetch?: false,
      },
      readonly cell?: boolean,
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
      readonly default?: "node" | "deno" | "bun" | "uf",
      readonly compatibility?: $ReadOnlyArray<
        "node" | "bun" | "deno" | "edge" | "serverless" | "container",
      >,
      readonly capabilityJsHost?: {
        readonly default?: CapabilityJsHost,
        readonly hosts?: $ReadOnlyArray<CapabilityJsHost>,
        readonly autoDetect?: boolean,
      },
      readonly deploy?: {
        readonly enabled?: boolean,
        readonly adapters?: $ReadOnlyArray<
          "node" | "bun" | "deno" | "edge" | "serverless" | "static" | "container",
        >,
      },
    },
    readonly router?: {
      readonly entry?: string,
      readonly root?: string,
      readonly manifest?: string,
    },
    readonly rendering?: {
      readonly modes?: $ReadOnlyArray<"ppr" | "ssr" | "ssg" | "isr">,
      readonly cache?: {
        readonly actions?: boolean,
        readonly data?: boolean,
        readonly fetch?: boolean,
        readonly route?: boolean,
      },
    },
  },
  readonly build?: {
    readonly entries?: $ReadOnlyArray<string>,
    readonly outDir?: string,
    readonly staticBuild?: boolean,
    readonly sourcemap?: boolean,
  },
  readonly dev?: {
    readonly host?: string,
    readonly port?: number,
    readonly strictPort?: boolean,
  },
  readonly docs?: {
    readonly enabled?: boolean,
    readonly app?: string,
    readonly source?: string,
    readonly outDir?: string,
    readonly staticBuild?: boolean,
    readonly deploy?: "void",
  },
  readonly lint?: {
    readonly engine?: "rust",
    readonly flow?: {
      readonly builtins?: "mixed",
      readonly parser?: "official-flow-rust",
    },
    readonly rules?: { readonly [string]: RuleLevel },
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
  readonly package?: {
    readonly generator?: "napi-rs",
    readonly targets?: $ReadOnlyArray<
      "node-napi" | "bun-napi" | "deno-napi" | "edge-wasm" | "serverless-napi",
    >,
    readonly typescriptDeclarationsToFlow?: true,
  },
  readonly pm?: {
    readonly module?: "@uniflowed/pm",
    readonly resolver?: "uf-native",
    readonly lockfile?: "uf.lock",
    readonly storeDir?: string,
    readonly allowLifecycleScripts?: false,
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
      readonly runtime?: "capability-js-host" | "uf-self-hosted",
      readonly jsHosts?: $ReadOnlyArray<CapabilityJsHost>,
      readonly scheduler?: "native-work-stealing",
      readonly performanceTarget?: "faster-than-bun",
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
