// @flow
import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  app: {
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
});
