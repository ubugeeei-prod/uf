// @flow
//
// Every uf command, timed on an application of a stated size, and the tools it
// is compared with timed on the same application.
//
//   uf run bench:toolchain                            # uf alone, the small preset
//   uf run bench:toolchain --tools all                # and every comparison tool
//   uf run bench:toolchain --preset large --runs 10
//   uf run bench:toolchain --preset all --require-quiet --out results.json
//
// # Why this exists
//
// ubugeeei-prod/uf#945. The manual quoted numbers from separate one-off runs,
// each on its own input, with nothing anybody could run again — so a regression
// went unnoticed, and nobody could say which gap to close next. This is one
// command that produces every number from generated fixtures on one machine
// under one set of rules, and writes the rules into the same file as the
// numbers. `report.js` documents that file, `regress.js` compares one with the
// baseline in this directory, and `docs/app/guide/benchmarks` renders it.
//
// # The tools
//
// `--tools` names them, `uf` alone unless it says otherwise, `all` for every
// one. Besides uf: Vite+ (`vp`), Next.js, Vitest, Bun, ESLint, Prettier,
// Biome, Flow, `tsc` and pnpm. `rivals.js` says what input each is given —
// its own idiomatic copy of the same application — and where each is found.
// A tool that is not installed is skipped by name, with the reason, in the
// table and in the file. Nothing is timed in its place.
//
// A row is keyed by tool, stage and fixture, so `uf/build.cold/small` and
// `next/build.cold/small` are the same stage of the same application measured
// with two tools, and a reader compares those.
//
// # What one run of each stage is
//
// Wall clock from spawning the command to it exiting, read in this process —
// never the total a command prints about itself, which starts after the binary
// has loaded and stops before it exits. And beside it, for every command that
// runs to completion, the CPU time of the command and every process under it
// (`measure.js` says how): a runner that spreads the same work over more cores
// is quicker by the clock and no cheaper, and on a machine doing other work
// the CPU is what there is less of.
//
//   fmt.cold, fmt.warm      `uf fmt --check`, on a tree `uf fmt` has formatted
//   lint.cold, lint.warm    `uf lint`
//   check.cold, check.warm  `uf check`: the linter, then Flow
//   test.cold, test.warm    `uf test`
//   build.cold, build.warm  `uf build`
//   dev.cold, dev.warm      `uf dev`, from spawning it to the first `200` for
//                           `/` requested with `Accept: text/html`
//   hmr.message             an edit to `components/HotCounter.js`, from the
//                           write to Vite's `update` message naming it
//   hmr.applied             the same edit, from the write to the edited module
//                           having been served at the URL the page imports it by
//   install.cold            `uf install` with no lockfile, no `node_modules` and
//                           an empty package-manager cache
//   install.warm            `uf install` with the lockfile and the cache the
//                           previous install left, and no `node_modules`
//
// The other tools' rows are the same stages with their own commands: `vp fmt
// --check`, `prettier --check .`, `eslint .`, `next build`, `bun test`, `pnpm
// install` and the rest, each listed in `rivals.js`.
//
// *Cold* removes a tool's caches from its copy before every run: `.uf`, `dist`
// and `node_modules/.vite` for uf; `.next` for Next.js; `node_modules/.vite`
// for Vite+ and Vitest. A tool that keeps no cache by default — Prettier,
// ESLint without `--cache`, Biome, `tsc` without `--incremental` — has nothing
// removed, and its cold and warm rows are the same command twice. *Warm*
// leaves what the run before it wrote: the second time you type the command.
// Both, because they regress differently — a slower transform shows in cold,
// and a cache that stopped hitting shows only in warm.
//
// # The test suite of many small files
//
// ubugeeei-prod/uf#944 measures a second shape of test suite, where the cost is
// starting workers rather than running tests: 50 files of 20 cases of two
// assertions. `tools/bench/testing/suite.js` writes it, once for each runner in
// that runner's idiom, and its rows are the `test` stages of the fixture
// called `suite`, for uf, Bun and Vitest.
//
// # Why the HMR stage has no browser in it
//
// A browser would make every number depend on which browser was installed and
// how fast it evaluates JavaScript. What the harness does instead is every part
// of a page's work that involves uf. It loads the document, reads the token Vite
// writes into `/@vite/client`, opens the HMR socket with it, and requests the
// page's client module graph the way the browser's module loader would — which
// is what makes Vite count `HotCounter.js` as loaded and hot-update it at all.
// Then it writes the file, waits for the `update` naming it, and fetches the
// module at `<path>?t=<timestamp>`, which is the request `/@vite/client` makes
// next. Left out is the browser evaluating that module and React re-rendering:
// the browser's time, not the toolchain's. `vp dev` is measured the same way;
// `next dev` speaks a protocol of its own and has no HMR rows.
//
// # What makes a number publishable
//
// A quiet machine. `--require-quiet` waits until the one-minute load average
// has been under 1.5 with no `rustc` running for a whole minute, and fails if
// that does not happen. Every file says whether it did (`provisional`), what the
// machine was, and every version involved. A number from a run that was not
// quiet is a number about the other work on the machine.
//
// # What makes a run fail rather than report
//
// Every command has to exit 0. And before anything is timed, `uf lint --json`
// and `uf check --json` have to have looked at every module the fixture has and
// found nothing wrong, and `uf test --json` has to have passed exactly the tests
// the fixture was written with: a linter that silently looked at nothing is
// fast, and a benchmark that timed it would be timing a bug. Every other test
// runner has to report passing exactly the tests its copy was written with,
// too, before it is timed.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import { GUIDE_PRESET, generateSuites } from "../testing/suite.js";
import {
  HOT_FILE,
  HOT_MARKER,
  PRESETS,
  fixtureFiles,
  generateFixture,
  installManifest,
  linkDependencies,
  presetNamed,
  refuseInsideRepository,
  requireInstall,
} from "./fixture.js";
import type { FixtureSummary, Preset, Versions } from "./fixture.js";
import {
  freePort,
  measureHmr,
  quietness,
  run,
  startDevServer,
  stopEverything,
  summarise,
  tail,
  waitForDocument,
  waitForQuiet,
} from "./measure.js";
import type { Environment, Finished, Quietness } from "./measure.js";
import { formatTable, writeReport } from "./report.js";
import type { InstallFixture, Machine, Report, Row, Skipped } from "./report.js";
import {
  DEV_RIVALS,
  INSTALL_RIVALS,
  ONE_SHOT_RIVALS,
  RIVALS_DIR,
  RIVAL_TOOLS,
  TOOL_NAMES,
  flowFiles,
  linkInto,
  locateTool,
  mirrorInto,
  rivalFiles,
  testsPassed,
  versionIn,
  writeFiles,
} from "./rivals.js";
import type { Copy, DevSpec, OneShotSpec } from "./rivals.js";

