// @flow
import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  ignore: ["node_modules", "dist", "backend"],
  app: {
    router: { entry: "app.js", root: "app" },
    rendering: { modes: ["ssr"] },
    builtins: { style: "style-x" },
  },
  build: { staticBuild: false, outDir: "dist" },
  tasks: {
    relay: { command: "node tools/relay.mjs" },
    backend: { command: "go run .", cwd: "backend" },
  },
});
