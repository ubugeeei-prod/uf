// @flow
import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  app: {
    router: {
      entry: "app.js",
      root: "app",
      // Served under `/docs`, and every page spelled without a trailing slash.
      // `uf dev`, `uf preview`, `uf start` and the adapters are all asked the
      // same questions about both.
      basePath: "/docs",
      trailingSlash: "never",
    },
  },
  build: {
    entries: ["app.js"],
    outDir: "dist",
  },
  // What makes `uf build` write `sitemap.xml`, so a test can read which
  // addresses the build names: under the base path, in the policy's spelling.
  // The host is fictional and never resolved.
  site: {
    url: "https://based.example",
  },
});
