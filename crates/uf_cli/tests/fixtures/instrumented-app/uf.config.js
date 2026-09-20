// @flow
import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  app: {
    router: { entry: "app.js", root: "app" },
    rendering: { modes: ["ssr"] },
  },
  build: { staticBuild: false, outDir: "dist" },
});
