// @flow
//
// Production HTTP comparison for #1363. This runs on the same CI runner and
// generated application as the toolchain benchmark, with three route shapes.
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import { generateFixture, linkDependencies, presetNamed } from "./fixture.js";
import { freePort, run, startDevServer, waitForDocument } from "./measure.js";
import type { DevServer } from "./measure.js";
import { mirrorInto, rivalFiles, RIVALS_DIR, writeFiles } from "./rivals.js";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");
const uf = path.resolve(process.env.UF_BINARY ?? path.join(repo, "target/release/uf"));
const preset = presetNamed(process.env.UF_BENCH_PRESET ?? "small");
const durationMs = Number(process.env.UF_BENCH_SSR_DURATION_MS ?? 10_000);
const concurrency = Number(process.env.UF_BENCH_SSR_CONCURRENCY ?? 16);
const work = path.resolve(
  process.env.UF_BENCH_SSR_WORK_DIR ?? path.join(os.tmpdir(), "uf-bench-ssr"),
);
const out = path.resolve(process.env.UF_BENCH_SSR_OUT ?? path.join(work, "results.json"));

if (
  !Number.isInteger(durationMs) ||
  durationMs < 1_000 ||
  !Number.isInteger(concurrency) ||
  concurrency < 1
) {
  throw new Error(
    "UF_BENCH_SSR_DURATION_MS must be at least 1000 and UF_BENCH_SSR_CONCURRENCY must be positive",
  );
}
if (work === repo || work.startsWith(`${repo}${path.sep}`)) {
  throw new Error("the SSR benchmark work directory must be outside this repository");
}

/** One page of the request mix, and the text that proves it rendered. */
type Route = { readonly name: string, readonly path: string, readonly marker: string };

/** One route's numbers for one server. */
type Row = {
  readonly route: string,
  readonly requests: number,
  readonly elapsedMs: number,
  readonly requestsPerSecond: number,
  readonly p50Ms: number,
  readonly p99Ms: number,
  readonly peakRssBytes: number | null,
};

type ToolRow = { ...Row, readonly tool: string };

/** A column of the comparison: which number, and which direction is a win. */
type Metric = {
  readonly label: string,
  readonly value: (row: ToolRow) => number | null,
  readonly higherIsBetter: boolean,
};

const routes: $ReadOnlyArray<Route> = [
  { name: "static", path: "/r001", marker: "Route 1" },
  { name: "dynamic", path: "/bench-dynamic", marker: "dynamic benchmark" },
  { name: "streamed", path: "/bench-stream", marker: "streamed benchmark" },
];

const ufDynamic = `// @flow
export const dynamic = "force-dynamic";
export component Page() { return <main>dynamic benchmark</main>; }
`;
const nextDynamic = `export const dynamic = "force-dynamic";
export default function Page() { return <main>dynamic benchmark</main>; }
`;
const ufStream = `// @flow
import * as React from "@uniflowed/react";
export const dynamic = "force-dynamic";
async function Slow(): Promise<React.MixedElement> {
  await new Promise<void>((resolve) => setTimeout(resolve, 5));
  return <p>streamed benchmark</p>;
}
export component Page() {
  return <React.Suspense fallback={<p>streaming fallback</p>}><Slow /></React.Suspense>;
}
`;
const nextStream = `import { Suspense } from "react";
export const dynamic = "force-dynamic";
async function Slow() {
  await new Promise<void>((resolve) => setTimeout(resolve, 5));
  return <p>streamed benchmark</p>;
}
export default function Page() {
  return <Suspense fallback={<p>streaming fallback</p>}><Slow /></Suspense>;
}
`;

function write(dir: string, name: string, contents: string): void {
  const file = path.join(dir, name);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, contents);
}

function rssTree(pid: ?number, seen: Set<number> = new Set()): number {
  if (pid == null || seen.has(pid) || process.platform !== "linux") return 0;
  seen.add(pid);
  let bytes = 0;
  try {
    const status = fs.readFileSync(`/proc/${pid}/status`, "utf8");
    bytes = Number(status.match(/^VmRSS:\s+(\d+) kB/m)?.[1] ?? 0) * 1024;
    const children = fs.readFileSync(`/proc/${pid}/task/${pid}/children`, "utf8").trim();
    for (const child of children.split(/\s+/).filter(Boolean)) {
      bytes += rssTree(Number(child), seen);
    }
  } catch {
    // A process may exit between readings. The next sample sees its parent.
  }
  return bytes;
}

