// @noflow
//
// Plain JavaScript: executed by the host that runs Vite, before any transform
// exists — this module is how the transform is reached, so it cannot be Flow.
//
// The Flow → JavaScript transform lives in `uf` itself (`crates/uf_transform`:
// the official Flow parser, Flow's own lowering rules, the official React
// Compiler, oxc for JSX and code generation). This module is the JavaScript
// side of the `uf transform` service: one long-lived `uf` process per host
// process, newline-delimited JSON in, replies in request order out.
//
// Every host that runs Flow — the Vite plugin, the Node loader hook, the Bun
// preload, the config loader — goes through here, which is what makes them
// all produce the same module from the same source.

import { spawn, spawnSync } from "node:child_process";
import { accessSync, constants, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { createInterface } from "node:readline";

/** File extensions uf treats as Flow source. */
export const FLOW_EXTENSIONS = [".js", ".jsx", ".mjs", ".cjs"];

/**
 * Whether uf is responsible for transforming this module.
 *
 * Mirrors `uf_transform::is_flow_module`, and must keep mirroring it: a `uf
 * dev` session and a `uf test` run that disagree about which files are Flow
 * disagree about what the code is.
 *
 * A build tool synthesises modules of its own — ids beginning with a NUL byte,
 * a bundler's shims — and a third-party dependency ships JavaScript that is
 * already JavaScript; neither is Flow. `@uniflowed/*` under `node_modules` is
 * the deliberate exception: those packages ship Flow source, because that is
 * what uf tells everyone to write.
 *
 * Which build tool is deliberately not named. This loader runs Flow on a
 * Capability JS Host and has no bundler in it; naming one would tie the answer
 * to a tool that is not in this file's dependency graph.
 */
export function isFlowModule(id) {
  if (id.startsWith("\0")) return false;
  const clean = normalizePathSeparators(stripQuery(id));
  if (!FLOW_EXTENSIONS.some((extension) => clean.endsWith(extension))) return false;
  const at = clean.lastIndexOf("/node_modules/");
  return at === -1 || clean.slice(at).startsWith("/node_modules/@uniflowed/");
}

/**
 * Whether `filename` is JavaScript uf itself compiled and wrote under `.uf/`:
 * the compiled `uf.config.*` the config loader wrote, or anything `uf build`
 * wrote under `.uf/build/` — the rsc graph, the server bundle, the compiled
 * application.
 *
 * `isFlowModule` answers "is this path uf's to transform?" from the path's
 * shape, and a bundle under the project root has the shape of project source.
 * It is not source. It is the transform's own output, already through the Flow
 * chain and the React Compiler, and handing it back meant compiling it twice:
 * `uf build`'s server pass re-transformed the whole rsc bundle, and the
 * prerender's `import()` of the server bundle re-transformed that — hundreds of
 * kilobytes each, on every build, in series, and with the React Compiler run a
 * second time over components it had already compiled.
 *
 * Deliberately a second question rather than a clause in `isFlowModule`: that
 * function's answer is mirrored by `FLOW_MODULE_PATTERN` and by
 * `uf_transform::is_flow_module`, and "already compiled" is a fact about where
 * uf writes, which only a caller that is about to compile needs.
 *
 * @param {string} filename an absolute path, or a module id with a query
 * @returns {boolean}
 */
export function isCompiledOutput(filename) {
  const clean = normalizePathSeparators(stripQuery(filename));
  return clean.includes("/.uf/build/") || clean.includes("/.uf/config/uf.config.");
}

function normalizePathSeparators(id) {
  return typeof process !== "undefined" && process.platform === "win32"
    ? id.replaceAll("\\", "/")
    : id;
}

function stripQuery(id) {
  const at = id.search(/[?#]/);
  return at === -1 ? id : id.slice(0, at);
}

/**
 * `isFlowModule`'s answer for a plain file path, as a pattern.
 *
 * # Why a second expression of one policy exists at all
 *
 * Node's loader hooks are asked about every module and may hand one back
 * untouched, so `./internal/node-hooks.js` calls `isFlowModule` and defers
 * what it does not claim. Bun's plugin API has no such move: a module reaches
 * `onLoad` because a *pattern* selected it, and once it is there the hook has
 * to answer with contents. There is no shape that means "not mine" —
 * `undefined`, `null`, `{}` and `{ loader }` are all rejected with
 * `TypeError: onLoad() expects an object returned`, which is why no Bun
 * project could load `./bun-preload.js` at all (ubugeeei-prod/uf#418).
 *
 * Handing the file's own bytes back is not the fix it looks like. A module
 * that leaves `onLoad` is an ES module in Bun's eyes whatever its contents
 * say, so a CommonJS dependency returned unchanged stops having a default
 * export: `import dep from "dep"` becomes
 * `SyntaxError: Missing 'default' export`. Declining correctly is impossible
 * from inside the hook, so the decision has to be made before it — which
 * means the policy has to exist as something a pattern can express.
 *
 * # The contract, which is the only reason this is safe
 *
 * **For every path with no query string and no leading NUL,
 * `FLOW_MODULE_PATTERN.test(path) === isFlowModule(path)`.** Not "is a
 * conservative approximation of": exactly equal, including the case a
 * conservative approximation would get wrong — `@uniflowed/*` nested inside
 * another package's `node_modules`, which the lookahead's inner
 * `(?!.*\/node_modules\/)` is there to keep, and which a simpler pattern that
 * rejected any path containing `node_modules` would silently stop
 * transforming.
 *
 * `npm/host/flow-modules.test.js` is that sentence as a test, over a
 * table that includes every case either expression could get wrong on its own.
 * Two spellings of one rule are a drift risk and the test is the thing that
 * makes them not one; the extension list is shared rather than repeated for
 * the same reason.
 *
 * The NUL-prefixed exclusion is outside the contract because it cannot arrive
 * here: it is a bundler's synthetic module rather than a path a host asks its
 * filesystem about. A query string can arrive on Bun when module mocking gives
 * a path import an epoch identity, so the pattern accepts query-carrying file
 * paths while still deciding from the path itself.
 */
const separator =
  typeof process !== "undefined" && process.platform === "win32" ? String.raw`[\\/]` : "/";
export const FLOW_MODULE_PATTERN = new RegExp(
  // Reject when the *last* `/node_modules/` on the path is not followed by
  // `@uniflowed/`, which is `isFlowModule`'s `lastIndexOf` written as a
  // lookahead: the inner negative lookahead is what pins "last".
  String.raw`^(?![^?#]*${separator}node_modules${separator}(?![^?#]*${separator}node_modules${separator})(?!@uniflowed${separator}))` +
    String.raw`[^?#]*\.(?:${FLOW_EXTENSIONS.map((extension) => extension.slice(1)).join("|")})` +
    String.raw`(?:[?#].*)?$`,
);

/**
 * Every `@uniflowed/*` package the project can resolve.
 *
 * Vite's dependency optimizer takes package names, not predicates. A Vite app
 * that owns its config can use this list as `optimizeDeps.exclude`, matching
 * the same `@uniflowed/*` exception `isFlowModule` makes for Flow source in
 * `node_modules`.
 */
export function uniflowedPackages(root) {
  const names = new Set();
  let directory = root;
  for (;;) {
    const scope = path.join(directory, "node_modules", "@uniflowed");
    try {
      for (const entry of readdirSync(scope)) names.add(`@uniflowed/${entry}`);
    } catch {
      // no packages at this level
    }
    const parent = path.dirname(directory);
    if (parent === directory) break;
    directory = parent;
  }
  return [...names].sort();
}

/**
 * The `uf` binary to talk to.
 *
 * `uf dev`, `uf build` and `uf test` set `UF_BINARY` to themselves when they
 * start a host, so the host reaches exactly the binary that started it. A host
 * started by hand finds `uf` on PATH, which is what the installer arranges.
 */
export function ufBinary() {
  return environmentVariable("UF_BINARY") ?? "uf";
}

/**
 * Whether this host is compiling for a run that collects in-source tests.
 *
 * `uf test` sets `UF_IN_SOURCE_TESTS` on every worker it starts, and nothing
 * else sets it. The flag decides what `import.meta.uf.test` compiles to — uf's
 * test API here, `void 0` everywhere else — which is why it has to reach the
 * transform rather than only the runtime: a block that survives into a
 * production bundle is worse than no in-source tests at all.
 *
 * Read per call rather than captured once, because the loader hooks are
 * installed before `uf` has told the process anything.
 */
export function inSourceTests() {
  return environmentVariable("UF_IN_SOURCE_TESTS") === "1";
}

/**
 * One environment variable, or `undefined` when it is unset *or* this process
 * may not read it.
 *
 * Deno denies by default, and a worker `uf test` starts there is granted the
 * variables uf set on it and nothing else. Reading any other throws
 * `NotCapable` rather than answering `undefined` — measured on Deno 2.9 — and
 * every variable this package reads on the way to a module is one it only
 * consults: a loader that took the process down over one would be failing a
 * suite over nothing. On Node and Bun this is `process.env[name]` and no more.
 */
export function environmentVariable(name) {
  try {
    return process.env[name];
  } catch {
    return undefined;
  }
}

/**
 * The names a `uf transform` child inherits when the whole environment may not
 * be read.
 *
 * `PATH`, because `uf transform` may start a host of its own to evaluate
 * `uf.config.js`; `HOME` and `TMPDIR`, which the platform's own libraries read;
 * and uf's three, which are what the transform is about.
 */
const SANDBOXED_TRANSFORM_ENVIRONMENT = [
  "PATH",
  "HOME",
  "TMPDIR",
  "UF_BINARY",
  "UF_PROJECT_ROOT",
  "UF_IN_SOURCE_TESTS",
];

/**
 * The environment a `uf transform` child starts with: this process's.
 *
 * All of it where the host lets it be enumerated, which is every host that does
 * not sandbox the environment. Deno does, and spreading `process.env` there is
 * a request for every variable at once, refused as `NotCapable` — measured on
 * Deno 2.9, from the transform thread's first compile. So a refusal falls back
 * to the names above, each read on its own and skipped when that one is
 * withheld too. The child is `uf`, not the project's code, and a transform
 * needs nothing a test's sandbox keeps from it.
 */
function inheritedEnvironment() {
  try {
    return { ...process.env };
  } catch {
    const env = {};
    for (const name of SANDBOXED_TRANSFORM_ENVIRONMENT) {
      const value = environmentVariable(name);
      if (value != null) env[name] = value;
    }
    return env;
  }
}

/**
 * Which *build* of `uf` a host will transform through, or `null` when that
 * cannot be established.
 *
 * `ufBinary()` names the compiler; this identifies it. Anything kept across
 * runs needs the second, because the first does not change when the compiler
 * does: `crates/uf_transform` is edited, `cargo build` writes a new binary
 * over the old one, and every answer already on disk is now wrong while the
 * name that produced them is unchanged. A version string is the same promise
 * one step removed — every build between two releases shares one.
 *
 * So: the size and modification time of the file that will be executed. They
 * move together on every rebuild, they are one `stat` away, and — this is the
 * part that decided it — reading them does not require starting `uf`. A run
 * that finds everything already compiled must not have to spawn the compiler
 * to learn that it does not need it, which is what asking the running
 * `uf transform` to introduce itself would have cost.
 *
 * The same test is applied to a path as to a bare name: a regular file with
 * the execute bit. Size and mtime do not move when a binary loses that bit, so
 * without the test a chmod produced the same identity as before, a warm cache
 * went on serving, and a cold one failed to start `uf` — the answer depending
 * on how warm the cache was, which is the class of bug this key exists to
 * remove.
 *
 * `null` means the question could not be answered. It is not an invitation to
 * hash the rest anyway: a key that leaves the compiler out is one key for
 * every build of it, which is the whole defect.
 *
 * @param {string} [command] the binary; `ufBinary()` by default
 * @returns {string | null} an opaque identity, stable while that build is
 */
export function ufBinaryIdentity(command = ufBinary()) {
  const binary = resolveExecutable(command);
  if (binary == null) return null;
  try {
    const stats = statSync(binary);
    if (!stats.isFile() || !isExecutable(binary, stats)) return null;
    // Whole milliseconds, because hosts disagree below that: Node reports the
    // filesystem's nanosecond timestamp as a fraction and Deno reports whole
    // milliseconds. Node's and Deno's loaders share one cache
    // (`./internal/flow-cache.js`), and two spellings of one binary's
    // identity would be two keys for every module either host compiled. A
    // rebuild that lands in the same millisecond at the same size is not a
    // rebuild anybody runs.
    return `${binary}\0${stats.size}\0${Math.trunc(stats.mtimeMs)}`;
  } catch {
    // Named a binary that is not there, or is not one. The caller gets `null`
    // and stops trusting the cache, which is right: nothing can be compiled
    // either.
    return null;
  }
}

/**
 * Whether `file`, whose `stats` the caller already holds, may be executed.
 *
 * `access(2)` with `X_OK`, which asks the kernel on behalf of this process —
 * the question `spawn` is about to ask. Deno will not answer it inside its
 * permission model without `--allow-sys=uid`, because the answer depends on
 * who is asking, and a worker `uf test` starts on Deno is granted no `sys` at
 * all. Measured on Deno 2.9: the call throws `NotCapable`, and treating that as
 * "not executable" left every Deno run with no identity, so its loader never
 * read or wrote the cache.
 *
 * So a refusal *by the sandbox* is answered from the mode bits `stat` already
 * returned — executable by somebody. That is a looser test than `access`, and
 * loose in the direction that fails loudly: a binary that has an execute bit
 * and still may not be run by this user is one `spawn` cannot start, which is an
 * error, not a stale answer. Anything else `access` throws is still "no".
 */
function isExecutable(file, stats) {
  try {
    accessSync(file, constants.X_OK);
    return true;
  } catch (error) {
    return error?.name === "NotCapable" && (stats.mode & 0o111) !== 0;
  }
}

/**
 * The file `spawn` will execute for `command`, or `null` when there is none.
 *
 * A bare name is searched along PATH the way `execvp` searches for it — the
 * first regular, executable file wins — so that the identity above describes
 * the binary that actually runs rather than some other `uf` further down the
 * list. Getting this wrong is not a slow cache but a silently stale one, which
 * is why a directory named `uf` is skipped here as `execvp` skips it, rather
 * than being accepted because `access` says a directory is executable.
 *
 * Windows resolves a bare name by rules of its own — `PATHEXT`, the current
 * directory — which this does not implement. There a bare name is `null` and
 * the caller falls back to not caching, rather than to caching under the
 * identity of a file that may not be the one that ran. `UF_BINARY`, which is
 * how every uf-started host arrives here, is an absolute path on every
 * platform and never takes this path at all.
 */
function resolveExecutable(command) {
  // A path is taken as given — `spawn` will execute exactly it — and
  // `ufBinaryIdentity` applies the file-and-executable test to the result
  // either way, so a path that is a directory or is not executable is no more
  // trusted than a bare name that resolves to one.
  if (path.basename(command) !== command) return command;
  for (const directory of (environmentVariable("PATH") ?? "").split(path.delimiter)) {
    if (directory === "") continue;
    const candidate = path.join(directory, command);
    try {
      const stats = statSync(candidate);
      if (stats.isFile() && isExecutable(candidate, stats)) return candidate;
    } catch {
      // Not in this directory. Keep looking, exactly as the shell would.
    }
  }
  return null;
}

/**
 * An error the transform reported for one module, with its position when
 * the parser or the lowering rules gave one.
 */
export class TransformError extends Error {
  constructor(id, message, line, column) {
    super(message);
    this.name = "TransformError";
    this.id = id;
    this.loc = line != null ? { file: id, line, column: column ?? 0 } : undefined;
  }
}

/**
 * One `uf transform` process, with requests answered in the order they were
 * sent.
 *
 * `uf transform` replies once per request, in order, so a plain queue of
 * resolvers pairs a reply with its caller — no correlation ids and no map to
 * leak. Any exit is final: a request made after the process has gone is
 * rejected at once rather than queued against something that will never
 * answer.
 *
 * # Why the child is unreferenced between requests
 *
 * A live child process and its pipes are handles, and a host with a handle
 * open does not exit. Nothing closes the process-wide service — the loader
 * hooks and the Bun preload both take it and neither has an "afterwards" to
 * close it in — so on Bun `bun --preload @uniflowed/host/bun-preload app.js`
 * ran the program, printed its output, and then sat there forever. Node hides
 * this: its loaders keep the service on a thread of their own — the loader
 * thread `register()` starts, or the transform thread
 * `./internal/sync-hooks.js` starts on a cache miss — and the process exits
 * with the main thread whatever that thread is still holding. That accident is
 * the only reason it was ever invisible, and it is not something the second
 * host can be asked to reproduce.
 *
 * So the service holds its host open for exactly as long as it owes an
 * answer: referenced when a request joins an empty queue, unreferenced when
 * the queue drains, and unreferenced from the start. Unreferencing
 * unconditionally would be the other bug — the host would be free to exit
 * during a transform, and `uf build` would end in the middle of a module with
 * no error anywhere.
 */
export class TransformService {
  #child;
  #pending = [];
  #identity;
  #failure = null;
  /**
   * Whether the host is currently held open for this service.
   *
   * `true` before the constructor's first release, because that is what a
   * freshly spawned child and its pipes are: referenced. Starting it `false`
   * would make that release a no-op and leave the service holding the host
   * from the moment it was made, which is the bug this field exists to end
   * rather than a second spelling of it.
   */
  #held = true;

  /**
   * @param {object} [options]
   * @param {string} [options.command] the `uf` binary; `ufBinary()` by default
   * @param {string} [options.root] project root, so `uf.config.js` is found
   * @param {boolean} [options.configBootstrap] transform code that is needed
   *   before `uf.config.js` can be evaluated
   */
  constructor(options = {}) {
    const command = options.command ?? ufBinary();
    const root = options.root ?? process.cwd();
    // Read before the spawn and kept: this is the identity of the build that
    // answers every request this service ever serves, because a child goes on
    // executing the binary it started from however many times that file is
    // rewritten underneath it. Anything written to disk from an answer of
    // this service belongs under *this* identity — a caller that stat'd the
    // binary earlier and wrote under that would file build B's output under
    // build A's name, which is the original defect with a smaller window.
    this.#identity = ufBinaryIdentity(command);
    const env = inheritedEnvironment();
    if (options.configBootstrap === true) {
      env.UF_TRANSFORM_BOOTSTRAP_CONFIG = "1";
    } else {
      delete env.UF_TRANSFORM_BOOTSTRAP_CONFIG;
    }
    this.#child = spawn(command, ["--cwd", root, "transform"], {
      stdio: ["pipe", "pipe", "inherit"],
      env,
    });

    createInterface({ input: this.#child.stdout }).on("line", (line) => {
      const waiting = this.#pending.shift();
      if (!waiting) return;
      // `finally`, so the hold is released down every path out of this
      // handler and not only the successful one. A rejected request is still a
      // request that has been answered, and staying referenced after one would
      // turn a module that failed to compile into a process that never exits.
      try {
        let reply;
        try {
          reply = JSON.parse(line);
        } catch {
          waiting.reject(new Error(`uf transform sent a malformed reply: ${line}`));
          return;
        }
        if (reply.error != null) {
          waiting.reject(new TransformError(waiting.id, reply.error, reply.line, reply.column));
          return;
        }
        waiting.resolve(reply);
      } finally {
        this.#holdHost(this.#pending.length > 0);
      }
    });

    this.#child.on("error", (error) => {
      this.#settleAll(new Error(`could not run \`${command} transform\`: ${error.message}`));
    });
    this.#child.on("close", (code) => {
      this.#settleAll(new Error(`uf transform exited (${code})`));
    });

    this.#holdHost(false);
  }

  /**
   * Keep the host process alive, or stop keeping it alive.
   *
   * The pipes as well as the child: each is a handle of its own, and a host
   * that unreferenced only the child would still be held open by the stdout it
   * is reading replies from. Every call is optional-chained because this runs
   * on whatever host started it, and a runtime that has no notion of
   * referenced handles has nothing to do here — it is not an error, it is a
   * runtime whose loop already ends when the work does.
   *
   * # Why the current state is tracked rather than set every time
   *
   * On Node `ref()` and `unref()` set a flag, so calling either twice is the
   * same as calling it once and a caller may say what it wants as often as it
   * likes. **On Bun they count**, and that difference is a hang: two modules
   * imported at the same time are two `transform` calls, which is two `ref`s,
   * and the first reply to arrive leaves one request outstanding and so asks
   * to hold again — three `ref`s against the one `unref` the drain performs.
   * The host then never exits, which is ubugeeei-prod/uf#418's symptom
   * returning by another route: `bun --preload @uniflowed/host/bun-preload`
   * ran a program of three or more Flow modules, printed its output and sat
   * there. Two modules were sequential, so it looked fine; anything that
   * imports `@uniflowed/test` was not.
   *
   * Tracking the state and acting only on the edge is one `ref` per held
   * period and one `unref` per release on both runtimes, which is what a
   * counting implementation needs and what a flag implementation cannot tell
   * apart.
   */
  #holdHost(hold) {
    if (hold === this.#held) return;
    this.#held = hold;
    const method = hold ? "ref" : "unref";
    this.#child[method]?.();
    this.#child.stdin?.[method]?.();
    this.#child.stdout?.[method]?.();
  }

  #settleAll(error) {
    this.#failure = error;
    while (this.#pending.length > 0) this.#pending.shift().reject(error);
    this.#holdHost(false);
  }

  /**
   * Transform one module.
   *
   * Resolves to `{ code, map, css, diagnostics }`, or to `null` when the
   * module is not uf's to transform (see `isFlowModule`). Rejects with a
   * `TransformError` carrying the position when the source is not valid Flow.
   *
   * `css` is the stylesheet the module's StyleX rules declare, and `null` when
   * it declares none.
   *
   * @param {string} id absolute path, used for the map and for errors
   * @param {string} code the Flow source
   * @param {object} [options]
   * @param {boolean} [options.development] readable output, `jsxDEV`
   * @param {boolean} [options.refresh] Fast Refresh registrations (development only)
   * @param {boolean} [options.sourceMap] produce a source map; on by default
   */
  transform(id, code, options = {}) {
    if (this.#failure) return Promise.reject(this.#failure);
    // Before the push, so the host is held from the moment it is owed an
    // answer rather than from the moment the write lands.
    this.#holdHost(true);
    return new Promise((resolve, reject) => {
      this.#pending.push({
        id,
        reject,
        resolve: (reply) => {
          if (reply.code == null) {
            resolve(null);
            return;
          }
          // Named field by field rather than passed through, so a host reads
          // the protocol rather than whatever `uf transform` happens to send —
          // which means every field the protocol grows has to be added here,
          // and one was not. `css` arrived with the StyleX compiler and this
          // object did not mention it, so `out.css` was `undefined` in every
          // host: the Vite plugin's `if (out.css != null)` never ran, no module
          // ever imported its own stylesheet, and an application styled with
          // `stylex.create` shipped class names and no CSS. The transform was
          // right the whole time; the shim in front of it was returning three
          // quarters of the answer. See ubugeeei-prod/uf#306.
          resolve({
            code: reply.code,
            map: reply.map ?? null,
            css: reply.css ?? null,
            diagnostics: reply.diagnostics ?? [],
          });
        },
      });
      this.#child.stdin.write(`${JSON.stringify({ id, code, options })}\n`);
    });
  }

  /**
   * The build of `uf` this service's child is executing, or `null` when that
   * could not be established.
   *
   * Read once, before the spawn, and never again: the child goes on executing
   * the binary it started from however many times that file is rewritten
   * underneath it. Anything kept from an answer of this service belongs under
   * this identity and not under whatever the file says now.
   *
   * @returns {string | null}
   */
  get identity() {
    return this.#identity;
  }

  /**
   * Whether this service can still answer: `false` once its process has
   * exited or could not be started, after which every request rejects with
   * the reason. A caller that keeps a service across builds starts a new one
   * rather than failing every build after the one that lost it.
   *
   * @returns {boolean}
   */
  get alive() {
    return this.#failure == null;
  }

  /** Stop the process. Outstanding requests are rejected. */
  close() {
    this.#child.stdin.end();
    this.#child.kill();
  }
}

