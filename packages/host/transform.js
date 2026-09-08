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

import { spawn } from "node:child_process";
import { accessSync, constants, statSync } from "node:fs";
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
  const clean = stripQuery(id);
  if (!FLOW_EXTENSIONS.some((extension) => clean.endsWith(extension))) return false;
  const at = clean.lastIndexOf("/node_modules/");
  return at === -1 || clean.slice(at).startsWith("/node_modules/@uniflowed/");
}

function stripQuery(id) {
  const at = id.indexOf("?");
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
 * `tests/library/flow-modules.test.js` is that sentence as a test, over a
 * table that includes every case either expression could get wrong on its own.
 * Two spellings of one rule are a drift risk and the test is the thing that
 * makes them not one; the extension list is shared rather than repeated for
 * the same reason.
 *
 * The two exclusions are outside the contract because they cannot arrive
 * here: a NUL-prefixed id is a bundler's synthetic module and a query string
 * is a bundler's parameter, and neither is a path a host asks its filesystem
 * about. `isFlowModule` remains the authority for those callers.
 */
export const FLOW_MODULE_PATTERN = new RegExp(
  // Reject when the *last* `/node_modules/` on the path is not followed by
  // `@uniflowed/`, which is `isFlowModule`'s `lastIndexOf` written as a
  // lookahead: the inner negative lookahead is what pins "last".
  String.raw`^(?!.*/node_modules/(?!.*/node_modules/)(?!@uniflowed/))` +
    String.raw`.*\.(?:${FLOW_EXTENSIONS.map((extension) => extension.slice(1)).join("|")})$`,
);

/**
 * The `uf` binary to talk to.
 *
 * `uf dev`, `uf build` and `uf test` set `UF_BINARY` to themselves when they
 * start a host, so the host reaches exactly the binary that started it. A host
 * started by hand finds `uf` on PATH, which is what the installer arranges.
 */
export function ufBinary() {
  return process.env.UF_BINARY ?? "uf";
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
  return process.env.UF_IN_SOURCE_TESTS === "1";
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
    if (!stats.isFile()) return null;
    accessSync(binary, constants.X_OK);
    return `${binary}\0${stats.size}\0${stats.mtimeMs}`;
  } catch {
    // Named a binary that is not there, or is not one. The caller gets `null`
    // and stops trusting the cache, which is right: nothing can be compiled
    // either.
    return null;
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
  for (const directory of (process.env.PATH ?? "").split(path.delimiter)) {
    if (directory === "") continue;
    const candidate = path.join(directory, command);
    try {
      if (!statSync(candidate).isFile()) continue;
      accessSync(candidate, constants.X_OK);
      return candidate;
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
 * this: its module hooks run on a loader thread of their own, and the process
 * exits with the main thread whatever that thread is still holding. That
 * accident is the only reason it was ever invisible, and it is not something
 * the second host can be asked to reproduce.
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
    this.#child = spawn(command, ["--cwd", root, "transform"], {
      stdio: ["pipe", "pipe", "inherit"],
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

  /** Stop the process. Outstanding requests are rejected. */
  close() {
    this.#child.stdin.end();
    this.#child.kill();
  }
}

let shared = null;

/**
 * The process-wide service, started on first use.
 *
 * The loader hooks and the config loader share one process per host rather
 * than one per module; it lives as long as the host does.
 */
export function sharedService(root) {
  shared ??= new TransformService({ root: root ?? process.env.UF_PROJECT_ROOT ?? process.cwd() });
  return shared;
}

/**
 * Transform one Flow module through the shared service.
 *
 * Returns `{ code, map, css, diagnostics }`; a module that is not uf's to
 * transform comes back as `null`.
 */
export function transformFlow(code, filename, options = {}) {
  return sharedService(options.root).transform(filename, code, options);
}
