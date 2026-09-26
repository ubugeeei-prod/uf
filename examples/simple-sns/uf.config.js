// @flow

import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  app: {
    builtins: {
      reactCompiler: {
        enabled: true,
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
    nonFlow: {
      formatter: "biome",
    },
  },
});
