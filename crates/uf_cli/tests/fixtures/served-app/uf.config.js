// @flow
import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  app: {
    router: { entry: "app.js", root: "app" },
  },
  build: {
    entries: ["app.js"],
    outDir: "dist",
  },
  // What makes `uf build` write `sitemap.xml` and `robots.txt` here. The host
  // is fictional and never resolved: what the tests read is which URLs the
  // build decided it could name, and this fixture has the interesting ones —
  // three parameterised routes nothing can enumerate, and a not-found boundary
  // whose document is served and is not a page.
  site: {
    url: "https://served.example",
  },
});
