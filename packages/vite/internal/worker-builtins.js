// @noflow
//
// Plain JavaScript: `../driver.js` imports this before any transform exists.
//
// The Node built-ins a Cloudflare Worker does not provide at the compatibility
// date `uf build --adapter edge` writes, and what the build says about a module
// that reaches one.
//
// # Where the list comes from
//
// A measurement rather than a summary of the documentation. Each module below
// was imported statically into a Worker — so Wrangler applied the polyfills it
// applies to uf's own bundle — served by `wrangler dev --local` at compatibility
// date 2024-09-23 with `nodejs_compat`, and the function beside it was called.
// Every one threw `[unenv] <function> is not implemented yet!`.
// `tools/ci/edge-worker-smoke.sh` builds that probe from this table on every run
// and fails when a module here stops throwing or stops importing, so the list
// cannot go on claiming something workerd no longer does.
//
// Whole modules only. `net` answers `connect` and refuses `createServer`, and
// `dgram`, `inspector` and `worker_threads` construct objects without throwing;
// a module that partly works is not one a build can name as absent. A call that
// fails in one of those fails at request time, where `@uniflowed/server/edge`
// names the function in the log line.
//
// # Reported, not refused
//
// The build goes on. The date is the one uf writes into `wrangler.json`, and a
// project that deploys with a `wrangler.json` of its own at a newer date may
// have the module — `node:fs` gained a virtual filesystem at a later one. A
// refusal here would stop a deployment uf cannot see from being built at all,
// so what uf owes is the sentence: which module, which file reached it, and what
// a request that reaches it answers on uf's date.

/** The compatibility date the table was measured at; `wrangler.json` pins it. */
export const WORKERS_COMPATIBILITY_DATE = "2024-09-23";

/**
 * Every module a Worker provides only as a stub at that date, with the call
 * that shows it.
 *
 * `construct` is for the one entry whose representative member is a class.
 * The smoke builds `new wasi.WASI(...)` from it rather than a call.
 */
export const UNAVAILABLE_ON_WORKERS = Object.freeze([
  Object.freeze({ module: "child_process", member: "execFileSync", args: ["true"] }),
  Object.freeze({ module: "cluster", member: "fork", args: [] }),
  Object.freeze({ module: "fs", member: "readFileSync", args: ["/tmp/uf-probe"] }),
  Object.freeze({ module: "http", member: "createServer", args: [] }),
  Object.freeze({ module: "http2", member: "createServer", args: [] }),
  Object.freeze({ module: "https", member: "createServer", args: [] }),
  Object.freeze({ module: "repl", member: "start", args: [] }),
  Object.freeze({
    module: "wasi",
    member: "WASI",
    construct: true,
    args: [{ version: "preview1" }],
  }),
]);

/** How many reached modules one build names before it counts the rest. */
export const MAX_NAMED = 8;

/**
 * The table entry `specifier` reaches, or `null` when it reaches none.
 *
 * `node:fs`, `fs` and `node:fs/promises` are all `fs`: the prefix is optional
 * in an import and a subpath is the same module's functions under another name.
 */
export function unavailableOnWorkers(specifier) {
  if (typeof specifier !== "string") return null;
  const bare = specifier.startsWith("node:") ? specifier.slice("node:".length) : specifier;
  const name = bare.split("/")[0];
  return UNAVAILABLE_ON_WORKERS.find((entry) => entry.module === name) ?? null;
}

/**
 * The reached imports that are still in the output once the bundler has shaken
 * the graph.
 *
 * Resolution sees every import of every module the graph names, including
 * modules that contribute nothing to the bundle. Measured on the
 * `rsc-split-app` fixture: a client component's top-level
 * `import { expect, it } from "@uniflowed/test"`, kept for an in-source test
 * that compiles to `void 0` in a build, reached `node:fs` and
 * `node:child_process` through `@uniflowed/test` and `@uniflowed/host` — and not
 * one byte of either was in the Worker. A warning about a module the Worker does
 * not contain is a warning every edge project would learn to ignore.
 *
 * So an import counts only when a chunk of the output still imports that
 * module *and* the importing module rendered code into that chunk. `chunks` is
 * the bundle's chunk entries, as `generateBundle` receives them: `imports` is
 * the chunk's external imports, `modules` its modules with `renderedLength`.
 */
export function survivingImports(reached, chunks) {
  return reached.filter(({ specifier, importer }) => {
    const entry = unavailableOnWorkers(specifier);
    if (entry == null) return false;
    return chunks.some(
      (chunk) =>
        (chunk.imports ?? []).some((name) => unavailableOnWorkers(name)?.module === entry.module) &&
        (chunk.modules?.[importer]?.renderedLength ?? 0) > 0,
    );
  });
}

/**
 * The warnings one edge build prints, one line per module and importer.
 *
 * `reached` is `{ specifier, importer }` in the order the bundler met them,
 * `root` the project; an importer inside the project and outside
 * `node_modules` is the project's own code and is named before any
 * dependency, because it is the one a reader can change. Bounded by
 * [`MAX_NAMED`], with the rest counted, so a dependency that reaches a stub
 * from a hundred files cannot print a hundred lines.
 */
export function workerBuiltinWarnings(reached, root) {
  const seen = new Set();
  const lines = [];
  for (const { specifier, importer } of reached) {
    const entry = unavailableOnWorkers(specifier);
    if (entry == null || importer == null) continue;
    const key = `${entry.module}\0${importer}`;
    if (seen.has(key)) continue;
    seen.add(key);
    const inside = isProjectModule(importer, root);
    const from = inside ? relative(importer, root) : dependencyName(importer);
    lines.push({
      inside,
      text:
        `${from} imports node:${entry.module}, which a Cloudflare Worker at compatibility date ` +
        `${WORKERS_COMPATIBILITY_DATE} provides only as a stub: every function of it throws. ` +
        (inside
          ? "A request that reaches it will answer 500 on this target."
          : "The dependency may never call it here; if a request does, it answers 500."),
    });
  }
  lines.sort((a, b) => Number(b.inside) - Number(a.inside));
  const named = lines.slice(0, MAX_NAMED).map((line) => `--adapter edge: ${line.text}`);
  if (lines.length > MAX_NAMED) {
    named.push(
      `--adapter edge: and ${lines.length - MAX_NAMED} more importers of the same modules`,
    );
  }
  return named;
}

function isProjectModule(importer, root) {
  const normalised = importer.split("\\").join("/");
  const base = root.split("\\").join("/").replace(/\/$/, "");
  return normalised.startsWith(`${base}/`) && !normalised.includes("/node_modules/");
}

function relative(importer, root) {
  const base = root.split("\\").join("/").replace(/\/$/, "");
  return importer
    .split("\\")
    .join("/")
    .slice(base.length + 1);
}

/** `a dependency (name)`, from the last `node_modules/` segment of a path. */
function dependencyName(importer) {
  const normalised = importer.split("\\").join("/");
  const at = normalised.lastIndexOf("/node_modules/");
  if (at === -1) return `a module outside the project (${normalised})`;
  const rest = normalised.slice(at + "/node_modules/".length).split("/");
  const name = rest[0]?.startsWith("@") ? `${rest[0]}/${rest[1]}` : rest[0];
  return `the dependency ${name}`;
}
