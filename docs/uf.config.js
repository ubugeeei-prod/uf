// @flow
import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  app: {
    runtime: {
      default: "node",
      capabilityJsHost: {
        default: "node",
        hosts: ["node", "deno", "bun"],
        autoDetect: true,
      },
    },
    builtins: {
      markdown: {
        module: "@uniflowed/markdown",
        engine: "ox-content-wasm",
        mdx: {
          enabled: true,
          extensions: [".mdx"],
          jsxImportSource: "@uniflowed/jsx-runtime",
          pipelinePlugin: "built-in",
        },
        cache: "opt-in",
      },
    },
    router: {
      entry: "app.js",
      root: "app",
    },
    rendering: {
      modes: ["ssg"],
      cache: {
        actions: false,
        data: false,
        fetch: false,
        route: false,
      },
    },
  },
  build: {
    entries: ["app.js"],
    outDir: "dist/docs",
    staticBuild: true,
  },
  docs: {
    enabled: true,
    app: "app.js",
    source: ".",
    outDir: "dist/docs",
    staticBuild: true,
    deploy: "void",
  },
  fmt: {
    flow: {
      parser: "official-flow-rust",
      printer: "uf-rust",
    },
    nonFlow: {
      formatter: "biome",
    },
  },
  lint: {
    engine: "rust",
    flow: {
      builtins: "mixed",
      parser: "official-flow-rust",
    },
  },
  test: {
    runner: {
      runtime: "capability-js-host",
      jsHosts: ["node", "deno", "bun"],
    },
  },
});