let shared = null;
let sharedForConfigBootstrap = null;

/**
 * The process-wide service, started on first use.
 *
 * The loader hooks and the config loader share one process per host rather
 * than one per module; it lives as long as the host does.
 */
export function sharedService(root, options = {}) {
  const entry = options.configBootstrap === true ? "bootstrap" : "project";
  if (entry === "bootstrap") {
    sharedForConfigBootstrap ??= new TransformService({
      root: root ?? environmentVariable("UF_PROJECT_ROOT") ?? process.cwd(),
      configBootstrap: true,
    });
    return sharedForConfigBootstrap;
  }
  shared ??= new TransformService({
    root: root ?? environmentVariable("UF_PROJECT_ROOT") ?? process.cwd(),
  });
  return shared;
}

/**
 * Transform one Flow module through the shared service.
 *
 * Returns `{ code, map, css, diagnostics }`; a module that is not uf's to
 * transform comes back as `null`.
 */
export function transformFlow(code, filename, options = {}) {
  return sharedService(options.root, {
    configBootstrap: options.configBootstrap === true,
  }).transform(filename, code, options);
}

/**
 * The most bytes one synchronous reply may be.
 *
 * `spawnSync` buffers the child's whole output and needs a ceiling to do it
 * with; its default is one megabyte, which a large module with its source map
 * appended can pass. Sixty-four is far above anything uf's own transform
 * ceilings let through, and still a ceiling.
 */
