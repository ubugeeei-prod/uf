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
 */
export class TransformService {
  #child;
  #pending = [];
  #identity;
  #failure = null;

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
    });

    this.#child.on("error", (error) => {
      this.#settleAll(new Error(`could not run \`${command} transform\`: ${error.message}`));
    });
    this.#child.on("close", (code) => {
      this.#settleAll(new Error(`uf transform exited (${code})`));
    });
  }

  #settleAll(error) {
    this.#failure = error;
    while (this.#pending.length > 0) this.#pending.shift().reject(error);
  }

  /**
   * Transform one module.
   *
   * Resolves to `{ code, map, diagnostics }`, or to `null` when the module is
   * not uf's to transform (see `isFlowModule`). Rejects with a
   * `TransformError` carrying the position when the source is not valid Flow.
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
    return new Promise((resolve, reject) => {
      this.#pending.push({
        id,
        reject,
        resolve: (reply) => {
          if (reply.code == null) {
            resolve(null);
            return;
          }
          resolve({
            code: reply.code,
            map: reply.map ?? null,
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
 * Returns `{ code, map, diagnostics }`; a module that is not uf's to transform
 * comes back as `null`.
 */
export function transformFlow(code, filename, options = {}) {
  return sharedService(options.root).transform(filename, code, options);
}
