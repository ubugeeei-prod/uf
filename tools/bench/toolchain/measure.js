// @flow
//
// The stopwatches: a command from spawn to exit, a dev server from spawn to its
// first document, and an edit from the write to the page having the new module.
//
// # One clock, in this process
//
// `performance.now()` here, read on either side of the thing being measured.
// Nothing a child reports about itself is used. `uf build` prints its own
// total, and it is a number about the part of the run uf can see — it starts
// after the binary has loaded and stops before the process exits. What a
// person waits for is from pressing return to getting the prompt back, and
// that is spawn to exit.
//
// # Process groups
//
// Every child starts in a group of its own (`detached`) and is stopped by
// signalling the group. `uf dev` is a Rust process with a Node driver under it
// and Vite under that; signalling only the pid would be trusting every layer to
// pass the signal on, and a benchmark that leaks a dev server measures its own
// leak on the next run, as a port that is taken and a core that is busy.

import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { performance } from "node:perf_hooks";
import process from "node:process";

export type Summary = {
  readonly median: number,
  readonly min: number,
  readonly max: number,
  readonly mean: number,
  readonly stddev: number,
};

/** A tenth of a millisecond, which is already finer than any of these clocks. */
const round = (value: number): number => Number(value.toFixed(1));

/**
 * The median, the extremes, the mean and the standard deviation of `samples`.
 *
 * The median is the number to read: a run on a shared machine has outliers in
 * one direction only, and the mean is pulled by every one of them. The spread
 * is beside it so a reader can see how much a median is worth.
 */
export function summarise(samples: $ReadOnlyArray<number>): Summary {
  if (samples.length === 0) {
    throw new Error(
      "there are no samples to summarise, and a stage that timed nothing has no number",
    );
  }
  const sorted = [...samples].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  const median =
    sorted.length % 2 === 0 ? (sorted[middle - 1] + sorted[middle]) / 2 : sorted[middle];
  const mean = samples.reduce((sum, value) => sum + value, 0) / samples.length;
  const variance = samples.reduce((sum, value) => sum + (value - mean) ** 2, 0) / samples.length;
  return {
    median: round(median),
    min: round(sorted[0]),
    max: round(sorted[sorted.length - 1]),
    mean: round(mean),
    stddev: round(Math.sqrt(variance)),
  };
}

export type Environment = { readonly [string]: string | void };

export type Finished = {
  readonly ms: number,
  readonly code: number | null,
  readonly signal: string | null,
  readonly timedOut: boolean,
  readonly stdout: string,
  readonly stderr: string,
};

/**
 * The part of a child process this file uses.
 *
 * Written out because the `node:child_process` library definition types the
 * class as `any`, and Flow will not accept an `any`-typed value as a type. The
 * listeners take `mixed` and refine it, which is what an event payload is.
 */
type Stream = {
  setEncoding(encoding: string): mixed,
  on(event: "data", listener: (chunk: string) => void): mixed,
  ...
};

type Child = {
  readonly pid: ?number,
  readonly stdout: ?Stream,
  readonly stderr: ?Stream,
  on(event: string, listener: (...values: Array<mixed>) => void): mixed,
  ...
};

const sleep = (ms: number): Promise<void> =>
  new Promise((resolve) => {
    setTimeout(resolve, ms);
  });

/** Every child still running, so a failure can take them all down with it. */
const live: Set<Child> = new Set();

function signalGroup(child: Child, signal: string): void {
  if (child.pid == null) {
    return;
  }
  try {
    process.kill(-child.pid, signal);
  } catch {
    // The group is already gone, which is the state being asked for.
  }
}

function groupAlive(child: Child): boolean {
  if (child.pid == null) {
    return false;
  }
  try {
    process.kill(-child.pid, 0);
    return true;
  } catch {
    return false;
  }
}

/** Stop everything this process started, for the paths that never finish. */
export function stopEverything(): void {
  for (const child of live) {
    signalGroup(child, "SIGKILL");
  }
  live.clear();
}

/**
 * Run `program` to completion and time it from spawn to exit.
 *
 * Exit rather than `close`: `close` waits for every holder of the pipes, and a
 * grandchild that keeps stdout open would add its lifetime to a command that
 * had already given the prompt back. The promise still waits for `close`, so
 * the output is complete when it is read.
 */