const MAX_SYNC_REPLY_BYTES = 64 * 1024 * 1024;

/**
 * Why Deno would refuse to start `command`, in Deno's own words, or `null`.
 *
 * `node:child_process`'s `spawnSync` on Deno answers a refused spawn with no
 * error and no output, so the reason is asked for again through `Deno.Command`,
 * which throws it. Measured on Deno 2.9.6, there are two: a program
 * `--allow-run` does not name, and a dynamic-loader variable such as
 * `LD_LIBRARY_PATH` in the environment, which Deno will not pass to a child
 * unless `--allow-run` is unscoped. `null` off Deno, and when Deno would start
 * the program after all.
 */
function denoRefusal(command) {
  const deno = globalThis.Deno;
  if (deno == null) return null;
  try {
    new deno.Command(command, {
      args: ["--version"],
      stdin: "null",
      stdout: "null",
      stderr: "null",
    }).outputSync();
    return null;
  } catch (error) {
    return typeof error?.message === "string" ? error.message : null;
  }
}

/**
 * Transform one Flow module in a short-lived `uf transform`, and wait for it.
 *
 * Resolves nothing and returns what `transformFlow` resolves to —
 * `{ code, map, css, diagnostics }`, or `null` for a module that is not uf's —
 * and throws what it rejects with. One request written to the child's stdin,
 * stdin closed, one reply read back: the same binary and the same protocol as
 * the service, so the module cannot differ between the two ways in.
 *
 * For Deno's in-thread hooks (`./internal/sync-hooks.js`), which on Node sleep
 * on a transform thread instead. On Deno that thread is not safe to rely on:
 * measured on Deno 2.9.6 in this repository's CI, Linux x86_64, one of two runs
 * of the same commit panicked with `Fatal error in :0: unreachable code` in
 * every test that compiled a module through the thread, and in none that read
 * the cache. A child process per cold module costs about ten milliseconds and
 * involves no thread and no `Atomics.wait` at all.
 *
 * @param {string} code the Flow source
 * @param {string} filename absolute path, used for the map and for errors
 * @param {object} [options] as for `transformFlow`, plus `command`
 */
