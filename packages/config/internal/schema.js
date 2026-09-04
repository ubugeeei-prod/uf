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
    };

export type CapabilityJsHost = "node" | "deno" | "bun";

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
      readonly formatter?: "biome",
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
