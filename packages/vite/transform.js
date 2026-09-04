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
 * A build driver synthesises modules of its own (`\0vite/client`, Rolldown's
 * shims), and a third-party dependency ships JavaScript that is already
 * JavaScript; neither is Flow. `@uniflowed/*` under `node_modules` is the
 * deliberate exception: those packages ship Flow source, because that is what
 * uf tells everyone to write.
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
  #failure = null;

  /**
   * @param {object} [options]
   * @param {string} [options.command] the `uf` binary; `ufBinary()` by default
   * @param {string} [options.root] project root, so `uf.config.js` is found
   */
  constructor(options = {}) {
    const command = options.command ?? ufBinary();
    const root = options.root ?? process.cwd();
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
          resolve({ code: reply.code, map: reply.map ?? null, diagnostics: reply.diagnostics ?? [] });
        },
      });
      this.#child.stdin.write(`${JSON.stringify({ id, code, options })}\n`);
    });
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