/** The repository root: this file is `tools/bench/toolchain/bench.js`. */
function repositoryRoot(): string {
  const url = import.meta.url;
  if (typeof url !== "string") {
    throw new Error(
      "import.meta.url is not a string here, so the benchmark cannot find its repository",
    );
  }
  return path.resolve(path.dirname(fileURLToPath(url)), "..", "..", "..");
}

const REPO = repositoryRoot();

const USAGE = `usage: uf run bench:toolchain [options]

  --preset small|large|all   the fixture to measure (small)
  --tools a,b|all            the tools to measure (uf); the others are
                             ${RIVAL_TOOLS.map((tool) => tool.name).join(", ")}
  --runs N                   timed runs of every stage (5)
  --warmup N                 runs made and thrown away before those (1)
  --hmr-edits N              timed edits in the HMR stage (10)
  --stages a,b               only these stages, by id or prefix: dev, build.cold
  --work-dir DIR             where fixtures are generated; outside the repository
                             (a directory under the system temporary directory)
  --out FILE                 where the JSON goes (<work-dir>/results.json)
  --require-quiet            wait for a quiet machine first, and fail without one
  --quiet-timeout MINUTES    how long to wait for it (15)

UF_BINARY names the uf to measure; \`uf run bench:toolchain\` builds one and sets it.
The comparison tools are pinned in ${RIVALS_DIR}: \`npm ci --prefix ${RIVALS_DIR}\`.`;

/** How long any one command may take before the run is abandoned. */
const COMMAND_TIMEOUT_MS = 10 * 60_000;
/** How long a dev server may take to serve its first document. */
const DEV_TIMEOUT_MS = 2 * 60_000;
/** How long an edit may take to arrive as an HMR update. */
const HMR_TIMEOUT_MS = 15_000;

type Options = {
  readonly presets: $ReadOnlyArray<Preset>,
  readonly tools: $ReadOnlyArray<string>,
  readonly runs: number,
  readonly warmup: number,
  readonly hmrEdits: number,
  readonly stages: $ReadOnlyArray<string> | null,
  readonly workDir: string,
  readonly out: string | null,
  readonly requireQuiet: boolean,
  readonly quietTimeoutMs: number,
};

function argument(flag: string, value: string | void): string {
  if (value == null || value.startsWith("--")) {
    throw new Error(`${flag} needs a value\n\n${USAGE}`);
  }
  return value;
}

function wholeNumber(flag: string, value: string, least: number): number {
  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed < least) {
    throw new Error(
      `${flag} takes a whole number no smaller than ${String(least)}, and was given ${value}\n\n${USAGE}`,
    );
  }
  return parsed;
}

function list(value: string): $ReadOnlyArray<string> {
  return value
    .split(",")
    .map((item) => item.trim())
    .filter((item) => item !== "");
}

function parseTools(value: string): $ReadOnlyArray<string> {
  if (value === "all") {
    return TOOL_NAMES;
  }
  const named = list(value);
  for (const tool of named) {
    if (!TOOL_NAMES.includes(tool)) {
      throw new Error(
        `--tools names ${tool}, which this benchmark does not know; the tools are ` +
          `${TOOL_NAMES.join(", ")}, or all`,
      );
    }
  }
  return TOOL_NAMES.filter((tool) => named.includes(tool));
}

function parseOptions(argv: $ReadOnlyArray<string>): Options {
  let presets: $ReadOnlyArray<Preset> = [presetNamed("small")];
  let tools: $ReadOnlyArray<string> = ["uf"];
  let runs = 5;
  let warmup = 1;
  let hmrEdits = 10;
  let stages: $ReadOnlyArray<string> | null = null;
  let workDir = path.join(os.tmpdir(), "uf-bench-toolchain");
  let out: string | null = null;
  let requireQuiet = false;
  let quietTimeoutMs = 15 * 60_000;
  for (let at = 0; at < argv.length; at += 1) {
    const flag = argv[at];
    if (flag === "--require-quiet") {
      requireQuiet = true;
      continue;
    }
    const value = argument(flag, argv[at + 1]);
    at += 1;
    switch (flag) {
      case "--preset":
        presets = value === "all" ? PRESETS : [presetNamed(value)];
        break;
      case "--tools":
        tools = parseTools(value);
        break;
      case "--runs":
        runs = wholeNumber(flag, value, 1);
        break;
      case "--warmup":
        warmup = wholeNumber(flag, value, 0);
        break;
      case "--hmr-edits":
        hmrEdits = wholeNumber(flag, value, 1);
        break;
      case "--stages":
        stages = list(value);
        break;
      case "--work-dir":
        workDir = path.resolve(value);
        break;
      case "--out":
        out = path.resolve(value);
        break;
      case "--quiet-timeout":
        quietTimeoutMs = wholeNumber(flag, value, 1) * 60_000;
        break;
      default:
        throw new Error(`${flag} is not an option of this benchmark\n\n${USAGE}`);
    }
  }
  return {
    presets,
    tools,
    runs,
    warmup,
    hmrEdits,
    stages,
    workDir,
    out,
    requireQuiet,
    quietTimeoutMs,
  };
}