export function run(
  program: string,
  args: $ReadOnlyArray<string>,
  options: { readonly cwd: string, readonly env: Environment, readonly timeoutMs: number },
): Promise<Finished> {
  return new Promise<Finished>((resolve, reject) => {
    const startedAt = performance.now();
    let exitedAt = startedAt;
    let timedOut = false;
    const child: Child = spawn(program, [...args], {
      cwd: options.cwd,
      env: options.env,
      detached: true,
      stdio: ["ignore", "pipe", "pipe"],
    });
    live.add(child);
    let stdout = "";
    let stderr = "";
    child.stdout?.setEncoding("utf8");
    child.stderr?.setEncoding("utf8");
    child.stdout?.on("data", (chunk: string) => {
      stdout += chunk;
    });
    child.stderr?.on("data", (chunk: string) => {
      stderr += chunk;
    });
    const timer = setTimeout(() => {
      timedOut = true;
      signalGroup(child, "SIGKILL");
    }, options.timeoutMs);
    child.on("error", (error: mixed) => {
      clearTimeout(timer);
      live.delete(child);
      reject(error instanceof Error ? error : new Error(String(error)));
    });
    child.on("exit", () => {
      exitedAt = performance.now();
    });
    child.on("close", (code: mixed, signal: mixed) => {
      clearTimeout(timer);
      live.delete(child);
      resolve({
        ms: exitedAt - startedAt,
        code: typeof code === "number" ? code : null,
        signal: typeof signal === "string" ? signal : null,
        timedOut,
        stdout,
        stderr,
      });
    });
  });
}

/**
 * A port nothing is listening on, from the operating system.
 *
 * There is a moment between closing this socket and `uf dev` binding the port
 * in which something else could take it. `uf dev --port` refuses to move to
 * another port rather than quietly listening one along, so that race is a loud
 * failure and never a measurement of the wrong server.
 */
export function freePort(): Promise<number> {
  return new Promise<number>((resolve, reject) => {
    const server = net.createServer();
    server.unref();
    server.on("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      const port = address != null && typeof address === "object" ? address.port : 0;
      server.close(() => {
        if (port === 0) {
          reject(new Error("the operating system handed out no port to start `uf dev` on"));
        } else {
          resolve(port);
        }
      });
    });
  });
}

/**
 * `fetch`, and the body, abandoned after `ms`.
 *
 * Not `AbortSignal.timeout`, which the Flow library definitions do not have
 * yet; this is the four lines it would have saved.
 */
async function fetchText(
  url: string,
  headers: { [string]: string },
  ms: number,
): Promise<{ readonly status: number, readonly body: string }> {
  const controller = new AbortController();
  const timer = setTimeout(() => {
    controller.abort();
  }, ms);
  try {
    const response = await fetch(url, { headers, signal: controller.signal });
    return { status: response.status, body: await response.text() };
  } finally {
    clearTimeout(timer);
  }
}

export type DevServer = {
  readonly port: number,
  readonly startedAt: number,
  readonly exited: () => number | null,
  readonly log: () => string,
  readonly stop: () => Promise<void>,
};

/** Start a long-running server in a process group of its own. */
export function startDevServer(
  program: string,
  args: $ReadOnlyArray<string>,
  options: { readonly cwd: string, readonly env: Environment, readonly port: number },
): DevServer {
  const startedAt = performance.now();
  const child: Child = spawn(program, [...args], {
    cwd: options.cwd,
    env: options.env,
    detached: true,
    stdio: ["ignore", "pipe", "pipe"],
  });
  live.add(child);
  let output = "";
  const keep = (chunk: string) => {
    output = `${output}${chunk}`.slice(-64_000);
  };
  child.stdout?.setEncoding("utf8");
  child.stderr?.setEncoding("utf8");
  child.stdout?.on("data", keep);
  child.stderr?.on("data", keep);
  let exitCode: number | null = null;
  const exited = new Promise<void>((resolve) => {
    child.on("exit", (code: mixed) => {
      exitCode = typeof code === "number" ? code : -1;
      resolve();
    });
  });
  return {
    port: options.port,
    startedAt,
    exited: () => exitCode,
    log: () => output,
    stop: async () => {
      if (exitCode == null) {
        signalGroup(child, "SIGTERM");
        // A timer that is cleared once the race is decided: an uncleared one
        // would hold this process open for ten seconds after the last server
        // had already gone.
        let timer = null;
        await Promise.race([
          exited,
          new Promise<void>((resolve) => {
            timer = setTimeout(resolve, 10_000);
          }),
        ]);
        if (timer != null) {
          clearTimeout(timer);
        }
      }
      // uf has gone; give the rest of its group — the driver, Vite — the same
      // few seconds to finish closing before it is killed, so a cache being
      // written on the way out is not cut in half for the next warm start.
      for (let waited = 0; waited < 5_000 && groupAlive(child); waited += 50) {
        await sleep(50);
      }
      signalGroup(child, "SIGKILL");
      live.delete(child);
    },
  };
}

