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
});
