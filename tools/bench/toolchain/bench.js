// @flow
//
// Every uf command, timed on an application of a stated size.
//
//   uf run bench:toolchain                            # the small preset
//   uf run bench:toolchain --preset large --runs 10
//   uf run bench:toolchain --preset all --require-quiet --out results.json
//
// # Why this exists
//
// ubugeeei-prod/uf#945. The manual quoted numbers from separate one-off runs,
// each on its own input, with nothing anybody could run again — so a regression
// went unnoticed, and nobody could say which gap to close next. This is one
// command that produces every uf number from one generated fixture on one
// machine under one set of rules, and writes the rules into the same file as
// the numbers. `report.js` documents that file.
//
// It is the uf half of #945. Vite+, Next.js, Bun and pnpm on the same stages
// are the other half and will be rows in the same file, which is why a row is
// keyed by tool as well as by stage.
//
// # What one run of each stage is
//
// Wall clock from spawning the command to it exiting, read in this process —
// never the total a command prints about itself, which starts after the binary
// has loaded and stops before it exits.
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
// *Cold* removes `.uf`, `dist` and `node_modules/.vite` from the fixture before
// every run: a fresh clone with its dependencies installed. *Warm* leaves what
// the run before it wrote: the second time you type the command. Both, because
// they regress differently — a slower transform shows in cold, and a cache that
// stopped hitting shows only in warm.
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
// the browser's time, not the toolchain's.
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
// fast, and a benchmark that timed it would be timing a bug.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import {
  HOT_FILE,
  HOT_MARKER,
  PRESETS,
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
import type { InstallFixture, Machine, Report, Row } from "./report.js";

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
  --runs N                   timed runs of every stage (5)
  --warmup N                 runs made and thrown away before those (1)
  --hmr-edits N              timed edits in the HMR stage (10)
  --stages a,b               only these stages, by id or prefix: dev, build.cold
  --work-dir DIR             where fixtures are generated; outside the repository
                             (a directory under the system temporary directory)
  --out FILE                 where the JSON goes (<work-dir>/results.json)
  --require-quiet            wait for a quiet machine first, and fail without one
  --quiet-timeout MINUTES    how long to wait for it (15)

UF_BINARY names the uf to measure; \`uf run bench:toolchain\` builds one and sets it.`;

/** How long any one command may take before the run is abandoned. */
const COMMAND_TIMEOUT_MS = 10 * 60_000;
/** How long `uf dev` may take to serve its first document. */
const DEV_TIMEOUT_MS = 2 * 60_000;
/** How long an edit may take to arrive as an HMR update. */
const HMR_TIMEOUT_MS = 15_000;

