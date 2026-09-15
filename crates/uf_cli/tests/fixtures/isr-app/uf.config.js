// @flow
import { defineConfig } from "@uniflowed/config";

// Regeneration takes two things from this file and one from the page. The
// default `app.rendering.modes` allows `isr`, and `rendering.cache.route` turns
// the route cache on; `app/clock/$page.js` states the lifetime. No store is
// named, so each target keeps regenerated pages where it can by default: a disk
// under `uf start`, `uf preview` and a process adapter, Workers KV on the edge.
export default defineConfig({
  app: {
    router: { entry: "app.js", root: "app" },
    rendering: {
      cache: { route: true },
    },
  },
  build: {
    entries: ["app.js"],
    outDir: "dist",
  },
});
