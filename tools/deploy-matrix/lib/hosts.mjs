// @noflow
//
// The hosts the deploy matrix starts: one per column of `../matrix.json`, each
// the platform's own runtime or its own local emulator, serving the directory
// `uf build --adapter <target>` wrote — never a harness calling `handler.js`.
//
// | target | what serves the build |
// | --- | --- |
// | `node`, `bun`, `deno` | the generated `server.js`, started the way `uf build` prints |
// | `container` | the generated `Dockerfile`, built and run by Docker |
// | `edge` | `wrangler dev --local` — workerd, the assets binding, local Workers KV |
// | `serverless` | Kumo's API Gateway HTTP API, invoking the function in the AWS Lambda Node image (its Runtime Interface Emulator) |
// | `static` | Workers static assets under `wrangler dev --local`, with no Worker script |
// | `vercel` | the Build Output API directory, routed and invoked by `./vercel-output.mjs` (emulated), with Kumo's S3 |
//
// A host is `{ base, stop, restart, alive, logs }`. `restart` stops the
// process and starts it again on the same address with the same durable store
// (the working directory, the container's filesystem, Wrangler's local state,
// the S3 bucket in Kumo) — which is what lets the ISR checks prove a
// regenerated page outlives the process that regenerated it.
//
// Pinned tool versions live here and in `.github/workflows/ci.yml` (which
// installs them); `../matrix.json` names them for the docs.

import assert from "node:assert/strict";
import { execFileSync, spawn, spawnSync } from "node:child_process";
import { lookup } from "node:dns/promises";
import { mkdirSync, writeFileSync } from "node:fs";
import net from "node:net";
import path from "node:path";
import { setTimeout as sleep } from "node:timers/promises";
import { fileURLToPath } from "node:url";

import { waitUntilAnswering } from "./http.mjs";

/** The Wrangler every Cloudflare host runs; the same pin as `tools/ci/edge-worker-smoke.sh`. */
export const WRANGLER_VERSION = "4.128.0";

/**
 * The AWS Lambda Node.js 24 base image, by digest: its entrypoint is the
 * Runtime Interface Emulator in front of the real Node runtime client, which is
 * as close to Lambda as a machine without an AWS account gets.
 */
export const LAMBDA_IMAGE =
  "public.ecr.aws/lambda/nodejs:24@sha256:486431faaa8a45cc6b23c06ce5281670ed98faa7a45f06411121c33b7247df49";

/** A free TCP port on the loopback interface. */
export function freePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address();
      server.close(() => resolve(port));
    });
  });
}

/**
 * Start `command` in its own process group, logging to `log`.
 *
 * A process group because the interesting process is often a grandchild —
 * Wrangler's Node process starts workerd — and signalling only the child
 * leaves the server answering. See `tools/ci/edge-worker-smoke.sh`, which
 * learned that the hard way.
 */
function startProcess(command, args, { cwd, env, log }) {
  const lines = [];
  const child = spawn(command, args, {
    cwd,
    env: { ...process.env, ...env },
    detached: true,
    stdio: ["ignore", "pipe", "pipe"],
  });
  for (const stream of [child.stdout, child.stderr]) {
    stream.on("data", (data) => {
      lines.push(String(data));
      if (log != null) log(String(data));
    });
  }
  let exited = false;
  child.once("exit", () => {
    exited = true;
  });
  return {
    child,
    alive: () => !exited,
    output: () => lines.join(""),
    async stop() {
      if (exited) return;
      const gone = new Promise((resolve) => child.once("exit", resolve));
      try {
        process.kill(-child.pid, "SIGTERM");
      } catch {
        return;
      }
      await Promise.race([gone, sleep(5000)]);
      if (!exited) {
        try {
          process.kill(-child.pid, "SIGKILL");
        } catch {
          // Already gone between the check and the signal.
        }
        await Promise.race([gone, sleep(2000)]);
      }
    },
  };
}

/**
 * A container's output, both streams: `docker logs` replays the container's
 * stderr on its own stderr, which is where a crashing server writes why.
 */
function dockerLogs(name) {
  const logged = spawnSync("docker", ["logs", name], { encoding: "utf8" });
  return `${logged.stdout ?? ""}${logged.stderr ?? ""}`;
}

