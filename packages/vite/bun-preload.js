// Plain JavaScript: this file registers the loader, so it cannot need one.
//
// `bun --preload @uniflowed/vite/bun-preload app.js` runs a Flow project on
// Bun without a build step, through Bun's own plugin API: every module uf is
// responsible for is transformed by `uf transform` as Bun loads it. It is the
// Bun counterpart of `./register.js`, and the policy of which files count is
// the same `isFlowModule`.

import { isFlowModule, transformFlow } from "./transform.js";

Bun.plugin({
  name: "uniflowed-flow",
  setup(build) {
    build.onLoad({ filter: /\.(js|jsx|mjs)$/ }, async (args) => {
      if (!isFlowModule(args.path)) return undefined;
      const source = await Bun.file(args.path).text();
      const out = await transformFlow(source, args.path, { development: true, sourceMap: false });
      if (out == null) return undefined;
      return { contents: out.code, loader: "js" };
    });
  },
});
