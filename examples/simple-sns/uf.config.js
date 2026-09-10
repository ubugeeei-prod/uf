// @flow
import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  app: {
    builtins: {
      reactCompiler: {
        enabled: true,
        implementation: "official-rust",
        mode: "syntax",
      },
      style: "style-x",
    },
    router: { entry: "app.js", root: "app" },
    rendering: { modes: ["ssr"] },
  },
  build: {
    entries: ["app.js"],
    outDir: "dist",
    staticBuild: false,
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
      jsHosts: ["node"],
    },
  },
});
