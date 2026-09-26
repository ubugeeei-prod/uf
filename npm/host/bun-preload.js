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
// by `npm/host/flow-modules.test.js`; Bun asks it before it calls
// anything, and a dependency uf does not own goes to Bun's own loader having
// never touched this file.
//
// # The transform cache, shared with Node's loaders
//
// Every module this hook compiles is kept in `.uf/cache/transform/`, under the
// same key Node's loaders use and in the same bytes: development output with
// its source map appended inline (`./internal/flow-cache.js`). Until
// ubugeeei-prod/uf#944 this hook compiled every module on every run, so a
// `uf test` on Bun — and a suite `test.runner: "bun"` hands to `bun test` —
// started a `uf transform` and waited on it for every Flow module in the graph,
// every time, while the same run on Node read them back from disk.
//
// The bytes have to be the same, not merely equivalent. This hook used to ask
// for no source map, and an entry written that way under Node's key would be
// served to the next Node run as a module with no map — stack frames and code
// frames pointing at generated lines, silently, depending on which host ran
// first. Asking for the map here makes the two hosts' entries interchangeable:
// either host warms the cache for the other.

import {
  FLOW_MODULE_PATTERN,
  isFlowModule,
  sharedService,
  transformFlow,
  ufBinaryIdentity,
} from "./transform.js";
import {
  cacheDirectoryFor,
  cacheEntryFor,
  compileOptions,
  framed,
  readCached,
  writeCached,
} from "./internal/flow-cache.js";

const root = process.env.UF_PROJECT_ROOT ?? process.cwd();
const cacheDirectory = cacheDirectoryFor(root);

Bun.plugin({
  name: "uniflowed-flow",
  setup(build) {
    build.onLoad({ filter: FLOW_MODULE_PATTERN }, async (args) => {
      const file = filePath(args.path);
      const source = await Bun.file(file).text();

      // Both checks below are reachable only if the pattern and the two
      // `is_flow_module` implementations behind it ever disagree — the
      // pattern is equal to `isFlowModule`, and `transformFlow` answers
      // `null` for exactly what `uf_transform::is_flow_module` declines.
      // Handing the source back is the least wrong thing a hook with no way
      // to say "not mine" can do, and it is only *right* because a path the
      // pattern accepted is uf's own Flow source, which is an ES module
      // already. That it would be wrong for anything else is the reason the
      // agreement is a test rather than a comment.
      if (!isFlowModule(args.path)) return declined(file, source);

      // Read under the binary as it is now, written under the binary the
      // service is executing: "The two identities" in `./internal/flow-cache.js`.
      const cached = readCached(cacheEntryFor(cacheDirectory, ufBinaryIdentity(), source, file));
      if (cached != null) return { contents: cached, loader: "js" };

      const options = compileOptions(root);
      const out = await transformFlow(source, file, options);
      if (out == null) return declined(file, source);
      const output = framed(out);
      const service = sharedService(root, { configBootstrap: options.configBootstrap });
      writeCached(cacheEntryFor(cacheDirectory, service.identity, source, file), output);

      // `js` and not the file's own extension: the transform has already
      // turned the JSX into calls, and asking Bun to parse JSX in the output
      // would be asking it to parse code that no longer has any.
      return { contents: output, loader: "js" };
    });
  },
});

/** A module that reached the hook and should not have, given back as it was. */
function declined(path, source) {
  return { contents: source, loader: path.endsWith(".jsx") ? "jsx" : "js" };
}

/** The real file path behind a Bun module identity. */
function filePath(id) {
  const query = id.search(/[?#]/);
  return query === -1 ? id : id.slice(0, query);
}
