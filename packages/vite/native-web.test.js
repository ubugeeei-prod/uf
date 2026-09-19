// @flow

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, expect, it } from "@uniflowed/test";
import { nativeWebPlugin } from "./internal/native-web.js";

describe("shared React Native pages", () => {
  it("resolves the app's own web implementation and refuses a missing peer by name", () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "uf-native-web-"));
    try {
      const plugin = nativeWebPlugin("web");
      plugin.configResolved({ root });
      expect(() => plugin.resolveId("react-native")).toThrow("uf add react-native-web");
      const peer = path.join(root, "node_modules/react-native-web");
      fs.mkdirSync(peer, { recursive: true });
      fs.writeFileSync(path.join(peer, "package.json"), JSON.stringify({ main: "index.js" }));
      fs.writeFileSync(path.join(peer, "index.js"), "export const View = 'div';\n");
      expect(plugin.resolveId("react-native")).toBe(fs.realpathSync(path.join(peer, "index.js")));
      expect(plugin.resolveId("react-native/private")).toBe(null);
      expect(nativeWebPlugin("ios").resolveId("react-native")).toBe(null);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });
});