/** The last lines of a log, which is the part that says what went wrong. */
export function tail(text: string, lines: number = 30): string {
  return text.trim().split("\n").slice(-lines).join("\n");
}

/**
 * Poll `/` until it answers `200`, and time it from the server's spawn.
 *
 * `Accept: text/html`, because that is what a navigation sends and uf renders a
 * document only for a request that accepts one: anything else is a module
 * request, and a bare `curl` gets a deliberate 404. And `200` rather than any
 * answer, because the server answers before its routes are wired.
 */
export async function waitForDocument(
  server: DevServer,
  timeoutMs: number,
): Promise<{ readonly ms: number, readonly body: string }> {
  const deadline = server.startedAt + timeoutMs;
  while (performance.now() < deadline) {
    const code = server.exited();
    if (code != null) {
      throw new Error(
        `\`uf dev\` exited with ${String(code)} before it served a document:\n${tail(server.log())}`,
      );
    }
    try {
      const answer = await fetchText(
        `http://127.0.0.1:${String(server.port)}/`,
        { accept: "text/html" },
        10_000,
      );
      if (answer.status === 200) {
        return { ms: performance.now() - server.startedAt, body: answer.body };
      }
    } catch {
      // Not listening yet, which is most of what this loop waits through.
    }
    await sleep(10);
  }
  throw new Error(
    `\`uf dev\` served no 200 for / within ${String(timeoutMs / 1000)} s:\n${tail(server.log())}`,
  );
}

/** A module specifier in served JavaScript: static, re-exported or dynamic. */
const SPECIFIER =
  /\b(?:import|export)\b[^"'`;]*?["']([^"'\n]+)["']|\bimport\s*\(\s*["']([^"'\n]+)["']/g;
const SCRIPT = /<script\b([^>]*)>([\s\S]*?)<\/script>/g;

/** How many modules the crawl follows before it concludes the graph is not a page's. */
const CRAWL_LIMIT = 5_000;

function specifiers(code: string): Array<string> {
  const found = [];
  for (const match of code.matchAll(SPECIFIER)) {
    const specifier = match[1] ?? match[2];
    if (specifier != null) {
      found.push(specifier);
    }
  }
  return found;
}

/**
 * Request the page's client module graph, the way the browser would.
 *
 * This is what makes Vite hot-update a module at all: its module graph is built
 * from what was requested, and a component nothing has asked for is a component
 * Vite has no reason to tell anybody about. Everything the document loads as a
 * module is followed — `src`s, the imports inside inline module scripts, and
 * then every import in what those serve. Vite rewrites imports to absolute
 * paths, so a specifier that is not a path is a bare name inside a string and
 * is skipped. A request that fails is skipped too: served code is scanned, not
 * parsed, and a browser would not have executed the line it came from.
 *
 * Returns every path requested.
 */
async function crawl(base: string, document: string): Promise<Set<string>> {
  const origin = new URL(base).origin;
  const entries: Array<URL> = [];
  for (const match of document.matchAll(SCRIPT)) {
    const attributes = match[1];
    if (!/\btype="module"/.test(attributes)) {
      continue;
    }
    const source = attributes.match(/\bsrc="([^"]+)"/);
    if (source != null) {
      entries.push(new URL(source[1], `${base}/`));
    } else {
      for (const specifier of specifiers(match[2])) {
        entries.push(new URL(specifier, `${base}/`));
      }
    }
  }
  const seen: Set<string> = new Set();
  const paths: Set<string> = new Set();
  let queue = entries;
  while (queue.length > 0 && seen.size < CRAWL_LIMIT) {
    const batch = [];
    for (const url of queue) {
      const key = `${url.pathname}${url.search}`;
      if (url.origin === origin && !seen.has(key)) {
        seen.add(key);
        paths.add(url.pathname);
        batch.push(url);
      }
    }
    queue = [];
    for (let at = 0; at < batch.length; at += 16) {
      const bodies = await Promise.all(
        batch.slice(at, at + 16).map(async (url) => {
          try {
            const answer = await fetchText(url.href, {}, 30_000);
            return { url, body: answer.status === 200 ? answer.body : "" };
          } catch {
            return { url, body: "" };
          }
        }),
      );
      for (const { url, body } of bodies) {
        for (const specifier of specifiers(body)) {
          if (specifier.startsWith("/") || specifier.startsWith(".")) {
            queue.push(new URL(specifier, url));
          }
        }
      }
    }
  }
  return paths;
}

