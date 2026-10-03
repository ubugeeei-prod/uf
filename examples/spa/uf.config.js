// @flow
import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  app: {
    builtins : { style: "style-x" },
    router   : { entry: "app.js", root: "app" },
    rendering: { modes: ["csr"] },
  },
  build: { entries: ["app.js"], outDir: "dist" },
});