type Options = {
  readonly presets: $ReadOnlyArray<Preset>,
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

function parseOptions(argv: $ReadOnlyArray<string>): Options {
  let presets: $ReadOnlyArray<Preset> = [presetNamed("small")];
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
        stages = value
          .split(",")
          .map((stage) => stage.trim())
          .filter((stage) => stage !== "");
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
  return { presets, runs, warmup, hmrEdits, stages, workDir, out, requireQuiet, quietTimeoutMs };
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
 * finds, which on a machine with Bun on `PATH` and no Node is Bun.
 */
function childEnvironment(): Environment {
  const env: { [string]: string | void } = { ...process.env };
  delete env.UF_BINARY;
  delete env.UF_PROJECT_ROOT;
  delete env.NODE_OPTIONS;
  env.PATH = [path.dirname(process.execPath), process.env.PATH ?? ""].join(path.delimiter);
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

async function collectVersions(uf: string, env: Environment): Promise<{ [string]: string }> {
  const output = async (program: string, args: $ReadOnlyArray<string>): Promise<string> => {
    try {
      const finished = await run(program, args, { cwd: REPO, env, timeoutMs: 60_000 });
      return finished.code === 0 ? finished.stdout.trim() : "unknown";
    } catch {
      return "unknown";
    }
  };
  const relative = path.relative(REPO, uf);
  return {
    // `uf --version` prints `uf 0.0.0-alpha.32`, and the key is already the name.
    uf: (await output(uf, ["--version"])).replace(/^uf\s+/, ""),
    commit: await output("git", ["rev-parse", "HEAD"]),
    // Where the binary came from: `target/release/uf` is a release build, and a
    // path outside the checkout is reported by name rather than leaking a home
    // directory into a file somebody may publish.
    binary: relative.startsWith("..") || path.isAbsolute(relative) ? path.basename(uf) : relative,
    node: process.version,
    npm: await output(path.join(path.dirname(process.execPath), "npm"), ["--version"]),
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

/** What a cold run removes from the fixture before it starts. */
const CACHES = [".uf", "dist", path.join("node_modules", ".vite")];
const COLD = "removes .uf, dist and node_modules/.vite before every run";
const WARM = "keeps what the run before it wrote";

function clearCaches(dir: string): void {
  for (const cache of CACHES) {
    fs.rmSync(path.join(dir, cache), { recursive: true, force: true });
  }
}

function expectSuccess(command: string, dir: string, finished: Finished): void {
  const printed = tail(`${finished.stdout}\n${finished.stderr}`);
  if (finished.timedOut) {
    throw new Error(
      `\`${command}\` in ${dir} was still running after ` +
        `${String(COMMAND_TIMEOUT_MS / 60_000)} minutes and was stopped:\n${printed}`,
    );
  }
  if (finished.code !== 0) {
    throw new Error(
      `\`${command}\` in ${dir} exited with ${String(finished.code ?? finished.signal)}, ` +
        `so it has no time to report:\n${printed}`,
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
  readonly uf: string,
  readonly env: Environment,
  readonly options: Options,
  readonly fixture: string,
  readonly dir: string,
};

type RowFields = {
  readonly stage: string,
  readonly title: string,
  readonly command: string,
  readonly cache: string,
  readonly warmup: number,
};

function makeRow(fixture: string, fields: RowFields, samples: $ReadOnlyArray<number>): Row {
  const summary = summarise(samples);
  return {
    id: `uf/${fields.stage}/${fixture}`,
    tool: "uf",
    stage: fields.stage,
    fixture,
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
  };
}

function progress(line: string): void {
  process.stderr.write(`  ${line}\n`);
}

/**
 * Make the fixture what the stages assume, and prove it before timing anything.
 *
 * `uf fmt` first, so `uf fmt --check` is timed on a tree it passes. Then the
 * linter and the checker have to have looked at every module and found
 * nothing, and the runner has to have passed exactly the tests that were
 * written. Returns the host `uf test` ran the suite on.
 */
async function prepare(context: Context, summary: FixtureSummary): Promise<string> {
  const { uf, env, dir } = context;
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

  const tested = await run(uf, ["test", "--json"], options);
  const results = jsonObject("uf test --json", tested);
  const passed = numberIn("uf test", results, "passed");
  const failed = numberIn("uf test", results, "failed");
  if (failed !== 0 || passed !== summary.tests) {
    throw new Error(
      `\`uf test\` passed ${String(passed)} and failed ${String(failed)} of the ` +
        `${String(summary.tests)} tests the fixture was written with:\n${tail(tested.stdout, 60)}`,
    );
  }

  expectSuccess("uf fmt --check", dir, await run(uf, ["fmt", "--check"], options));

  const host = results.host;
  if (host != null && typeof host === "object" && !Array.isArray(host)) {
    return typeof host.kind === "string" ? host.kind : "unknown";
  }
  return "unknown";
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

async function oneShot(context: Context, stage: OneShot, cold: boolean): Promise<Row> {
  const { options } = context;
  const command = `uf ${stage.args.join(" ")}`;
  const samples = [];
  for (let at = 0; at < options.warmup + options.runs; at += 1) {
    if (cold) {
      clearCaches(context.dir);
    }
    const finished = await run(context.uf, stage.args, {
      cwd: context.dir,
      env: context.env,
      timeoutMs: COMMAND_TIMEOUT_MS,
    });
    expectSuccess(command, context.dir, finished);
    if (at >= options.warmup) {
      samples.push(finished.ms);
    }
  }
  const temperature = cold ? "cold" : "warm";
  return makeRow(
    context.fixture,
    {
      stage: `${stage.name}.${temperature}`,
      title: `${stage.title}, ${temperature}`,
      command,
      cache: cold ? COLD : WARM,
      warmup: options.warmup,
    },
    samples,
  );
}

async function devStart(context: Context, cold: boolean): Promise<Row> {
  const { options } = context;
  const samples = [];
  for (let at = 0; at < options.warmup + options.runs; at += 1) {
    if (cold) {
      clearCaches(context.dir);
    }
    const port = await freePort();
    const server = startDevServer(context.uf, ["dev", "--port", String(port)], {
      cwd: context.dir,
      env: context.env,
      port,
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
    context.fixture,
    {
      stage: `dev.${temperature}`,
      title: `dev server start to first document, ${temperature}`,
      command: "uf dev",
      cache: cold ? COLD : WARM,
      warmup: options.warmup,
    },
    samples,
  );
}

async function hotUpdate(context: Context): Promise<$ReadOnlyArray<Row>> {
  const { options } = context;
  // At least one edit is thrown away whatever `--warmup` says, for the reason
  // every stage throws its first run away.
  const warmup = Math.max(1, options.warmup);
  const port = await freePort();
  const server = startDevServer(context.uf, ["dev", "--port", String(port)], {
    cwd: context.dir,
    env: context.env,
    port,
  });
  try {
    const first = await waitForDocument(server, DEV_TIMEOUT_MS);
    const samples = await measureHmr({
      port,
      document: first.body,
      file: path.join(context.dir, HOT_FILE),
      urlPath: `/${HOT_FILE}`,
      marker: HOT_MARKER,
      edits: warmup + options.hmrEdits,
      timeoutMs: HMR_TIMEOUT_MS,
      settleMs: 250,
    });
    const kept = samples.slice(warmup);
    const command = `uf dev; edit ${HOT_FILE}`;
    const cache = "one warm dev server; each edit rewrites one string literal";
    return [
      makeRow(
        context.fixture,
        { stage: "hmr.message", title: "edit to HMR update message", command, cache, warmup },
        kept.map((sample) => sample.messageMs),
      ),
      makeRow(
        context.fixture,
        { stage: "hmr.applied", title: "edit to updated module served", command, cache, warmup },
        kept.map((sample) => sample.appliedMs),
      ),
    ];
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`${message}\n\n\`uf dev\` printed:\n${tail(server.log())}`);
  } finally {
    await server.stop();
  }
}

/** Every lockfile a manager uf drives could write, all removed for a cold install. */
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

async function install(
  uf: string,
  env: Environment,
  options: Options,
): Promise<{ readonly rows: $ReadOnlyArray<Row>, readonly fixture: InstallFixture }> {
  const root = path.join(options.workDir, "install");
  const project = path.join(root, "project");
  const home = path.join(root, "home");
  fs.rmSync(root, { recursive: true, force: true });
  fs.mkdirSync(project, { recursive: true });
  const manifest = installManifest(REPO);
  fs.writeFileSync(path.join(project, "package.json"), manifest.contents);
  const installEnv = isolatedHome(env, home);

  const timed = async (reset: () => void): Promise<Array<number>> => {
    const samples = [];
    for (let at = 0; at < options.warmup + options.runs; at += 1) {
      reset();
      const finished = await run(uf, ["install"], {
        cwd: project,
        env: installEnv,
        timeoutMs: COMMAND_TIMEOUT_MS,
      });
      expectSuccess("uf install", project, finished);
      if (!fs.existsSync(path.join(project, "node_modules", "react", "package.json"))) {
        throw new Error(
          `\`uf install\` exited 0 in ${project} and installed no react:\n${tail(finished.stdout)}`,
        );
      }
      if (at >= options.warmup) {
        samples.push(finished.ms);
      }
    }
    return samples;
  };

  progress("install: install.cold");
  const cold = await timed(() => {
    for (const entry of ["node_modules", ".uf", ...LOCKFILES]) {
      fs.rmSync(path.join(project, entry), { recursive: true, force: true });
    }
    fs.rmSync(home, { recursive: true, force: true });
    fs.mkdirSync(home, { recursive: true });
  });
  // Warm starts from what the last cold install left: its lockfile and its cache.
  progress("install: install.warm");
  const warm = await timed(() => {
    fs.rmSync(path.join(project, "node_modules"), { recursive: true, force: true });
  });
  return {
    rows: [
      makeRow(
        "install",
        {
          stage: "install.cold",
          title: "install, cold",
          command: "uf install",
          cache:
            "no lockfile, no node_modules, and an empty home, so an empty package-manager cache",
          warmup: options.warmup,
        },
        cold,
      ),
      makeRow(
        "install",
        {
          stage: "install.warm",
          title: "install, warm",
          command: "uf install",
          cache:
            "the lockfile and package-manager cache the install before it left; no node_modules",
          warmup: options.warmup,
        },
        warm,
      ),
    ],
    fixture: { dependencies: manifest.dependencies, manager: managerOf(project) },
  };
}

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
      const dir = path.join(options.workDir, preset.name);
      fs.rmSync(dir, { recursive: true, force: true });
      const summary = generateFixture(dir, preset, fixtureVersions);
      linkDependencies(dir, REPO);
      fixtures[preset.name] = summary;
      const context = { uf, env, options, fixture: preset.name, dir };
      progress(`${preset.name}: checking the ${String(summary.files)} generated files first`);
      host = await prepare(context, summary);
      for (const stage of ONE_SHOT) {
        for (const cold of [true, false]) {
          const id = `${stage.name}.${cold ? "cold" : "warm"}`;
          if (selected(id)) {
            progress(`${preset.name}: ${id}`);
            results.push(await oneShot(context, stage, cold));
          }
        }
      }
      for (const cold of [true, false]) {
        const id = `dev.${cold ? "cold" : "warm"}`;
        if (selected(id)) {
          progress(`${preset.name}: ${id}`);
          results.push(await devStart(context, cold));
        }
      }
      if (selected("hmr.message") || selected("hmr.applied")) {
        progress(`${preset.name}: hmr`);
        for (const row of await hotUpdate(context)) {
          if (selected(row.stage)) {
            results.push(row);
          }
        }
      }
    }
  }

  let installed = null;
  if (selected("install.cold") || selected("install.warm")) {
    const measured = await install(uf, env, options);
    installed = measured.fixture;
    for (const row of measured.rows) {
      if (selected(row.stage)) {
        results.push(row);
      }
    }
  }

  const report: Report = {
    schema: 1,
    arguments: argv,
    startedAt,
    finishedAt: new Date().toISOString(),
    provisional: !before.quiet,
    machine: machine(before, quietness()),
    versions: { ...versions, host },
    settings: { runs: options.runs, warmup: options.warmup, hmrEdits: options.hmrEdits },
    fixtures,
    install: installed,
    results,
  };
  const out = options.out ?? path.join(options.workDir, "results.json");
  writeReport(out, report);
  process.stdout.write(formatTable(report));
  process.stdout.write(`\n  wrote ${out}\n`);
}

for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => {
    stopEverything();
    process.exit(130);
  });
}

// Not top-level `await`: the Flow parser uf vendors does not accept it, which
// is a real limit of running a Flow entry point on the Capability JS Host.
main().catch((error: mixed) => {
  stopEverything();
  process.stderr.write(`\n${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
});