type Update = {
  readonly at: number,
  readonly path: string,
  readonly timestamp: number,
  readonly explicitImportRequired: boolean,
};

/** The update `message` carries for `urlPath`, an error it reports, or nothing. */
function updateFor(message: mixed, urlPath: string): Update | Error | null {
  if (message == null || typeof message !== "object" || Array.isArray(message)) {
    return null;
  }
  if (message.type === "full-reload") {
    return new Error(
      `Vite asked the page to reload instead of hot-updating it (${JSON.stringify(message)}). ` +
        "An edit to a string in a client component is expected to hot-update, so the time " +
        "measured would be a reload's and not an update's",
    );
  }
  if (message.type === "error") {
    return new Error(`Vite reported an error for the edit: ${JSON.stringify(message)}`);
  }
  const updates = message.updates;
  if (message.type !== "update" || !Array.isArray(updates)) {
    return null;
  }
  for (const update of updates) {
    if (update == null || typeof update !== "object" || Array.isArray(update)) {
      continue;
    }
    if (update.path !== urlPath && update.acceptedPath !== urlPath) {
      continue;
    }
    return {
      at: performance.now(),
      path: typeof update.acceptedPath === "string" ? update.acceptedPath : urlPath,
      timestamp: typeof update.timestamp === "number" ? update.timestamp : Date.now(),
      explicitImportRequired: update.explicitImportRequired === true,
    };
  }
  return null;
}

export type HmrSample = { readonly messageMs: number, readonly appliedMs: number };

type Waiter = {
  readonly resolve: (update: Update) => void,
  readonly reject: (error: Error) => void,
};

/**
 * Edit `file` `edits` times against a running dev server and time each edit.
 *
 * `messageMs` is from the write to Vite's `update` naming the file on the HMR
 * socket. `appliedMs` is from the write to the edited module having been served
 * from `<acceptedPath>?t=<timestamp>` — the URL `/@vite/client` imports after
 * that message — and the body is checked for the new string, so a stale module
 * served from a cache fails the run instead of being fast.
 *
 * The socket needs the token Vite writes into `/@vite/client`, which is how
 * Vite refuses sockets from pages it did not serve. Reading it from there is
 * exactly what the page's own client does.
 */