function percentile(sorted: $ReadOnlyArray<number>, fraction: number): number {
  return sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * fraction) - 1)];
}

async function sample(server: DevServer, route: Route): Promise<number> {
  const started = performance.now();
  const response = await fetch(`http://127.0.0.1:${server.port}${route.path}`, {
    headers: { accept: "text/html" },
    signal: AbortSignal.timeout(30_000),
  });
  const html = await response.text();
  if (response.status !== 200 || !html.includes(route.marker)) {
    throw new Error(
      `${server.label} ${route.path}: ${response.status}, expected ${route.marker}; ${html.slice(
        0,
        200,
      )}`,
    );
  }
  return performance.now() - started;
}

async function load(server: DevServer): Promise<Array<Row>> {
  // Both servers see the same round-robin request mix at the same concurrency.
  await Promise.all(
    Array.from({ length: concurrency }, (_, index) =>
      sample(server, routes[index % routes.length]),
    ),
  );
  // One list per route, in `routes`' order.
  const latencies: Array<Array<number>> = routes.map(() => []);
  let requestNumber = 0;
  let peakRssBytes = 0;
  const ticker = setInterval(() => {
    peakRssBytes = Math.max(peakRssBytes, rssTree(server.pid));
  }, 50);
  const started = performance.now();
  const deadline = started + durationMs;
  try {
    await Promise.all(
      Array.from({ length: concurrency }, async () => {
        while (performance.now() < deadline) {
          const index = requestNumber % routes.length;
          requestNumber += 1;
          latencies[index].push(await sample(server, routes[index]));
        }
      }),
    );
  } finally {
    clearInterval(ticker);
  }
  const elapsedMs = performance.now() - started;
  const all = latencies.flat().sort((a, b) => a - b);
  const rows = routes.map((route, index): Row => {
    const samples = latencies[index];
    samples.sort((a, b) => a - b);
    if (samples.length === 0) {
      throw new Error(`${server.label} ${route.path}: no completed requests`);
    }
    return {
      route: route.name,
      requests: samples.length,
      elapsedMs,
      requestsPerSecond: (samples.length * 1_000) / elapsedMs,
      p50Ms: percentile(samples, 0.5),
      p99Ms: percentile(samples, 0.99),
      peakRssBytes: process.platform === "linux" ? peakRssBytes : null,
    };
  });
  rows.unshift({
    route: "mixed",
    requests: all.length,
    elapsedMs,
    requestsPerSecond: (all.length * 1_000) / elapsedMs,
    p50Ms: percentile(all, 0.5),
    p99Ms: percentile(all, 0.99),
    peakRssBytes: process.platform === "linux" ? peakRssBytes : null,
  });
  return rows;
}

async function checkedBuild(
  program: string,
  args: $ReadOnlyArray<string>,
  dir: string,
  label: string,
): Promise<void> {
  const result = await run(program, args, {
    cwd: dir,
    env: process.env,
    timeoutMs: 10 * 60_000,
  });
  if (result.code !== 0) {
    throw new Error(`${label} failed:\n${result.stdout}\n${result.stderr}`);
  }
}

async function measure(
  tool: string,
  program: string,
  args: (port: number) => $ReadOnlyArray<string>,
  dir: string,
): Promise<Array<ToolRow>> {
  const port = await freePort();
  const server = startDevServer(program, args(port), {
    cwd: dir,
    env: { ...process.env, NEXT_TELEMETRY_DISABLED: "1" },
    port,
    label: `${tool} start`,
  });
  try {
    await waitForDocument(server, 120_000);
    process.stderr.write(`${tool} static/dynamic/streamed mix...\n`);
    return (await load(server)).map((row) => ({ tool, ...row }));
  } finally {
    await server.stop();
  }
}

