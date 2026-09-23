// @flow
import { defineConfig } from "@uniflowed/config";

// Nothing about partial prerendering is configured here, and that is the
// point: the default `app.rendering.modes` allows `ppr`, routes render as
// Server Components by default, and every server `uf start`, `uf preview` and
// a streaming adapter leave behind can fill a hole. `app/account/$page.js`
// reading a cookie inside a `<Suspense>` boundary is what makes it one.
export default defineConfig({
  app: {
    router: { entry: "app.js", root: "app" },
  },
  build: {
    entries: ["app.js"],
    outDir: "dist",
  },
});