/**
 * The environment every measured command runs in.
 *
 * `UF_BINARY` and `UF_PROJECT_ROOT` belong to this harness — the loader it runs
 * under reads them to transform these files — and a `uf` that inherited them
 * would be told about a project that is not the fixture. `NODE_OPTIONS` goes
 * for the reason any flag in a person's shell goes: it would be measured too.
 *
 * And the Node running this harness goes first on `PATH`. uf finds `node` there
 * to host the dev server, the tests and the build, so this puts every row on
 * one Node — the one `versions.node` names — instead of whatever a lookup
 * finds, which on a machine with Bun on `PATH` and no Node is Bun. Every other
 * Node tool is started through the same lookup.
 *
 * Next.js's telemetry is off. It sends a request about the build during the
 * build, and a row that timed a network request would be timing the network.
 */
function childEnvironment(): Environment {
  const env: { [string]: string | void } = { ...process.env };
  delete env.UF_BINARY;
  delete env.UF_PROJECT_ROOT;
  delete env.NODE_OPTIONS;
  env.PATH = [path.dirname(process.execPath), process.env.PATH ?? ""].join(path.delimiter);
  env.NEXT_TELEMETRY_DISABLED = "1";
  return env;
}

function binary(): string {
  const configured = process.env.UF_BINARY;
  if (configured == null || configured === "") {
    throw new Error(
      "UF_BINARY is not set. It names the uf to measure: `uf run bench:toolchain` builds " +
        "target/release/uf and sets it, and any other binary is measured with " +
        "`UF_BINARY=<path> node --import @uniflowed/host/register tools/bench/toolchain/bench.js`",
    );
  }
  const resolved = path.resolve(configured);
  if (!fs.existsSync(resolved)) {
    throw new Error(`UF_BINARY is ${resolved}, and there is nothing there to measure`);
  }
  return resolved;
}

function packageVersion(directory: string): string {
  try {
    const manifest: mixed = JSON.parse(
      fs.readFileSync(path.join(directory, "package.json"), "utf8"),
    );
    if (
      manifest != null &&
      typeof manifest === "object" &&
      !Array.isArray(manifest) &&
      typeof manifest.version === "string"
    ) {
      return manifest.version;
    }
  } catch {
    // Reported as unknown, which is what it is.
  }
  return "unknown";
}

async function printed(
  program: string,
  args: $ReadOnlyArray<string>,
  env: Environment,
): Promise<string> {
  try {
    const finished = await run(program, args, { cwd: REPO, env, timeoutMs: 60_000 });
    return finished.code === 0 ? `${finished.stdout}\n${finished.stderr}`.trim() : "unknown";
  } catch {
    return "unknown";
  }
}

async function collectVersions(uf: string, env: Environment): Promise<{ [string]: string }> {
  const relative = path.relative(REPO, uf);
  return {
    // `uf --version` prints `uf 0.0.0-alpha.32`, and the key is already the name.
    uf: (await printed(uf, ["--version"], env)).replace(/^uf\s+/, ""),
    commit: await printed("git", ["rev-parse", "HEAD"], env),
    // Where the binary came from: `target/release/uf` is a release build, and a
    // path outside the checkout is reported by name rather than leaking a home
    // directory into a file somebody may publish.
    binary: relative.startsWith("..") || path.isAbsolute(relative) ? path.basename(uf) : relative,
    node: process.version,
    npm: await printed(path.join(path.dirname(process.execPath), "npm"), ["--version"], env),
    vite: packageVersion(path.join(REPO, "node_modules", "vite")),
    react: packageVersion(path.join(REPO, "node_modules", "react")),
  };
}

function machine(before: Quietness, after: Quietness): Machine {
  const cpus = os.cpus();
  const ci =
    process.env.GITHUB_ACTIONS === "true"
      ? `github-actions ${process.env.RUNNER_NAME ?? "runner"} ` +
        `(${process.env.RUNNER_OS ?? "?"} ${process.env.RUNNER_ARCH ?? "?"}, ` +
        `${process.env.ImageOS ?? "?"} ${process.env.ImageVersion ?? "?"})`
      : null;
  return {
    platform: `${os.platform()} ${os.release()}`,
    arch: os.arch(),
    cpu: cpus.length > 0 ? cpus[0].model.trim() : "unknown",
    cores: cpus.length,
    memoryBytes: os.totalmem(),
    ci,
    before,
    after,
  };
}

/** What a cold run of a uf command removes from the fixture before it starts. */
const CACHES = [".uf", "dist", path.join("node_modules", ".vite")];

function clearCaches(dir: string, caches: $ReadOnlyArray<string>): void {
  for (const cache of caches) {
    fs.rmSync(path.join(dir, cache), { recursive: true, force: true });
  }
}

function describeCache(caches: $ReadOnlyArray<string>, cold: boolean): string {
  if (caches.length === 0) {
    return "keeps no cache by default, so cold and warm are the same run";
  }
  if (!cold) {
    return "keeps what the run before it wrote";
  }
  const named = caches.map((cache) => cache.split(path.sep).join("/"));
  const last = named[named.length - 1];
  const rest = named.slice(0, -1);
  return `removes ${rest.length === 0 ? last : `${rest.join(", ")} and ${last}`} before every run`;
}

function expectSuccess(command: string, dir: string, finished: Finished): void {
  const output = tail(`${finished.stdout}\n${finished.stderr}`);
  if (finished.timedOut) {
    throw new Error(
      `\`${command}\` in ${dir} was still running after ` +
        `${String(COMMAND_TIMEOUT_MS / 60_000)} minutes and was stopped:\n${output}`,
    );
  }
  if (finished.code !== 0) {
    throw new Error(
      `\`${command}\` in ${dir} exited with ${String(finished.code ?? finished.signal)}, ` +
        `so it has no time to report:\n${output}`,
    );
  }
}

function jsonObject(command: string, finished: Finished): { readonly [string]: mixed } {
  try {
    const value: mixed = JSON.parse(finished.stdout);
    if (value != null && typeof value === "object" && !Array.isArray(value)) {
      return value;
    }
  } catch {
    // Reported below, with what was printed instead.
  }
  throw new Error(
    `\`${command}\` did not print a JSON object:\n${tail(finished.stdout)}\n${tail(finished.stderr)}`,
  );
}