export function transformFlowSync(code, filename, options = {}) {
  const command = options.command ?? ufBinary();
  const root = options.root ?? environmentVariable("UF_PROJECT_ROOT") ?? process.cwd();
  // Which binary to run is this process's business and not the compiler's.
  const requestOptions = { ...options };
  delete requestOptions.command;
  const env = inheritedEnvironment();
  if (options.configBootstrap === true) {
    env.UF_TRANSFORM_BOOTSTRAP_CONFIG = "1";
  } else {
    delete env.UF_TRANSFORM_BOOTSTRAP_CONFIG;
  }
  const result = spawnSync(command, ["--cwd", root, "transform"], {
    input: `${JSON.stringify({ id: filename, code, options: requestOptions })}\n`,
    encoding: "utf8",
    stdio: ["pipe", "pipe", "inherit"],
    env,
    maxBuffer: MAX_SYNC_REPLY_BYTES,
  });
  if (result.error != null) {
    throw new Error(`could not run \`${command} transform\`: ${result.error.message}`);
  }
  // A spawn a sandbox refuses is not an error on every host. Measured on Deno
  // 2.9: a program its permission set does not name comes back with no
  // `error`, no status and no output at all, and reading a reply out of that is
  // a `TypeError` about `undefined` naming neither the binary nor the grant.
  if (typeof result.stdout !== "string") {
    throw new Error(
      `could not run \`${command} transform\` for ${filename}: ${
        denoRefusal(command) ??
        "no process started, which is how a sandbox answers a program it was not told about " +
          `— on Deno, \`--allow-run\` has to name ${command}`
      }`,
    );
  }
  const newline = result.stdout.indexOf("\n");
  const line = newline === -1 ? result.stdout : result.stdout.slice(0, newline);
  if (line.trim() === "") {
    throw new Error(`uf transform exited (${result.status}) without answering for ${filename}`);
  }
  let reply;
  try {
    reply = JSON.parse(line);
  } catch {
    throw new Error(`uf transform sent a malformed reply: ${line}`);
  }
  if (reply.error != null) {
    throw new TransformError(filename, reply.error, reply.line, reply.column);
  }
  if (reply.code == null) return null;
  return {
    code: reply.code,
    map: reply.map ?? null,
    css: reply.css ?? null,
    diagnostics: reply.diagnostics ?? [],
  };
}