async function main(): Promise<void> {
  fs.rmSync(work, { recursive: true, force: true });
  fs.mkdirSync(work, { recursive: true });
  const ufDir = path.join(work, "uf");
  const nextDir = path.join(work, "next");
  const version = JSON.parse(
    fs.readFileSync(path.join(repo, "packages/react/package.json"), "utf8"),
  ).version;
  const react = JSON.parse(
    fs.readFileSync(path.join(repo, "node_modules/react/package.json"), "utf8"),
  ).version;
  generateFixture(ufDir, preset, { uniflowed: version, react });
  linkDependencies(ufDir, repo);
  write(ufDir, "app/bench-dynamic/$page.js", ufDynamic);
  write(ufDir, "app/bench-stream/$page.js", ufStream);

  writeFiles(nextDir, rivalFiles(preset, "next"));
  mirrorInto(nextDir, path.join(repo, RIVALS_DIR, "node_modules"), ["flow-bin"]);
  write(nextDir, "app/bench-dynamic/page.tsx", nextDynamic);
  write(nextDir, "app/bench-stream/page.tsx", nextStream);
  const next = path.join(nextDir, "node_modules/.bin/next");

  await checkedBuild(uf, ["build", "--adapter", "node"], ufDir, "uf build");
  await checkedBuild(next, ["build"], nextDir, "next build");
  const rows = [
    ...(await measure(
      "uf",
      uf,
      (port) => ["start", "--host", "127.0.0.1", "--port", String(port)],
      ufDir,
    )),
    ...(await measure(
      "next",
      next,
      (port) => ["start", "--hostname", "127.0.0.1", "--port", String(port)],
      nextDir,
    )),
  ];
  const report = {
    schema: 1,
    fixture: preset,
    commit: process.env.GITHUB_SHA ?? null,
    machine: {
      platform: process.platform,
      arch: process.arch,
      cpu: os.cpus()[0]?.model ?? "unknown",
    },
    settings: { durationMs, concurrency, warmupRequests: concurrency },
    rows,
  };
  fs.mkdirSync(path.dirname(out), { recursive: true });
  fs.writeFileSync(out, `${JSON.stringify(report, null, 2)}\n`);
  const lines = [
    "| route | tool | req/s | p50 ms | p99 ms | peak RSS MiB |",
    "| --- | --- | ---: | ---: | ---: | ---: |",
  ];
  for (const routeName of ["mixed", ...routes.map((route) => route.name)]) {
    const pair = rows.filter((row) => row.route === routeName);
    for (const row of pair) {
      lines.push(
        `| ${routeName} | ${row.tool} | ${row.requestsPerSecond.toFixed(1)} | ${row.p50Ms.toFixed(
          1,
        )} | ${row.p99Ms.toFixed(1)} | ${
          row.peakRssBytes == null ? "-" : (row.peakRssBytes / 2 ** 20).toFixed(1)
        } |`,
      );
    }
  }
  const metrics: $ReadOnlyArray<Metric> = [
    { label: "req/s", value: (row) => row.requestsPerSecond, higherIsBetter: true },
    { label: "p50 ms", value: (row) => row.p50Ms, higherIsBetter: false },
    { label: "p99 ms", value: (row) => row.p99Ms, higherIsBetter: false },
  ];
  // Memory is sampled per server rather than per route, so only the mix has it.
  const rss: Metric = {
    label: "peak RSS MiB",
    value: (row) => row.peakRssBytes,
    higherIsBetter: false,
  };
  const differences: Array<{ route: string, metric: string, improvement: number }> = [];
  for (const routeName of ["mixed", ...routes.map((route) => route.name)]) {
    const ours = rows.find((row) => row.route === routeName && row.tool === "uf");
    const next = rows.find((row) => row.route === routeName && row.tool === "next");
    if (ours == null || next == null) {
      throw new Error(`${routeName}: both servers must report every route`);
    }
    for (const { label, value, higherIsBetter } of routeName === "mixed"
      ? [...metrics, rss]
      : metrics) {
      const mine = value(ours);
      const theirs = value(next);
      if (mine == null || theirs == null) continue;
      const improvement = (higherIsBetter ? mine - theirs : theirs - mine) / theirs;
      differences.push({ route: routeName, metric: label, improvement });
    }
  }
  // Negative percentages are uf's losses, shown before its wins.
  differences.sort((a, b) => a.improvement - b.improvement);
  lines.push("", "| uf vs Next | metric | uf advantage |", "| --- | --- | ---: |");
  for (const difference of differences) {
    const percent = difference.improvement * 100;
    lines.push(
      `| ${difference.route} | ${difference.metric} | ${percent >= 0 ? "+" : ""}${percent.toFixed(1)}% |`,
    );
  }
  const table = `${lines.join("\n")}\n`;
  process.stdout.write(table);
  if (process.env.GITHUB_STEP_SUMMARY) {
    fs.appendFileSync(
      process.env.GITHUB_STEP_SUMMARY,
      `## Production SSR (uf start / next start)\n\n${table}`,
    );
  }
}

main().catch((error) => {
  process.stderr.write(`${error instanceof Error ? error.stack : String(error)}\n`);
  process.exitCode = 1;
});