function numberIn(command: string, object: { readonly [string]: mixed }, key: string): number {
  const value = object[key];
  if (typeof value !== "number") {
    throw new Error(`\`${command}\` printed no number for "${key}"; its JSON has changed shape`);
  }
  return value;
}

type Context = {
  readonly env: Environment,
  readonly options: Options,
};

type RowFields = {
  readonly tool: string,
  readonly stage: string,
  readonly fixture: string,
  readonly title: string,
  readonly command: string,
  readonly cache: string,
  readonly warmup: number,
};

function makeRow(
  fields: RowFields,
  samples: $ReadOnlyArray<number>,
  cpuSamples: $ReadOnlyArray<number | null> | null,
): Row {
  const summary = summarise(samples);
  const cpu: Array<number> = [];
  for (const sample of cpuSamples ?? []) {
    if (sample != null) {
      cpu.push(sample);
    }
  }
  const cpuSummary =
    cpuSamples != null && cpu.length === cpuSamples.length && cpu.length > 0
      ? summarise(cpu)
      : null;
  return {
    id: `${fields.tool}/${fields.stage}/${fields.fixture}`,
    tool: fields.tool,
    stage: fields.stage,
    fixture: fields.fixture,
    title: fields.title,
    command: fields.command,
    cache: fields.cache,
    unit: "ms",
    warmup: fields.warmup,
    runs: samples.length,
    samples: samples.map((sample) => Number(sample.toFixed(1))),
    median: summary.median,
    min: summary.min,
    max: summary.max,
    mean: summary.mean,
    stddev: summary.stddev,
    cpu:
      cpuSummary == null
        ? null
        : {
            samples: cpu.map((sample) => Number(sample.toFixed(1))),
            median: cpuSummary.median,
            min: cpuSummary.min,
            max: cpuSummary.max,
          },
  };
}

function progress(line: string): void {
  process.stderr.write(`  ${line}\n`);
}

/**
 * Make the uf fixture what the stages assume, and prove it before timing anything.
 *
 * `uf fmt` first, so `uf fmt --check` is timed on a tree it passes. Then the
 * linter and the checker have to have looked at every module and found
 * nothing, and the runner has to have passed exactly the tests that were
 * written. Returns the host `uf test` ran the suite on.
 */
async function prepare(
  uf: string,
  env: Environment,
  dir: string,
  summary: FixtureSummary,
): Promise<string> {
  const options = { cwd: dir, env, timeoutMs: COMMAND_TIMEOUT_MS };
  // Every generated file but the manifest is a module.
  const modules = summary.files - 1;

  expectSuccess("uf fmt", dir, await run(uf, ["fmt"], options));

  for (const command of ["lint", "check"]) {
    const finished = await run(uf, [command, "--json"], options);
    const report = jsonObject(`uf ${command} --json`, finished);
    const checked = numberIn(`uf ${command}`, report, "filesChecked");
    const errors = numberIn(`uf ${command}`, report, "errors");
    if (errors !== 0 || checked < modules) {
      throw new Error(
        `\`uf ${command}\` checked ${String(checked)} of the fixture's ${String(modules)} modules ` +
          `and reported ${String(errors)} errors. The fixture is written to be clean, so either ` +
          `it has stopped being clean or uf has stopped looking at all of it:\n` +
          tail(finished.stdout, 60),
      );
    }
  }

  const host = await expectUfTests(uf, env, dir, summary.tests);
  expectSuccess("uf fmt --check", dir, await run(uf, ["fmt", "--check"], options));
  return host;
}

/** Run `uf test --json` in `dir`, require exactly `tests` passes, and return the host. */
async function expectUfTests(
  uf: string,
  env: Environment,
  dir: string,
  tests: number,
): Promise<string> {
  const tested = await run(uf, ["test", "--json"], {
    cwd: dir,
    env,
    timeoutMs: COMMAND_TIMEOUT_MS,
  });
  const results = jsonObject("uf test --json", tested);
  const passed = numberIn("uf test", results, "passed");
  const failed = numberIn("uf test", results, "failed");
  if (failed !== 0 || passed !== tests) {
    throw new Error(
      `\`uf test\` in ${dir} passed ${String(passed)} and failed ${String(failed)} of the ` +
        `${String(tests)} tests the fixture was written with:\n${tail(tested.stdout, 60)}`,
    );
  }
  const host = results.host;
  if (host != null && typeof host === "object" && !Array.isArray(host)) {
    return typeof host.kind === "string" ? host.kind : "unknown";
  }
  return "unknown";
}

/** A command that runs to completion, and everything needed to time it. */
type Timed = {
  readonly tool: string,
  readonly stage: string,
  readonly fixture: string,
  readonly title: string,
  readonly program: string,
  readonly args: $ReadOnlyArray<string>,
  readonly command: string,
  readonly dir: string,
  readonly caches: $ReadOnlyArray<string>,
};

async function oneShot(context: Context, timed: Timed, cold: boolean): Promise<Row> {
  const { options } = context;
  const samples = [];
  const cpu = [];
  for (let at = 0; at < options.warmup + options.runs; at += 1) {
    if (cold) {
      clearCaches(timed.dir, timed.caches);
    }
    const finished = await run(timed.program, timed.args, {
      cwd: timed.dir,
      env: context.env,
      timeoutMs: COMMAND_TIMEOUT_MS,
      cpu: true,
    });
    expectSuccess(timed.command, timed.dir, finished);
    if (at >= options.warmup) {
      samples.push(finished.ms);
      cpu.push(finished.cpuMs);
    }
  }
  const temperature = cold ? "cold" : "warm";
  return makeRow(
    {
      tool: timed.tool,
      stage: `${timed.stage}.${temperature}`,
      fixture: timed.fixture,
      title: `${timed.title}, ${temperature}`,
      command: timed.command,
      cache: describeCache(timed.caches, cold),
      warmup: options.warmup,
    },
    samples,
    cpu,
  );
}

