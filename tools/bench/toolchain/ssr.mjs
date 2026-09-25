// Production HTTP comparison for #1363. This runs on the same CI runner and
// generated application as the toolchain benchmark, with three route shapes.
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import { generateFixture, linkDependencies, presetNamed } from "./fixture.js";
import { freePort, run, startDevServer, waitForDocument } from "./measure.js";
import { mirrorInto, rivalFiles, RIVALS_DIR, writeFiles } from "./rivals.js";

const repo = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../../..",
);
const uf = path.resolve(
  process.env.UF_BINARY ?? path.join(repo, "target/release/uf"),
);
const preset = presetNamed(process.env.UF_BENCH_PRESET ?? "small");
const durationMs = Number(process.env.UF_BENCH_SSR_DURATION_MS ?? 10_000);
const concurrency = Number(process.env.UF_BENCH_SSR_CONCURRENCY ?? 16);
const work = path.resolve(
  process.env.UF_BENCH_SSR_WORK_DIR ?? path.join(os.tmpdir(), "uf-bench-ssr"),
);
const out = path.resolve(
  process.env.UF_BENCH_SSR_OUT ?? path.join(work, "results.json"),
);

if (
  !Number.isInteger(durationMs) || durationMs < 1_000 ||
  !Number.isInteger(concurrency) || concurrency < 1
) {
  throw new Error(
    "UF_BENCH_SSR_DURATION_MS must be at least 1000 and UF_BENCH_SSR_CONCURRENCY must be positive",
  );
}
if (work === repo || work.startsWith(`${repo}${path.sep}`)) {
  throw new Error(
    "the SSR benchmark work directory must be outside this repository",
  );
}

const routes = [
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
async function Slow(): Promise<React.Node> {
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

function write(dir, name, contents) {
  const file = path.join(dir, name);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, contents);
}

function rssTree(pid, seen = new Set()) {
  if (pid == null || seen.has(pid) || process.platform !== "linux") return 0;
  seen.add(pid);
  let bytes = 0;
  try {
    const status = fs.readFileSync(`/proc/${pid}/status`, "utf8");
    bytes = Number(status.match(/^VmRSS:\s+(\d+) kB/m)?.[1] ?? 0) * 1024;
    const children = fs.readFileSync(
      `/proc/${pid}/task/${pid}/children`,
      "utf8",
    ).trim();
    for (const child of children.split(/\s+/).filter(Boolean)) {
      bytes += rssTree(Number(child), seen);
    }
  } catch {
    // A process may exit between readings. The next sample sees its parent.
  }
  return bytes;
}

function percentile(sorted, fraction) {
  return sorted[
    Math.min(sorted.length - 1, Math.ceil(sorted.length * fraction) - 1)
  ];
}

async function sample(server, route) {
  const started = performance.now();
  const response = await fetch(`http://127.0.0.1:${server.port}${route.path}`, {
    headers: { accept: "text/html" },
    signal: AbortSignal.timeout(30_000),
  });
  const html = await response.text();
  if (response.status !== 200 || !html.includes(route.marker)) {
    throw new Error(
      `${server.label} ${route.path}: ${response.status}, expected ${route.marker}; ${
        html.slice(0, 200)
      }`,
    );
  }
  return performance.now() - started;
}

async function load(server) {
  // Both servers see the same round-robin request mix at the same concurrency.
  await Promise.all(
    Array.from(
      { length: concurrency },
      (_, index) => sample(server, routes[index % routes.length]),
    ),
  );
  const latencies = new Map(routes.map((route) => [route.name, []]));
  let requestNumber = 0;
  let peakRssBytes = 0;
  const ticker = setInterval(() => {
    peakRssBytes = Math.max(peakRssBytes, rssTree(server.pid));
  }, 50);
  const started = performance.now();
  const deadline = started + durationMs;
  try {
    await Promise.all(Array.from({ length: concurrency }, async () => {
      while (performance.now() < deadline) {
        const route = routes[requestNumber % routes.length];
        requestNumber += 1;
        latencies.get(route.name).push(await sample(server, route));
      }
    }));
  } finally {
    clearInterval(ticker);
  }
  const elapsedMs = performance.now() - started;
  const all = [...latencies.values()].flat().sort((a, b) => a - b);
  const rows = routes.map((route) => {
    const samples = latencies.get(route.name);
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

async function checkedBuild(program, args, dir, label) {
  const result = await run(program, args, {
    cwd: dir,
    env: process.env,
    timeoutMs: 10 * 60_000,
  });
  if (result.code !== 0) {
    throw new Error(`${label} failed:\n${result.stdout}\n${result.stderr}`);
  }
}

async function measure(tool, program, args, dir) {
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

async function main() {
  fs.rmSync(work, { recursive: true, force: true });
  fs.mkdirSync(work, { recursive: true });
  const ufDir = path.join(work, "uf");
  const nextDir = path.join(work, "next");
  const version = JSON.parse(
    fs.readFileSync(path.join(repo, "packages/react/package.json"), "utf8"),
  ).version;
  const react = JSON.parse(
    fs.readFileSync(
      path.join(repo, "node_modules/react/package.json"),
      "utf8",
    ),
  ).version;
  generateFixture(ufDir, preset, { uniflowed: version, react });
  linkDependencies(ufDir, repo);
  write(ufDir, "app/bench-dynamic/$page.js", ufDynamic);
  write(ufDir, "app/bench-stream/$page.js", ufStream);

  writeFiles(nextDir, rivalFiles(preset, "next"));
  mirrorInto(nextDir, path.join(repo, RIVALS_DIR, "node_modules"), [
    "flow-bin",
  ]);
  write(nextDir, "app/bench-dynamic/page.tsx", nextDynamic);
  write(nextDir, "app/bench-stream/page.tsx", nextStream);
  const next = path.join(nextDir, "node_modules/.bin/next");

  await checkedBuild(uf, ["build", "--adapter", "node"], ufDir, "uf build");
  await checkedBuild(next, ["build"], nextDir, "next build");
  const rows = [
    ...await measure(
      "uf",
      uf,
      (port) => ["start", "--host", "127.0.0.1", "--port", String(port)],
      ufDir,
    ),
    ...await measure(
      "next",
      next,
      (port) => ["start", "--hostname", "127.0.0.1", "--port", String(port)],
      nextDir,
    ),
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
    // The slower throughput is first, so losses are visible before wins.
    pair.sort((a, b) => a.requestsPerSecond - b.requestsPerSecond);
    for (const row of pair) {
      lines.push(
        `| ${routeName} | ${row.tool} | ${row.requestsPerSecond.toFixed(1)} | ${
          row.p50Ms.toFixed(1)
        } | ${row.p99Ms.toFixed(1)} | ${
          row.peakRssBytes == null
            ? "-"
            : (row.peakRssBytes / 2 ** 20).toFixed(1)
        } |`,
      );
    }
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
  process.stderr.write(
    `${error instanceof Error ? error.stack : String(error)}\n`,
  );
  process.exitCode = 1;
});
