// @noflow
"use strict";
// Reuse the app's RN Babel preset for upstream native modules and Jest mock
// hoisting. App Flow syntax and StyleX use the same uf compiler as Metro.
const { createRequire } = require("node:module");
const path = require("node:path");
const { isFlowModule, transformFlowSync } = require("@uniflowed/host/transform");
module.exports = {
  process(source, filename, options) {
    const root = options.config.rootDir;
    const load = createRequire(path.join(root, "package.json"));
    const babel = load("babel-jest").createTransformer({
      babelrc: false,
      configFile: false,
      presets: [load.resolve("@react-native/babel-preset")],
      plugins: [load.resolve("@babel/plugin-transform-dynamic-import")],
    });
    const transformed = isFlowModule(filename)
      ? transformFlowSync(source, filename, {
          root,
          development: true,
          nativeStyles: true,
          sourceMap: true,
        })
      : null;
    return babel.process(transformed?.code ?? source, filename, options);
  },
  // Include the compiler version/mtime, just as Metro does, on rebuilt tools.
  getCacheKey(source, filename, options) {
    const crypto = require("node:crypto");
    const { ufBinaryIdentity } = require("@uniflowed/host/transform");
    return crypto
      .createHash("sha256")
      .update(source)
      .update(filename)
      .update(options.configString)
      .update(String(ufBinaryIdentity(process.env.UF_BINARY || "uf")))
      .update(require("node:fs").readFileSync(__filename))
      .digest("hex");
  },
};
