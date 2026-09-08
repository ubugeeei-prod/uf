// @noflow
//
// Plain JavaScript: this file registers the loader, so it cannot need one.
//
// `bun --preload @uniflowed/host/bun-preload app.js` runs a Flow project on
// Bun without a build step, through Bun's own plugin API: every module uf is
// responsible for is transformed by `uf transform` as Bun loads it. It is the
// Bun counterpart of `./register.js`, and the policy of which files count is
// the same `isFlowModule`.
//
// # Why the filter carries the policy rather than the hook
//
// Node's hooks may defer a module to the next loader. Bun's may not: a module
// reaches `onLoad` because the filter selected it, and from there the hook
// owes Bun an object. Returning `undefined` — which this file did for every
// module it was not responsible for — is
//
//     TypeError: onLoad() expects an object returned
//
// and it took the process down on the first ordinary `.js` dependency a
// project had, which is to say on every Bun project (ubugeeei-prod/uf#418). Returning the
// file's own bytes instead is not the fix either: anything that leaves
// `onLoad` is an ES module to Bun, so a CommonJS dependency handed back
// unchanged loses its default export.
//
// So the module is never selected in the first place. `FLOW_MODULE_PATTERN`
// is `isFlowModule` as a pattern, equal to it path for path and pinned to it
// by `tests/library/flow-modules.test.js`; Bun asks it before it calls
// anything, and a dependency uf does not own goes to Bun's own loader having
// never touched this file.

import { FLOW_MODULE_PATTERN, inSourceTests, isFlowModule, transformFlow } from "./transform.js";

Bun.plugin({
  name: "uniflowed-flow",
  setup(build) {
    build.onLoad({ filter: FLOW_MODULE_PATTERN }, async (args) => {
      const source = await Bun.file(args.path).text();

      // Both checks below are reachable only if the pattern and the two
      // `is_flow_module` implementations behind it ever disagree — the
      // pattern is equal to `isFlowModule`, and `transformFlow` answers
      // `null` for exactly what `uf_transform::is_flow_module` declines.
      // Handing the source back is the least wrong thing a hook with no way
      // to say "not mine" can do, and it is only *right* because a path the
      // pattern accepted is uf's own Flow source, which is an ES module
      // already. That it would be wrong for anything else is the reason the
      // agreement is a test rather than a comment.
      if (!isFlowModule(args.path)) return declined(args.path, source);
      const out = await transformFlow(source, args.path, {
        development: true,
        sourceMap: false,
        inSourceTests: inSourceTests(),
      });
      if (out == null) return declined(args.path, source);

      // `js` and not the file's own extension: the transform has already
      // turned the JSX into calls, and asking Bun to parse JSX in the output
      // would be asking it to parse code that no longer has any.
      return { contents: out.code, loader: "js" };
    });
  },
});

/** A module that reached the hook and should not have, given back as it was. */
function declined(path, source) {
  return { contents: source, loader: path.endsWith(".jsx") ? "jsx" : "js" };
}