/** A dev server, and everything needed to start it. */
type Served = {
  readonly tool: string,
  readonly fixture: string,
  readonly program: string,
  readonly args: (port: number) => $ReadOnlyArray<string>,
  readonly command: string,
  readonly dir: string,
  readonly caches: $ReadOnlyArray<string>,
};

async function devStart(context: Context, served: Served, cold: boolean): Promise<Row> {
  const { options } = context;
  const samples = [];
  for (let at = 0; at < options.warmup + options.runs; at += 1) {
    if (cold) {
      clearCaches(served.dir, served.caches);
    }
    const port = await freePort();
    const server = startDevServer(served.program, served.args(port), {
      cwd: served.dir,
      env: context.env,
      port,
      label: served.command,
    });
    try {
      const first = await waitForDocument(server, DEV_TIMEOUT_MS);
      if (at >= options.warmup) {
        samples.push(first.ms);
      }
    } finally {
      await server.stop();
    }
  }
  const temperature = cold ? "cold" : "warm";
  return makeRow(
    {
      tool: served.tool,
      stage: `dev.${temperature}`,
      fixture: served.fixture,
      title: `dev server start to first document, ${temperature}`,
      command: served.command,
      cache: describeCache(served.caches, cold),
      warmup: options.warmup,
    },
    samples,
    null,
  );
}

async function hotUpdate(
  context: Context,
  served: Served,
  hot: { readonly file: string, readonly urlPath: string },
): Promise<$ReadOnlyArray<Row>> {
  const { options } = context;
  // At least one edit is thrown away whatever `--warmup` says, for the reason
  // every stage throws its first run away.
  const warmup = Math.max(1, options.warmup);
  const port = await freePort();
  const server = startDevServer(served.program, served.args(port), {
    cwd: served.dir,
    env: context.env,
    port,
    label: served.command,
  });
  try {
    const first = await waitForDocument(server, DEV_TIMEOUT_MS);
    const samples = await measureHmr({
      port,
      document: first.body,
      file: path.join(served.dir, hot.file),
      urlPath: hot.urlPath,
      marker: HOT_MARKER,
      edits: warmup + options.hmrEdits,
      timeoutMs: HMR_TIMEOUT_MS,
      settleMs: 250,
    });
    const kept = samples.slice(warmup);
    const command = `${served.command}; edit ${hot.file}`;
    const cache = "one warm dev server; each edit rewrites one string literal";
    const fields = { tool: served.tool, fixture: served.fixture, command, cache, warmup };
    return [
      makeRow(
        { ...fields, stage: "hmr.message", title: "edit to HMR update message" },
        kept.map((sample) => sample.messageMs),
        null,
      ),
      makeRow(
        { ...fields, stage: "hmr.applied", title: "edit to updated module served" },
        kept.map((sample) => sample.appliedMs),
        null,
      ),
    ];
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`${message}\n\n\`${served.command}\` printed:\n${tail(server.log())}`);
  } finally {
    await server.stop();
  }
}

/** Every lockfile a manager could write, all removed for a cold install. */
const LOCKFILES = [
  "package-lock.json",
  "npm-shrinkwrap.json",
  "pnpm-lock.yaml",
  "yarn.lock",
  "bun.lock",
  "bun.lockb",
  "uf.lock",
];

/**
 * The environment an install runs in: this one, with a home of its own.
 *
 * A package manager keeps its cache under the home directory — npm in `~/.npm`,
 * pnpm and Bun in theirs — so a home that starts empty is a cache that starts
 * empty, whichever manager uf chose, without the harness having to know where
 * each one keeps it. It is also a home with no `.npmrc`, which is what makes
 * two machines' installs the same install.
 *
 * The update notifier is off because on a person's machine it asks the registry
 * once a day, and in a home that is new every run it would ask every run: a
 * cost no real install pays, on a row that is otherwise the most faithful.
 */
function isolatedHome(env: Environment, home: string): Environment {
  return {
    ...env,
    HOME: home,
    XDG_CACHE_HOME: path.join(home, ".cache"),
    XDG_CONFIG_HOME: path.join(home, ".config"),
    XDG_DATA_HOME: path.join(home, ".local", "share"),
    npm_config_update_notifier: "false",
  };
}

/** The package manager `uf install` chose, from the plan it writes. */
function managerOf(project: string): string {
  try {
    const plan: mixed = JSON.parse(
      fs.readFileSync(path.join(project, ".uf", "install.json"), "utf8"),
    );
    if (plan != null && typeof plan === "object" && !Array.isArray(plan)) {
      const chosen = plan.packageManager;
      if (chosen != null && typeof chosen === "object" && !Array.isArray(chosen)) {
        return typeof chosen.manager === "string" ? chosen.manager : "unknown";
      }
    }
  } catch {
    // Reported as unknown, which is what it is.
  }
  return "unknown";
}

/**
 * `<tool> install`, cold and then warm, on the install manifest.
 *
 * Each manager gets a project and a home of its own, so no manager starts warm
 * from another's cache.
 */