export async function measureHmr(options: {
  readonly port: number,
  readonly document: string,
  readonly file: string,
  readonly urlPath: string,
  readonly marker: string,
  readonly edits: number,
  readonly timeoutMs: number,
  readonly settleMs: number,
}): Promise<Array<HmrSample>> {
  const base = `http://127.0.0.1:${String(options.port)}`;
  const client = await fetchText(`${base}/@vite/client`, {}, options.timeoutMs);
  const token = client.body.match(/const wsToken = "([^"]*)"/)?.[1];
  if (token == null) {
    throw new Error(
      "`/@vite/client` has no `wsToken` in it. Vite's client has changed shape, and the " +
        "HMR stage has to learn the new way to open the socket before it can measure anything",
    );
  }

  const socket = new WebSocket(
    `ws://127.0.0.1:${String(options.port)}/?token=${encodeURIComponent(token)}`,
    "vite-hmr",
  );
  const pending: { current: Waiter | null } = { current: null };
  socket.addEventListener("message", (event: MessageEvent) => {
    const waiter = pending.current;
    if (waiter == null) {
      return;
    }
    let message: mixed = null;
    try {
      message = JSON.parse(String(event.data));
    } catch {
      return;
    }
    const found = updateFor(message, options.urlPath);
    if (found instanceof Error) {
      waiter.reject(found);
    } else if (found != null) {
      waiter.resolve(found);
    }
  });
  await new Promise<void>((resolve, reject) => {
    const timer = setTimeout(() => {
      reject(new Error("Vite's HMR socket did not open"));
    }, options.timeoutMs);
    socket.addEventListener("open", () => {
      clearTimeout(timer);
      resolve();
    });
    socket.addEventListener("error", () => {
      clearTimeout(timer);
      reject(new Error("Vite refused the HMR socket"));
    });
  });

  const original = fs.readFileSync(options.file, "utf8");
  const quoted = `"${options.marker}"`;
  if (!original.includes(quoted)) {
    socket.close();
    throw new Error(`${options.file} has no ${quoted} in it for the HMR stage to rewrite`);
  }

  const samples: Array<HmrSample> = [];
  try {
    const graph = await crawl(base, options.document);
    if (!graph.has(options.urlPath)) {
      throw new Error(
        `the page's client module graph (${String(graph.size)} modules) does not reach ` +
          `${options.urlPath}, so Vite has no reason to hot-update it; the home page has to render it`,
      );
    }
    for (let edit = 1; edit <= options.edits; edit += 1) {
      const marker = `hmr-${String(edit)}`;
      let timer = null;
      const received = new Promise<Update>((resolve, reject) => {
        pending.current = { resolve, reject };
        timer = setTimeout(() => {
          reject(
            new Error(
              `no HMR update for ${options.urlPath} arrived within ` +
                `${String(options.timeoutMs / 1000)} s of edit ${String(edit)}`,
            ),
          );
        }, options.timeoutMs);
      });
      const editedAt = performance.now();
      fs.writeFileSync(options.file, original.replace(quoted, `"${marker}"`));
      const update = await received;
      pending.current = null;
      if (timer != null) {
        clearTimeout(timer);
      }
      const query = `${update.explicitImportRequired ? "import&" : ""}t=${String(update.timestamp)}`;
      const answer = await fetchText(`${base}${update.path}?${query}`, {}, options.timeoutMs);
      const appliedAt = performance.now();
      if (answer.status !== 200 || !answer.body.includes(marker)) {
        throw new Error(
          `after edit ${String(edit)}, ${update.path}?${query} answered ${String(answer.status)} ` +
            `without "${marker}" in it: the page would have been given a stale module`,
        );
      }
      samples.push({ messageMs: update.at - editedAt, appliedMs: appliedAt - editedAt });
      // Let the edit's other consequences — uf's debounced re-analysis, the
      // watcher's trailing events — finish before the next edit is timed.
      await sleep(options.settleMs);
    }
  } finally {
    pending.current = null;
    fs.writeFileSync(options.file, original);
    socket.close();
  }
  return samples;
}

export type Quietness = {
  readonly load: $ReadOnlyArray<number>,
  readonly rustc: number,
  readonly quiet: boolean,
};

/** The one-minute load average a quiet machine stays under. */
const QUIET_LOAD = 1.5;

/** How long the machine has to have stayed quiet before a run may start. */
const QUIET_FOR_MS = 60_000;

/** How many `rustc` processes are running, or -1 when `ps` could not say. */
function rustcProcesses(): number {
  const listed = spawnSync("ps", ["-A", "-o", "comm="], { encoding: "utf8" });
  if (listed.status !== 0) {
    return -1;
  }
  return String(listed.stdout)
    .split("\n")
    .filter((line) => path.basename(line.trim()) === "rustc").length;
}

/**
 * Whether the machine is quiet now.
 *
 * A compile anywhere on the machine is the usual reason a number is wrong, and
 * the load average lags it, so `rustc` is counted by name as well.
 */
export function quietness(): Quietness {
  const load = os.loadavg().map((value) => Number(value.toFixed(2)));
  const rustc = rustcProcesses();
  return { load, rustc, quiet: load[0] < QUIET_LOAD && rustc === 0 };
}

/** Wait for a minute of quiet, or fail with a sentence after `timeoutMs`. */
export async function waitForQuiet(timeoutMs: number): Promise<Quietness> {
  const startedAt = performance.now();
  let quietSince: number | null = null;
  let now = quietness();
  while (performance.now() - startedAt <= timeoutMs) {
    now = quietness();
    const at = performance.now();
    if (!now.quiet) {
      quietSince = null;
    } else if (quietSince == null) {
      quietSince = at;
    } else if (at - quietSince >= QUIET_FOR_MS) {
      return now;
    }
    await sleep(5_000);
  }
  throw new Error(
    `the machine did not stay quiet for a minute within ${String(Math.round(timeoutMs / 60_000))} ` +
      `minutes (load ${now.load.join(" ")}, ${String(now.rustc)} rustc). A number measured ` +
      "now would be a number about that other work: run again when it has finished, or run " +
      "without --require-quiet and publish nothing from the result",
  );
}
