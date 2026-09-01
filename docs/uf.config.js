// @flow
import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  app: {
    router: {
      entry: "docs/app.js",
      root: "docs/app",
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
    entries: ["docs/app.js"],
    outDir: "dist/docs",
    staticBuild: true,
  },
  docs: {
    enabled: true,
    app: "docs/app.js",
    source: "docs",
    outDir: "dist/docs",
    staticBuild: true,
    deploy: "void",
  },
});
