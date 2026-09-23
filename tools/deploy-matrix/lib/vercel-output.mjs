#!/usr/bin/env node
// @noflow
//
// Serve a Vercel Build Output API (v3) directory on this machine, the way
// Vercel's platform routes it and its Node.js launcher calls a function — for
// the deploy matrix's `vercel` column, which has no account and no token.
//
//   node tools/deploy-matrix/lib/vercel-output.mjs <.vercel/output> <port>
//
// # Why this exists, and what it is not
//
// Vercel offers no offline server for `.vercel/output`: `vercel dev` needs a
// linked project and a token, and it builds rather than serving a prebuilt
// directory. So this is uf's own emulation, and the matrix says so in the
// cells it backs ("runtime emulated by uf's harness, not deployed"). It is
// kept to what the Build Output API documents and nothing more, so a check
// that passes here fails only where Vercel's platform does something the
// specification does not say:
//
// * `config.json` must be `{ "version": 3, "routes": [...] }`. Routes are
//   tried in order: `{ "handle": "filesystem" }` serves `static/`; a route with
//   `src` (a regular expression over the path, anchored at both ends, as the
//   specification states) and `dest` invokes the function at
//   `functions/<dest>.func`; `headers` on a matched route are added; `status`
//   without `dest` answers with that status. Nothing else is supported, and an
//   unknown key is refused rather than ignored.
// * A function's `.vc-config.json` must name a `nodejs<N>.x` runtime, the
//   `Nodejs` launcher and a `handler` file that exists. The handler's default
//   export is called with Node's own `IncomingMessage` and `ServerResponse`,
//   with `req.url` the original path (a rewrite does not change it), which is
//   what Vercel's Node.js launcher does with `shouldAddHelpers: false`.
//   Functions stay loaded between requests, like a warm instance.
//
// It validates first and serves second: a directory the specification would
// reject makes this exit non-zero before any request, naming the problem.

import assert from "node:assert/strict";
import { existsSync, readFileSync, statSync } from "node:fs";
import { createServer } from "node:http";
import path from "node:path";
import { pathToFileURL } from "node:url";

/** The route keys this emulation implements; anything else is refused. */
const ROUTE_KEYS = new Set(["src", "dest", "headers", "status", "handle", "continue"]);

/**
 * Read and check `<output>/config.json` and every function a route names.
 *
 * @param {string} output the `.vercel/output` directory
 * @returns {{ routes: Array<object>, functions: Map<string, string> }} the
 *   routes, and each routed function's handler file by `dest`
 */
export function readBuildOutput(output) {
  const config = JSON.parse(readFileSync(path.join(output, "config.json"), "utf8"));
  assert.equal(config.version, 3, "config.json: version must be 3");
  assert.ok(Array.isArray(config.routes), "config.json: routes must be an array");
  const functions = new Map();
  for (const [at, route] of config.routes.entries()) {
    for (const key of Object.keys(route)) {
      assert.ok(
        ROUTE_KEYS.has(key),
        `config.json: routes[${at}] has ${key}, which this emulation does not implement`,
      );
    }
    if (route.handle != null) {
      assert.equal(
        route.handle,
        "filesystem",
        `config.json: routes[${at}]: only the filesystem phase is emulated`,
      );
      continue;
    }
    assert.equal(typeof route.src, "string", `config.json: routes[${at}] has no src`);
    new RegExp(route.src);
    if (route.dest == null) continue;
    const name = route.dest.replace(/^\//, "");
    const directory = path.join(output, "functions", `${name}.func`);
    const vc = JSON.parse(readFileSync(path.join(directory, ".vc-config.json"), "utf8"));
    assert.match(
      String(vc.runtime),
      /^nodejs\d+\.x$/,
      `${name}.func: runtime ${vc.runtime} is not a Node.js runtime`,
    );
    assert.equal(vc.launcherType, "Nodejs", `${name}.func: launcherType must be Nodejs`);
    assert.equal(typeof vc.handler, "string", `${name}.func: no handler`);
    const handler = path.join(directory, vc.handler);
    assert.ok(existsSync(handler), `${name}.func: the handler ${vc.handler} does not exist`);
    functions.set(route.dest, handler);
  }
  return { routes: config.routes, functions };
}

/** A file under `root` for `pathname`, or `null`; the filesystem phase. */
function staticFile(root, pathname) {
  const decoded = decodeURIComponent(pathname);
  for (const candidate of [decoded, path.join(decoded, "index.html"), `${decoded}.html`]) {
    const file = path.join(root, candidate);
    if (!file.startsWith(root)) return null;
    if (existsSync(file) && statSync(file).isFile()) return file;
  }
  return null;
}

/**
 * Start serving `output` on `port`; resolves with the server once it listens.
 *
 * @param {string} output
 * @param {number} port
 */
export async function serveBuildOutput(output, port) {
  const { routes, functions } = readBuildOutput(output);
  const loaded = new Map();
  for (const [dest, file] of functions) {
    const module = await import(pathToFileURL(file).href);
    assert.equal(
      typeof module.default,
      "function",
      `${file}: the default export is not a function`,
    );
    loaded.set(dest, module.default);
  }
  const staticRoot = path.join(output, "static");
  const server = createServer(async (req, res) => {
    const pathname = new URL(req.url ?? "/", "http://vercel.local").pathname;
    try {
      for (const route of routes) {
        if (route.handle === "filesystem") {
          const file = existsSync(staticRoot) ? staticFile(staticRoot, pathname) : null;
          if (file != null) {
            res.end(readFileSync(file));
            return;
          }
          continue;
        }
        if (!new RegExp(`^(?:${route.src})$`).test(pathname)) continue;
        for (const [name, value] of Object.entries(route.headers ?? {})) res.setHeader(name, value);
        if (route.dest != null) {
          await loaded.get(route.dest)(req, res);
          return;
        }
        if (route.status != null) {
          res.statusCode = route.status;
          res.end();
          return;
        }
      }
      res.statusCode = 404;
      res.end("NOT_FOUND\n");
    } catch (error) {
      process.stderr.write(`vercel-output: ${error?.stack ?? String(error)}\n`);
      if (!res.headersSent) res.statusCode = 500;
      res.end();
    }
  });
  await new Promise((resolve) => server.listen(port, "127.0.0.1", resolve));
  return server;
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  const [output, port] = process.argv.slice(2);
  if (output == null || port == null) {
    process.stderr.write(
      "usage: node tools/deploy-matrix/lib/vercel-output.mjs <.vercel/output> <port>\n",
    );
    process.exit(2);
  }
  try {
    await serveBuildOutput(path.resolve(output), Number(port));
    process.stdout.write(`vercel-output: serving ${output} on 127.0.0.1:${port}\n`);
  } catch (error) {
    process.stderr.write(`vercel-output: ${error?.message ?? String(error)}\n`);
    process.exit(1);
  }
}
