// @flow
//
// `@uniflowed/vite` — uf's build stages as a Vite plugin.
//
// Vite runs the dev server, the module graph, HMR and the production build.
// uf contributes only what is specific to Flow: erasing types, blanking the
// RSC directive prologue, and lowering JSX. Those are native, so this pipes
// each module through one long-lived `uf transform` process rather than
// re-entering the binary per file.

import { spawn } from "node:child_process";
import { createInterface } from "node:readline";

/**
 * One `uf transform` process, with requests answered in the order they were
 * sent.
 *
 * `uf transform` replies once per request, in order, so a plain queue of
 * resolvers is enough to pair a reply with its caller — no correlation ids and
 * no map to leak.
 */
function startService(command, cwd) {
  const child = spawn(command, ["--cwd", cwd, "transform"], {
    stdio: ["pipe", "pipe", "inherit"],
  });
  const pending = [];
  let failure = null;

  const settleAll = (error) => {
    failure = error;
    while (pending.length > 0) pending.shift().reject(error);
  };

  createInterface({ input: child.stdout }).on("line", (line) => {
    const waiting = pending.shift();
    if (!waiting) return;
    let reply;
    try {
      reply = JSON.parse(line);
    } catch (error) {
      waiting.reject(new Error(`uf transform sent a malformed reply: ${line}`));
      return;
    }
    if (reply.error) waiting.reject(new Error(reply.error));
    else waiting.resolve(reply.code ?? null);
  });

  child.on("error", (error) =>
    settleAll(new Error(`could not run \`${command} transform\`: ${error.message}`)),
  );
  // Any exit is final. Recording it even when nothing is outstanding means a
  // request made after an idle exit is rejected at once rather than queued
  // against a process that will never answer.
  child.on("close", (code) => {
    settleAll(new Error(`uf transform exited (${code})`));
  });

  return {
    transform(id, code) {
      if (failure) return Promise.reject(failure);
      return new Promise((resolve, reject) => {
        pending.push({ resolve, reject });
        child.stdin.write(`${JSON.stringify({ id, code })}\n`);
      });
    },
    close() {
      child.stdin.end();
      child.kill();
    },
  };
}

/**
 * Whether uf is responsible for transforming this module.
 *
 * Mirrors `uf_bundler::is_project_module`, and must keep mirroring it: a `uf
 * dev` session and a `uf build` that disagree about which files are Flow
 * disagree about what the code is.
 *
 * `@uniflowed/*` under `node_modules` is included on purpose — those packages
 * ship Flow source, and skipping them leaves `// @flow` in front of a bundler
 * that reports `Flow is not supported`.
 */
function isProjectModule(id) {
  if (!id.endsWith(".js") || id.startsWith("\0")) return false;
  const at = id.lastIndexOf("/node_modules/");
  return at === -1 || id.slice(at).startsWith("/node_modules/@uniflowed/");
}

/**
 * uf's Vite plugin.
 *
 * `command` is the `uf` binary to talk to; the default assumes it is on PATH,
 * which is what the installer arranges.
 */
export default function uniflowed(options) {
  const command = options?.command ?? "uf";
  let service = null;
  let root = process.cwd();

  return {
    name: "uniflowed",
    // Ahead of Vite's own esbuild/oxc transform, which does not know Flow and
    // would reject the annotations this removes.
    enforce: "pre",

    configResolved(config) {
      root = config.root ?? root;
    },

    buildStart() {
      service ??= startService(command, root);
    },

    async transform(code, id) {
      if (!isProjectModule(id)) return null;
      const transformed = await service.transform(id, code);
      // `null` means no stage changed anything, so Vite keeps the original
      // module and its existing source map rather than taking a copy.
      return transformed === null ? null : { code: transformed, map: null };
    },

    buildEnd() {
      service?.close();
      service = null;
    },
  };
}
