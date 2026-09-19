// @flow
import { defineConfig } from "@uniflowed/config";
export default defineConfig({
  app: { targets: ["react-native"], rsc: false, router: { root: "app", entry: "index.js" } },
  test: { target: "react-native", runtime: "node" },
  tasks: { ios: "expo start --ios", android: "expo start --android" },
});
