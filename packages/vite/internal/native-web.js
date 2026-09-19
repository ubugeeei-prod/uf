// @noflow

import { createRequire } from "node:module";
import path from "node:path";

/** React Native Web is supplied by the application, including its version. */
export function nativeWebPlugin(target) {
  let root = process.cwd();
  return {
    name: "uf:react-native-web",
    enforce: "pre",
    configResolved(config) {
      root = config.root;
    },
    resolveId(source) {
      if (target !== "web" || source !== "react-native") return null;
      try {
        return createRequire(path.join(root, "package.json")).resolve("react-native-web");
      } catch {
        throw new Error(
          "A shared React Native page needs the application's react-native-web dependency: run `uf add react-native-web`.",
        );
      }
    },
  };
}
