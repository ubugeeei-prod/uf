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
  // Where the manual is served from. `uf build` writes `sitemap.xml` and
  // `robots.txt` because this is here, and would write neither without it: a
  // `<loc>` has to be an absolute URL and nothing in a bundle knows the host.
  //
  // Nothing is disallowed. Every page of the manual is meant to be found, so
  // what makes the `robots.txt` worth writing is its `Sitemap:` line — the way
  // a crawler that was handed nothing else finds the thirty URLs.
  //
  // The same origin appears as `metadataBase` in `docs/app/_uf.layout.js`,
  // which is what the renderer resolves `/brand/uf.png` against. Two readers,
  // two places; keep them in step.
  site: {
    url: "https://docs.uniflowed.dev",
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
