// @flow
import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  app: {
    router: { entry: "app.js", root: "app" },
    rendering: { modes: ["csr"] },
  },
  build: { entries: ["app.js"], outDir: "dist" },
});
