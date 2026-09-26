// @flow
//
// The deploy-matrix fixture as a static export.
//
// Only what a file host can serve: the prerendered pages, their RSC payloads
// and the client bundle. `modes: ["ssg"]` makes a page that would need a server
// fail the build rather than disappear from it, and there are no `app.router`
// rules because nothing would apply them. The matrix removes every route that
// needs a server from the copy this is written into, after asserting that
// `--adapter static` refuses the whole application by name.

import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  app: {
    router: {
      entry: "app.js",
      root: "app",
    },
    rendering: {
      modes: ["ssg"],
    },
  },
  build: {
    entries: ["app.js"],
    outDir: "dist",
  },
});