async function install(
  context: Context,
  tool: string,
  program: string,
  manifest: string,
): Promise<{ readonly rows: $ReadOnlyArray<Row>, readonly manager: string }> {
  const { options } = context;
  const root = path.join(options.workDir, `install-${tool}`);
  const project = path.join(root, "project");
  const home = path.join(root, "home");
  fs.rmSync(root, { recursive: true, force: true });
  fs.mkdirSync(project, { recursive: true });
  fs.writeFileSync(path.join(project, "package.json"), manifest);
  const installEnv = isolatedHome(context.env, home);
  const command = `${tool} install`;

  const timed = async (
    reset: () => void,
  ): Promise<{ samples: Array<number>, cpu: Array<number | null> }> => {
    const samples = [];
    const cpu = [];
    for (let at = 0; at < options.warmup + options.runs; at += 1) {
      reset();
      const finished = await run(program, ["install"], {
        cwd: project,
        env: installEnv,
        timeoutMs: COMMAND_TIMEOUT_MS,
        cpu: true,
      });
      expectSuccess(command, project, finished);
      if (!fs.existsSync(path.join(project, "node_modules", "react", "package.json"))) {
        throw new Error(
          `\`${command}\` exited 0 in ${project} and installed no react:\n${tail(finished.stdout)}`,
        );
      }
      if (at >= options.warmup) {
        samples.push(finished.ms);
        cpu.push(finished.cpuMs);
      }
    }
    return { samples, cpu };
  };

  progress(`install: ${tool} install.cold`);
  const cold = await timed(() => {
    for (const entry of ["node_modules", ".uf", ...LOCKFILES]) {
      fs.rmSync(path.join(project, entry), { recursive: true, force: true });
    }
    fs.rmSync(home, { recursive: true, force: true });
    fs.mkdirSync(home, { recursive: true });
  });
  // Warm starts from what the last cold install left: its lockfile and its cache.
  progress(`install: ${tool} install.warm`);
  const warm = await timed(() => {
    fs.rmSync(path.join(project, "node_modules"), { recursive: true, force: true });
  });
  const fields = { tool, fixture: "install", command, warmup: options.warmup };
  return {
    rows: [
      makeRow(
        {
          ...fields,
          stage: "install.cold",
          title: "install, cold",
          cache:
            "no lockfile, no node_modules, and an empty home, so an empty package-manager cache",
        },
        cold.samples,
        cold.cpu,
      ),
      makeRow(
        {
          ...fields,
          stage: "install.warm",
          title: "install, warm",
          cache:
            "the lockfile and package-manager cache the install before it left; no node_modules",
        },
        warm.samples,
        warm.cpu,
      ),
    ],
    manager: tool === "uf" ? managerOf(project) : tool,
  };
}

type OneShot = {
  readonly name: string,
  readonly args: $ReadOnlyArray<string>,
  readonly title: string,
};

/** In the order they run, so each finds the fixture the way the last left it. */
const ONE_SHOT: $ReadOnlyArray<OneShot> = [
  { name: "fmt", args: ["fmt", "--check"], title: "format check" },
  { name: "lint", args: ["lint"], title: "lint" },
  { name: "check", args: ["check"], title: "lint and type check" },
  { name: "test", args: ["test"], title: "test suite" },
  { name: "build", args: ["build"], title: "production build" },
];

/** Every stage id, in the order they run. */
const STAGES: $ReadOnlyArray<string> = [
  ...ONE_SHOT.flatMap((stage) => [`${stage.name}.cold`, `${stage.name}.warm`]),
  "dev.cold",
  "dev.warm",
  "hmr.message",
  "hmr.applied",
  "install.cold",
  "install.warm",
];

/** The tools this run measures, found: each one's program, or why it is skipped. */
type Found = {
  readonly programs: Map<string, string>,
  readonly versions: { [string]: string },
  readonly skipped: Array<Skipped>,
};

async function findTools(options: Options, env: Environment): Promise<Found> {
  const programs: Map<string, string> = new Map();
  const versions: { [string]: string } = {};
  const skipped: Array<Skipped> = [];
  for (const tool of RIVAL_TOOLS) {
    if (!options.tools.includes(tool.name)) {
      continue;
    }
    const located = locateTool(tool, REPO, env.PATH ?? "");
    const program = located.program;
    if (program == null) {
      const reason = located.skipped ?? "not found";
      skipped.push({ tool: tool.name, stage: null, reason });
      progress(`${tool.name}: skipped, ${reason}`);
      continue;
    }
    programs.set(tool.name, program);
    versions[tool.name] = versionIn(await printed(program, ["--version"], env));
  }
  return { programs, versions, skipped };
}

/**
 * The copies of `preset` the selected tools need, written on first use.
 *
 * Every copy but Next.js's links its `node_modules` into the pinned install;
 * Next.js's is hard links (`mirrorInto` says why), without `flow-bin`, which
 * is a third of the install and which Next.js never reads.
 */
function copies(workDir: string, preset: Preset, versions: Versions): (copy: Copy) => string {
  const written: Set<Copy> = new Set();
  const installed = path.join(REPO, RIVALS_DIR, "node_modules");
  return (copy) => {
    const dir = path.join(workDir, `${preset.name}-${copy}`);
    if (written.has(copy)) {
      return dir;
    }
    fs.rmSync(dir, { recursive: true, force: true });
    if (copy === "flow") {
      writeFiles(dir, flowFiles(fixtureFiles(preset, versions)));
    } else {
      const kind = copy.startsWith("fmt-") ? "vite" : copy;
      if (kind !== "vite" && kind !== "next" && kind !== "vitest" && kind !== "bun") {
        throw new Error(`there is no copy called ${copy}`);
      }
      writeFiles(dir, rivalFiles(preset, kind));
      if (kind === "next") {
        mirrorInto(dir, installed, ["flow-bin"]);
      } else if (kind !== "bun") {
        linkInto(dir, installed);
      }
    }
    written.add(copy);
    return dir;
  };
}

/**
 * Run a comparison once, untimed, and refuse it if it did not do the work.
 *
 * The formatter formats its copy first. Then the command itself has to exit
 * 0, and a test runner has to report exactly the tests its copy has.
 */
async function prepareRival(
  context: Context,
  spec: OneShotSpec,
  program: string,
  dir: string,
  tests: number,
): Promise<void> {
  const options = { cwd: dir, env: context.env, timeoutMs: COMMAND_TIMEOUT_MS };
  const setup = spec.setup;
  if (setup != null) {
    expectSuccess(`${spec.tool} ${setup.join(" ")}`, dir, await run(program, setup, options));
  }
  const command = `${spec.tool} ${spec.args.join(" ")}`;
  const finished = await run(program, spec.args, options);
  expectSuccess(command, dir, finished);
  if (spec.tests) {
    const passed = testsPassed(`${finished.stdout}\n${finished.stderr}`);
    if (passed !== tests) {
      throw new Error(
        `\`${command}\` in ${dir} reported ${String(passed ?? "no")} passing tests, and its copy ` +
          `was written with ${String(tests)}:\n${tail(`${finished.stdout}\n${finished.stderr}`)}`,
      );
    }
  }
}