/** Run `docker …` and answer its stdout; throws with its stderr on failure. */
function docker(...args) {
  return execFileSync("docker", args, {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();
}

/**
 * A host that is a long-running process on a port: `node`, `bun`, `deno`, and
 * Wrangler for `edge` and `static`.
 */
async function processHost(name, spawnArgs, { cwd, env = {}, port }) {
  const base = `http://127.0.0.1:${port}`;
  let current = null;
  const start = async () => {
    current = startProcess(spawnArgs[0], spawnArgs.slice(1), { cwd, env });
    try {
      await waitUntilAnswering(base, { alive: () => current.alive() });
    } catch (error) {
      throw new Error(
        `${name} did not start: ${error.message}\n--- ${name} output ---\n${current.output().slice(-4000)}`,
      );
    }
  };
  await start();
  return {
    base,
    alive: () => current.alive(),
    logs: () => current.output(),
    stop: () => current.stop(),
    async restart() {
      await current.stop();
      await start();
    },
  };
}

/**
 * The command `uf build` printed for the directory it wrote — the `run` line
 * of its summary, without the `cd <directory> &&` in front — as an argument
 * list.
 *
 * Read from the build rather than written here, so the matrix starts a server
 * the way a person is told to, including every permission flag the printed
 * command has (or lacks). None of the commands has quoting in it; one that did
 * would be refused here rather than split wrongly.
 *
 * @param {string} output everything `uf build --adapter` printed
 */
export function printedCommand(output) {
  const line = output.split("\n").find((candidate) => /^\s*run\s{2,}\S/.test(candidate));
  assert.ok(line != null, `uf build printed no run line:\n${output.slice(-2000)}`);
  const command = line
    .replace(/^\s*run\s+/, "")
    .replace(/^cd \S+ && /, "")
    .trim();
  assert.ok(!/["'\\]/.test(command), `the printed run command needs a shell: ${command}`);
  return command.split(/\s+/);
}

/** A server started with the command `uf build` printed: `node`, `bun`, `deno`. */
async function printed(deployDir, { output }) {
  const port = await freePort();
  const command = printedCommand(output);
  return processHost(command.join(" "), command, {
    cwd: deployDir,
    env: { PORT: String(port), HOST: "127.0.0.1" },
    port,
  });
}

/** The generated `Dockerfile`, built and run; the image listens on 3000. */
async function container(deployDir) {
  const port = await freePort();
  const tag = "uf-deploy-matrix:container";
  const name = `uf-deploy-matrix-container-${process.pid}`;
  execFileSync("docker", ["build", "--quiet", "-t", tag, deployDir], { stdio: "inherit" });
  docker("run", "-d", "--name", name, "-p", `127.0.0.1:${port}:3000`, tag);
  const base = `http://127.0.0.1:${port}`;
  const running = () => {
    try {
      return docker("inspect", "-f", "{{.State.Running}}", name) === "true";
    } catch {
      return false;
    }
  };
  const logs = () => dockerLogs(name);
  const ready = async () => {
    try {
      await waitUntilAnswering(base, { alive: running });
    } catch (error) {
      let state = "";
      try {
        state = docker("inspect", "-f", "{{json .State}}", name);
      } catch (inspected) {
        state = String(inspected.stderr ?? inspected.message);
      }
      throw new Error(
        `the container did not start: ${error.message}\n--- state ---\n${state}\n--- docker logs ---\n${logs().slice(-4000)}`,
      );
    }
  };
  await ready();
  return {
    base,
    alive: running,
    logs,
    async stop() {
      try {
        docker("rm", "-f", name);
      } catch {
        // Nothing to remove.
      }
    },
    async restart() {
      // `docker restart` keeps the container's filesystem, which is where the
      // filesystem route cache lives: a restart, not a redeploy.
      docker("restart", name);
      await ready();
    },
  };
}

/**
 * The Wrangler binary: `WRANGLER_BIN`, which CI points at the pinned install
 * (`tools/deploy-matrix/install-tools.sh`), or `wrangler` on `PATH`. Refused
 * unless it is [`WRANGLER_VERSION`], so a column is never checked against a
 * different Wrangler than the one the documentation names.
 */
function wranglerBinary() {
  const binary = process.env.WRANGLER_BIN ?? "wrangler";
  let reported = "";
  try {
    reported = execFileSync(binary, ["--version"], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    });
  } catch (error) {
    throw new Error(
      `no Wrangler at ${binary} (${error.message}); run tools/deploy-matrix/install-tools.sh wrangler and set WRANGLER_BIN`,
    );
  }
  assert.ok(
    reported.includes(WRANGLER_VERSION),
    `${binary} is Wrangler ${reported.trim()}, and the matrix is pinned to ${WRANGLER_VERSION}`,
  );
  return binary;
}

/** `wrangler dev --local` on `configDir`'s `wrangler.json`. */
async function wrangler(configDir, label) {
  const port = await freePort();
  return processHost(
    `wrangler dev (${label})`,
    [
      wranglerBinary(),
      "dev",
      "--local",
      "--config",
      "wrangler.json",
      "--ip",
      "127.0.0.1",
      "--port",
      String(port),
      "--show-interactive-dev-session=false",
    ],
    { cwd: configDir, env: { CI: "1", WRANGLER_SEND_METRICS: "false" }, port },
  );
}

/** The Worker `uf build --adapter edge` wrote, with its assets binding and KV. */
function edge(deployDir) {
  return wrangler(deployDir, "edge");
}

/**
 * `--adapter static`'s directory, served as Workers static assets with no
 * Worker script: `html_handling` for pretty URLs and `404-page` for the
 * not-found document, which is how a static host is told about `404.html`.
 * The configuration is written beside the directory rather than into it,
 * because the directory is what gets uploaded and a host's configuration is
 * not part of the site.
 */
function staticHost(deployDir) {
  const hostDir = `${deployDir}-host`;
  mkdirSync(hostDir, { recursive: true });
  writeFileSync(
    path.join(hostDir, "wrangler.json"),
    `${JSON.stringify(
      {
        name: "uf-deploy-matrix-static",
        compatibility_date: "2024-09-23",
        assets: {
          directory: path.relative(hostDir, deployDir),
          // uf links to `/posts/first`, and the build writes
          // `posts/first/index.html`: served at the link's URL, not redirected
          // to one with a trailing slash first.
          html_handling: "drop-trailing-slash",
          not_found_handling: "404-page",
        },
      },
      null,
      2,
    )}\n`,
  );
  return wrangler(hostDir, "static");
}

/** One request to Kumo's control plane, as JSON. */
async function kumoCall(kumo, method, route, body) {
  const response = await fetch(`${kumo}${route}`, {
    method,
    headers: { "content-type": "application/json" },
    body: body == null ? undefined : JSON.stringify(body),
  });
  const text = await response.text();
  assert.ok(response.ok, `Kumo ${method} ${route} answered ${response.status}: ${text}`);
  return text === "" ? {} : JSON.parse(text);
}

/**
 * The function behind Kumo's API Gateway HTTP API.
 *
 * * Kumo (`KUMO_BIN`, installed by CI at a pinned version and checksum) is the
 *   AWS control and data plane: Lambda, API Gateway v2 and S3.
 * * The deployment package runs in the AWS Lambda Node 24 image. Its
 *   entrypoint is the Runtime Interface Emulator, which starts the real Node
 *   runtime client on `lambda.handler`, the file `uf build` wrote, mounted
 *   read-only at `/var/task` as a deployment package would be.
 * * Kumo's `CreateFunction` points at that emulator through `InvokeEndpoint`
 *   (a Kumo extension), and an HTTP API with a `$default` route and a
 *   payload format 2.0 `AWS_PROXY` integration sends every request to it.
 *
 * Requests go to `http://<apiId>.execute-api.localhost:<port>`, the address
 * Kumo answers an API on. The container uses the host network, so the
 * function reaches Kumo's S3 (the route cache, `s3-cache.js`) on loopback.
 */
/**
 * Kumo (`KUMO_BIN`), started on a free port and answering; its address and
 * process. The AWS control and data plane for `serverless`, and the S3 the
 * `vercel` column's route-cache provider keeps regenerated pages in.
 */
async function startKumo(cwd) {
  const kumoBin = process.env.KUMO_BIN ?? "kumo";
  const kumoPort = await freePort();
  const kumo = `http://127.0.0.1:${kumoPort}`;
  const server = startProcess(kumoBin, [], {
    cwd,
    env: { KUMO_HOST: "127.0.0.1", KUMO_PORT: String(kumoPort), KUMO_LOG_LEVEL: "warn" },
  });
  try {
    await waitUntilAnswering(kumo, { alive: server.alive, path: "/health" });
  } catch (error) {
    throw new Error(`Kumo did not start: ${error.message}\n${server.output().slice(-4000)}`);
  }
  return { kumo, kumoPort, server };
}

async function serverless(deployDir) {
  const { kumo, kumoPort, server } = await startKumo(deployDir);

  // The Runtime Interface Emulator listens on 8080 inside the container, and
  // with the host network that is 8080 on this machine.
  const name = `uf-deploy-matrix-lambda-${process.pid}`;
  const runtime = "http://127.0.0.1:8080/2015-03-31/functions/function/invocations";
  const startFunction = () =>
    docker(
      "run",
      "-d",
      "--name",
      name,
      "--network",
      "host",
      "-v",
      `${deployDir}:/var/task:ro`,
      "-e",
      `UF_MATRIX_S3_ENDPOINT=${kumo}`,
      "-e",
      "UF_MATRIX_S3_BUCKET=uf-deploy-matrix",
      "-e",
      "AWS_LAMBDA_FUNCTION_TIMEOUT=30",
      LAMBDA_IMAGE,
      "lambda.handler",
    );
  const functionLogs = () => dockerLogs(name);
  startFunction();

  await kumoCall(kumo, "POST", "/2015-03-31/functions", {
    FunctionName: "uf-deploy-matrix",
    Runtime: "nodejs24.x",
    Role: "arn:aws:iam::000000000000:role/uf-deploy-matrix",
    Handler: "lambda.handler",
    Code: {},
    Timeout: 30,
    InvokeEndpoint: runtime,
  });
  const api = await kumoCall(kumo, "POST", "/v2/apis", {
    name: "uf-deploy-matrix",
    protocolType: "HTTP",
  });
  const integration = await kumoCall(kumo, "POST", `/v2/apis/${api.apiId}/integrations`, {
    integrationType: "AWS_PROXY",
    integrationUri: "arn:aws:lambda:us-east-1:000000000000:function:uf-deploy-matrix",
    payloadFormatVersion: "2.0",
  });
  await kumoCall(kumo, "POST", `/v2/apis/${api.apiId}/routes`, {
    routeKey: "$default",
    target: `integrations/${integration.integrationId}`,
  });
  await kumoCall(kumo, "POST", `/v2/apis/${api.apiId}/stages`, {
    stageName: "$default",
    autoDeploy: true,
  });

  // `*.localhost` is loopback to a browser; to Node it is whatever the
  // resolver says, and a runner's resolver may say nothing. Adding the one
  // name to /etc/hosts is the fix that keeps the request itself untouched.
  const host = `${api.apiId}.execute-api.localhost`;
  try {
    await lookup(host);
  } catch {
    execFileSync("sudo", ["sh", "-c", `echo "127.0.0.1 ${host}" >> /etc/hosts`]);
  }
  const base = `http://${host}:${kumoPort}`;
  const running = () => {
    try {
      return server.alive() && docker("inspect", "-f", "{{.State.Running}}", name) === "true";
    } catch {
      return false;
    }
  };
  const ready = async () => {
    try {
      await waitUntilAnswering(base, { alive: running });
    } catch (error) {
      throw new Error(
        `the function did not answer through Kumo: ${error.message}\n--- function ---\n${functionLogs().slice(-4000)}\n--- kumo ---\n${server.output().slice(-4000)}`,
      );
    }
  };
  await ready();
  return {
    base,
    alive: running,
    logs: () => `--- function ---\n${functionLogs()}\n--- kumo ---\n${server.output()}`,
    async stop() {
      try {
        docker("rm", "-f", name);
      } catch {
        // Nothing to remove.
      }
      await server.stop();
    },
    async restart() {
      // A cold start: a new container, so a new Node process with empty
      // memory, while Kumo — and the S3 bucket in it — keeps running.
      docker("rm", "-f", name);
      startFunction();
      await ready();
    },
  };
}

/**
 * `uf build --adapter vercel`'s `.vercel/output`, served by
 * `./vercel-output.mjs` — uf's emulation of Vercel's router and Node.js
 * launcher, because Vercel has no offline server for a prebuilt directory and
 * `vercel dev` needs an account. What that file does and does not emulate is
 * in its header; the matrix names the cells it backs as emulated.
 *
 * Kumo runs beside it for S3, where the fixture's route-cache provider keeps
 * regenerated pages, so a restart (a new function process) reads them back.
 */
async function vercel(deployDir) {
  const { kumo, server: kumoServer } = await startKumo(deployDir);
  const port = await freePort();
  const host = await processHost(
    "vercel output",
    [
      process.execPath,
      fileURLToPath(new URL("./vercel-output.mjs", import.meta.url)),
      path.join(deployDir, ".vercel", "output"),
      String(port),
    ],
    {
      cwd: deployDir,
      env: { UF_MATRIX_S3_ENDPOINT: kumo, UF_MATRIX_S3_BUCKET: "uf-deploy-matrix" },
      port,
    },
  );
  return {
    ...host,
    alive: () => host.alive() && kumoServer.alive(),
    logs: () => `${host.logs()}\n--- kumo ---\n${kumoServer.output()}`,
    async stop() {
      await host.stop();
      await kumoServer.stop();
    },
  };
}

/** Every host, by the target id `../matrix.json` uses. */
export const HOSTS = {
  node: printed,
  bun: printed,
  deno: printed,
  container,
  edge,
  serverless,
  static: staticHost,
  vercel,
};
