// @flow
//
// Owns the Flow shape of `uf.config.js`; `index.js` keeps the public package
// entry point thin.

export type RuleLevel = "off" | "warn" | "error" | 0 | 1 | 2 | boolean;

export type TaskDefinition =
  | string
  | {
      +command: string,
      +cwd?: string,
      +dependsOn?: $ReadOnlyArray<string>,
      +env?: { +[string]: string },
    };

export type CapabilityJsHost = "node" | "deno" | "bun";

export type UniflowedConfig = {
  +app?: {
    +orm?: {
      +enabled?: boolean,
      +module?: "@uniflowed/orm",
      +native?: true,
      +generatedFlowTypes?: true,
      +preparedByDefault?: true,
    },
    +builtins?: {
      +fetch?: {
        +module?: "@uniflowed/fetch",
        +overrideGlobalFetch?: false,
      },
      +cell?: boolean,
      +graphql?: {
        +module?: "@uniflowed/graphql",
        +relayBase?: true,
      },
      +loader?: {
        +module?: "@uniflowed/loader",
        +stateModule?: "@uniflowed/state",
        +cache?: "opt-in",
      },
      +markdown?: {
        +module?: "@uniflowed/markdown",
        +engine?: "ox-content-wasm",
        +mdx?: {
          +enabled?: boolean,
          +extensions?: $ReadOnlyArray<".mdx">,
          +jsxImportSource?: "@uniflowed/jsx-runtime",
          +pipelinePlugin?: "built-in",
        },
        +cache?: "opt-in",
      },
      +motion?: {
        +module?: "@uniflowed/motion",
        +engine?: "uf-native",
        +compilerSafe?: true,
        +serverComponentSafe?: true,
        +reducedMotionDefault?: true,
      },
      +tui?: {
        +module?: "@uniflowed/tui",
        +stdModule?: "@uniflowed/std/tui",
        +standard?: "open-tui",
        +nativeRenderer?: true,
        +beatReactInk?: true,
        +richMedia?: true,
        +inMemoryTests?: true,
      },
      +pwa?: {
        +module?: "@uniflowed/pwa",
        +enabledByDefault?: false,
        +cache?: "opt-in",
      },
      +temporal?: {
        +module?: "@uniflowed/temporal",
        +lite?: true,
      },
      +web?: {
        +module?: "@uniflowed/web",
        +typedRoutes?: true,
        +linkPrefetch?: "off" | "intent" | "render",
        +cache?: "opt-in",
      },
    },
    +runtime?: {
      +default?: "node" | "deno" | "bun" | "uf",
      +compatibility?: $ReadOnlyArray<
        "node" | "bun" | "deno" | "edge" | "serverless" | "container",
      >,
      +capabilityJsHost?: {
        +default?: CapabilityJsHost,
        +hosts?: $ReadOnlyArray<CapabilityJsHost>,
        +autoDetect?: boolean,
      },
      +deploy?: {
        +enabled?: boolean,
        +adapters?: $ReadOnlyArray<
          "node" | "bun" | "deno" | "edge" | "serverless" | "static" | "container",
        >,
      },
    },
    +router?: {
      +entry?: string,
      +root?: string,
      +manifest?: string,
    },
    +rendering?: {
      +modes?: $ReadOnlyArray<"ppr" | "ssr" | "ssg" | "isr">,
      +cache?: {
        +actions?: boolean,
        +data?: boolean,
        +fetch?: boolean,
        +route?: boolean,
      },
    },
  },
  +build?: {
    +entries?: $ReadOnlyArray<string>,
    +outDir?: string,
    +staticBuild?: boolean,
    +sourcemap?: boolean,
  },
  +dev?: {
    +host?: string,
    +port?: number,
    +strictPort?: boolean,
  },
  +docs?: {
    +enabled?: boolean,
    +app?: string,
    +source?: string,
    +outDir?: string,
    +staticBuild?: boolean,
    +deploy?: "void",
  },
  +lint?: {
    +engine?: "rust",
    +flow?: {
      +builtins?: "mixed",
      +parser?: "official-flow-rust",
    },
    +rules?: { +[string]: RuleLevel },
  },
  +fmt?: {
    +indentWidth?: number,
    +lineWidth?: number,
    +maxBlankLines?: number,
    +flow?: {
      +parser?: "official-flow-rust",
      +printer?: "uf-rust",
    },
    +nonFlow?: {
      +formatter?: "biome",
    },
    +quotes?: "single" | "double",
    +semicolons?: boolean,
  },
  +package?: {
    +generator?: "napi-rs",
    +targets?: $ReadOnlyArray<
      "node-napi" | "bun-napi" | "deno-napi" | "edge-wasm" | "serverless-napi",
    >,
    +typescriptDeclarationsToFlow?: true,
  },
  +pm?: {
    +module?: "@uniflowed/pm",
    +resolver?: "uf-native",
    +lockfile?: "uf.lock",
    +storeDir?: string,
    +allowLifecycleScripts?: false,
  },
  +rm?: {
    +module?: "@uniflowed/rm",
    +inferFromConfig?: true,
    +version?: "node@system" | string,
    +autoSwitch?: boolean,
    +acquisition?: "auto",
    +apply?: "config-and-host",
    +doctor?: boolean,
  },
  +server?: {
    +engine?: "native-rust",
    +native?: {
      +streaming?: boolean,
      +zeroCopyHttp?: boolean,
      +adapters?: $ReadOnlyArray<
        "uf" | "node" | "bun" | "deno" | "edge" | "serverless" | "container",
      >,
    },
  },
  +std?: {
    +module?: "@uniflowed/std",
    +wintertcAligned?: true,
    +nativeBindings?: boolean,
    +modules?: $ReadOnlyArray<
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
  +publish?: {
    +registry?: string,
    +dryRun?: boolean,
    +firstPublish?: {
      +mode?: "local",
      +localBootstrap?: true,
    },
    +trustedPublish?: {
      +enabled?: true,
      +provider?: "github-actions-oidc",
      +tokenless?: true,
      +trigger?: "tag-push",
    },
  },
  +release?: {
    +tagPrefix?: "uf@",
    +command?: "uf release alpha" | string,
    +publish?: true,
  },
  +story?: {
    +enabled?: boolean,
    +module?: "@uniflowed/story",
    +mocks?: {
      +module?: "@uniflowed/mock",
      +mswCompatible?: boolean,
    },
    +browser?: {
      +module?: "@uniflowed/browser",
      +playwrightCompatible?: boolean,
    },
  },
  +taskRunner?: {
    +engine?: "uf-task",
    +allowPackageScripts?: false,
  },
  +test?: {
    +module?: "@uniflowed/test",
    +runner?: {
      +runtime?: "capability-js-host" | "uf-self-hosted",
      +jsHosts?: $ReadOnlyArray<CapabilityJsHost>,
      +scheduler?: "native-work-stealing",
      +performanceTarget?: "faster-than-bun",
      +officialFlowParser?: true,
    },
    +reactTestingLibraryNative?: true,
  },
  +tasks?: { +[string]: TaskDefinition },
  +vrt?: {
    +enabled?: boolean,
    +module?: "@uniflowed/vrt",
    +baselines?: string,
    +threshold?: number,
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