/** The program for `tool` in `dir`: Next.js's own copy, for the reason `copies` gives. */
function programIn(tool: string, program: string, dir: string): string {
  return tool === "next" ? path.join(dir, "node_modules", ".bin", "next") : program;
}

async function measurePreset(
  context: Context,
  preset: Preset,
  uf: string,
  found: Found,
  fixtureVersions: Versions,
  selected: (stage: string) => boolean,
  results: Array<Row>,
): Promise<{ readonly summary: FixtureSummary, readonly host: string | null }> {
  const { options, env } = context;
  const measuring = (tool: string): boolean =>
    tool === "uf" ? options.tools.includes("uf") : found.programs.has(tool);
  let host = null;

  const dir = path.join(options.workDir, preset.name);
  fs.rmSync(dir, { recursive: true, force: true });
  const summary = generateFixture(dir, preset, fixtureVersions);
  linkDependencies(dir, REPO);

  if (measuring("uf")) {
    progress(`${preset.name}: checking the ${String(summary.files)} generated files first`);
    host = await prepare(uf, env, dir, summary);
    for (const stage of ONE_SHOT) {
      for (const cold of [true, false]) {
        const id = `${stage.name}.${cold ? "cold" : "warm"}`;
        if (selected(id)) {
          progress(`${preset.name}: uf ${id}`);
          results.push(
            await oneShot(
              context,
              {
                tool: "uf",
                stage: stage.name,
                fixture: preset.name,
                title: stage.title,
                program: uf,
                args: stage.args,
                command: `uf ${stage.args.join(" ")}`,
                dir,
                caches: CACHES,
              },
              cold,
            ),
          );
        }
      }
    }
    const served = {
      tool: "uf",
      fixture: preset.name,
      program: uf,
      args: (port: number) => ["dev", "--port", String(port)],
      command: "uf dev",
      dir,
      caches: CACHES,
    };
    for (const cold of [true, false]) {
      const id = `dev.${cold ? "cold" : "warm"}`;
      if (selected(id)) {
        progress(`${preset.name}: uf ${id}`);
        results.push(await devStart(context, served, cold));
      }
    }
    if (selected("hmr.message") || selected("hmr.applied")) {
      progress(`${preset.name}: uf hmr`);
      for (const row of await hotUpdate(context, served, {
        file: HOT_FILE,
        urlPath: `/${HOT_FILE}`,
      })) {
        if (selected(row.stage)) {
          results.push(row);
        }
      }
    }
  }

  const copyOf = copies(options.workDir, preset, fixtureVersions);
  for (const spec of ONE_SHOT_RIVALS) {
    const program = found.programs.get(spec.tool);
    if (program == null || !(selected(`${spec.stage}.cold`) || selected(`${spec.stage}.warm`))) {
      continue;
    }
    const copyDir = copyOf(spec.copy);
    const local = programIn(spec.tool, program, copyDir);
    progress(`${preset.name}: ${spec.tool} ${spec.stage}, checking its copy first`);
    await prepareRival(context, spec, local, copyDir, summary.tests);
    for (const cold of [true, false]) {
      const id = `${spec.stage}.${cold ? "cold" : "warm"}`;
      if (selected(id)) {
        progress(`${preset.name}: ${spec.tool} ${id}`);
        results.push(
          await oneShot(
            context,
            {
              tool: spec.tool,
              stage: spec.stage,
              fixture: preset.name,
              title: spec.title,
              program: local,
              args: spec.args,
              command: `${spec.tool} ${spec.args.join(" ")}`,
              dir: copyDir,
              caches: spec.caches,
            },
            cold,
          ),
        );
      }
    }
  }

  for (const spec of DEV_RIVALS) {
    const program = found.programs.get(spec.tool);
    if (program == null) {
      continue;
    }
    const copyDir = copyOf(spec.copy);
    const served = servedRival(spec, program, copyDir, preset.name);
    for (const cold of [true, false]) {
      const id = `dev.${cold ? "cold" : "warm"}`;
      if (selected(id)) {
        progress(`${preset.name}: ${spec.tool} ${id}`);
        results.push(await devStart(context, served, cold));
      }
    }
    const hmr = spec.hmr;
    if (!(selected("hmr.message") || selected("hmr.applied"))) {
      continue;
    }
    if (hmr == null) {
      const reason = spec.hmrSkipped ?? "no HMR stage";
      if (!found.skipped.some((one) => one.tool === spec.tool && one.stage === "hmr")) {
        found.skipped.push({ tool: spec.tool, stage: "hmr", reason });
      }
      progress(`${preset.name}: ${spec.tool} hmr skipped, ${reason}`);
    } else {
      progress(`${preset.name}: ${spec.tool} hmr`);
      for (const row of await hotUpdate(context, served, hmr)) {
        if (selected(row.stage)) {
          results.push(row);
        }
      }
    }
  }
  return { summary, host };
}

function servedRival(spec: DevSpec, program: string, dir: string, fixture: string): Served {
  return {
    tool: spec.tool,
    fixture,
    program: programIn(spec.tool, program, dir),
    args: spec.args,
    command: `${spec.tool} dev`,
    dir,
    caches: spec.caches,
  };
}

/**
 * The suite of many small files, for uf, Bun and Vitest — whichever are measured.
 *
 * `suite.js` writes the three copies and links uf's; Vitest's is linked into
 * the pinned install here, and Bun's needs nothing.
 */
async function measureSuite(
  context: Context,
  uf: string,
  found: Found,
  selected: (stage: string) => boolean,
  results: Array<Row>,
): Promise<boolean> {
  const { options, env } = context;
  const runners: Array<{
    readonly tool: string,
    readonly program: string,
    readonly args: $ReadOnlyArray<string>,
  }> = [];
  if (options.tools.includes("uf")) {
    runners.push({ tool: "uf", program: uf, args: ["test"] });
  }
  const bun = found.programs.get("bun");
  if (bun != null) {
    runners.push({ tool: "bun", program: bun, args: ["test"] });
  }
  const vitest = found.programs.get("vitest");
  if (vitest != null) {
    runners.push({ tool: "vitest", program: vitest, args: ["run"] });
  }
  if (runners.length === 0 || !(selected("test.cold") || selected("test.warm"))) {
    return false;
  }
  const root = path.join(options.workDir, "suite");
  generateSuites(root, GUIDE_PRESET, REPO);
  if (vitest != null) {
    linkInto(path.join(root, "vitest"), path.join(REPO, RIVALS_DIR, "node_modules"));
  }
  const tests = GUIDE_PRESET.files * GUIDE_PRESET.cases;
  for (const runner of runners) {
    const dir = path.join(root, runner.tool);
    progress(`suite: ${runner.tool} test, checking its copy first`);
    if (runner.tool === "uf") {
      await expectUfTests(uf, env, dir, tests);
    } else {
      await prepareRival(
        context,
        {
          tool: runner.tool,
          stage: "test",
          title: "test suite",
          copy: "vite",
          args: runner.args,
          caches: [],
          setup: null,
          tests: true,
        },
        runner.program,
        dir,
        tests,
      );
    }
    for (const cold of [true, false]) {
      const id = `test.${cold ? "cold" : "warm"}`;
      if (selected(id)) {
        progress(`suite: ${runner.tool} ${id}`);
        results.push(
          await oneShot(
            context,
            {
              tool: runner.tool,
              stage: "test",
              fixture: "suite",
              title: "many small test files",
              program: runner.program,
              args: runner.args,
              command: `${runner.tool} ${runner.args.join(" ")}`,
              dir,
              caches:
                runner.tool === "uf"
                  ? CACHES
                  : runner.tool === "vitest"
                    ? [path.join("node_modules", ".vite")]
                    : [],
            },
            cold,
          ),
        );
      }
    }
  }
  return true;
}

async function main(): Promise<void> {
  const argv = process.argv.slice(2).filter((given) => given !== "--");
  if (argv.includes("--help") || argv.includes("-h")) {
    process.stdout.write(`${USAGE}\n`);
    return;
  }
  const options = parseOptions(argv);
  const stages = options.stages;
  const selected = (stage: string): boolean =>
    stages == null || stages.some((name) => stage === name || stage.startsWith(`${name}.`));
  if (!STAGES.some(selected)) {
    throw new Error(
      `--stages ${String(stages?.join(","))} names no stage; the stages are ${STAGES.join(", ")}`,
    );
  }
  const uf = binary();
  requireInstall(REPO);
  refuseInsideRepository(options.workDir, REPO);
  fs.mkdirSync(options.workDir, { recursive: true });

  const before = options.requireQuiet ? await waitForQuiet(options.quietTimeoutMs) : quietness();
  const startedAt = new Date().toISOString();
  const env = childEnvironment();
  const versions = await collectVersions(uf, env);
  const found = await findTools(options, env);
  const context = { env, options };
  const fixtureVersions: Versions = {
    uniflowed: packageVersion(path.join(REPO, "packages", "react")),
    react: versions.react,
  };

  const fixtures: { [string]: FixtureSummary } = {};
  const results: Array<Row> = [];
  let host = "unknown";
  const fixtureStages = STAGES.filter((stage) => !stage.startsWith("install."));

  if (fixtureStages.some(selected)) {
    for (const preset of options.presets) {
      const measured = await measurePreset(
        context,
        preset,
        uf,
        found,
        fixtureVersions,
        selected,
        results,
      );
      fixtures[preset.name] = measured.summary;
      host = measured.host ?? host;
    }
  }
  const suite = (await measureSuite(context, uf, found, selected, results))
    ? {
        files: GUIDE_PRESET.files,
        cases: GUIDE_PRESET.cases,
        tests: GUIDE_PRESET.files * GUIDE_PRESET.cases,
        assertions: GUIDE_PRESET.files * GUIDE_PRESET.cases * 2,
      }
    : null;

  let installed: InstallFixture | null = null;
  if (selected("install.cold") || selected("install.warm")) {
    const manifest = installManifest(REPO);
    let manager = null;
    const installers = [
      options.tools.includes("uf") ? { tool: "uf", program: uf } : null,
      ...INSTALL_RIVALS.map((tool) => {
        const program = found.programs.get(tool);
        return program == null ? null : { tool, program };
      }),
    ];
    for (const installer of installers) {
      if (installer == null) {
        continue;
      }
      const measured = await install(context, installer.tool, installer.program, manifest.contents);
      if (installer.tool === "uf") {
        manager = measured.manager;
      }
      for (const row of measured.rows) {
        if (selected(row.stage)) {
          results.push(row);
        }
      }
    }
    installed = {
      dependencies: manifest.dependencies,
      manager: manager ?? "not measured",
    };
  }

  const report: Report = {
    schema: 1,
    arguments: argv,
    startedAt,
    finishedAt: new Date().toISOString(),
    provisional: !before.quiet,
    machine: machine(before, quietness()),
    versions: Object.fromEntries([
      ...Object.entries(versions),
      ["host", host],
      ...Object.entries(found.versions),
    ]),
    settings: { runs: options.runs, warmup: options.warmup, hmrEdits: options.hmrEdits },
    tools: options.tools,
    skipped: found.skipped,
    fixtures,
    suite,
    install: installed,
    results,
  };
  const out = options.out ?? path.join(options.workDir, "results.json");
  writeReport(out, report);
  process.stdout.write(formatTable(report));
  process.stdout.write(`\n  wrote ${out}\n`);
}

// The shell's convention, 128 and the signal's number, so whoever started the
// run can tell an interrupted benchmark from a failed one.
const SIGNALS: $ReadOnlyArray<[string, number]> = [
  ["SIGINT", 130],
  ["SIGTERM", 143],
];
for (const [signal, code] of SIGNALS) {
  process.on(signal, () => {
    stopEverything();
    process.exit(code);
  });
}

// Not top-level `await`: the Flow parser uf vendors does not accept it, which
// is a real limit of running a Flow entry point on the Capability JS Host.
main().catch((error: mixed) => {
  stopEverything();
  process.stderr.write(`\n${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
});
