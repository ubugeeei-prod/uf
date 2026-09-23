#!/usr/bin/env node
// @noflow
//
// Run one column of the deployment compatibility matrix.
//
//   node tools/deploy-matrix/run.mjs --target <id> [--uf <path>] [--out <dir>] [--only <mode,…>]
//
// For the target named in `./matrix.json`:
//
// 1. **Refusals.** Every cell that carries a `refusal` is built as its own copy
//    of the fixture — the feature the target cannot serve, and nothing else it
//    would also refuse — and the build must fail with a message matching the
//    cell's pattern. That is what `rejected` means: unsupported, and said so
//    by name at build time rather than discovered in production.
// 2. **The build.** The fixture (or the copy the target's `build` names) is
//    built with `uf build --adapter <adapter>`.
// 3. **The host.** `./lib/hosts.mjs` starts the platform's runtime or its
//    local emulator on the directory that build wrote.
// 4. **The checks.** Every `verified` cell's check must pass; every
//    `not-emulated` cell's check must fail (so an emulator that starts
//    supporting it is noticed); `planned` cells are not run.
//
// It writes `<out>/<target>.json` with every cell's outcome, which
// `./report.mjs` joins into the matrix, and exits non-zero when any cell did
// not behave as `./matrix.json` says. Needs a socket: CI runs it, one job per
// target (`.github/workflows/ci.yml`, `deploy-matrix`).

import { spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";

import { hydration } from "./lib/browser.mjs";
import { CHECKS } from "./lib/checks.mjs";
import { ALL_FEATURES, MATRIX_DIR, makeCopy } from "./lib/fixture.mjs";
import { HOSTS } from "./lib/hosts.mjs";
import { cellsOf, loadMatrix } from "./lib/matrix.mjs";

const argv = process.argv.slice(2);
const option = (name) => {
  const at = argv.indexOf(`--${name}`);
  return at === -1 ? null : (argv[at + 1] ?? null);
};

const targetId = option("target");
const ufBinary = path.resolve(option("uf") ?? process.env.UF_BINARY ?? "target/release/uf");
const outDir = path.resolve(option("out") ?? path.join(MATRIX_DIR, ".work", "results"));
const only = option("only")?.split(",") ?? null;

const matrix = loadMatrix();
const target = matrix.targets.find((candidate) => candidate.id === targetId);
if (target == null) {
  process.stderr.write(
    `usage: node tools/deploy-matrix/run.mjs --target <${matrix.targets.map((t) => t.id).join("|")}> [--uf <path>] [--out <dir>] [--only <mode,…>]\n`,
  );
  process.exit(2);
}
const cells = cellsOf(matrix, target);

/** `uf build --adapter` in `dir`; answers the exit status and everything it printed. */
function build(dir, adapter) {
  const started = performance.now();
  const result = spawnSync(ufBinary, ["--cwd", dir, "build", "--adapter", adapter], {
    encoding: "utf8",
    env: { ...process.env, NO_COLOR: "1" },
    maxBuffer: 64 * 1024 * 1024,
  });
  if (result.error != null) throw result.error;
  return {
    status: result.status,
    output: `${result.stdout}\n${result.stderr}`,
    ms: Math.round(performance.now() - started),
  };
}

const results = {};
const record = (mode, outcome) => {
  results[mode] = { status: cells[mode].status, ...outcome };
  const mark = outcome.ok ? "ok  " : "FAIL";
  process.stdout.write(
    `  ${mark} ${mode}: ${outcome.outcome}${outcome.error ? `\n        ${outcome.error.split("\n").join("\n        ")}` : ""}\n`,
  );
  for (const note of outcome.notes ?? []) process.stdout.write(`        ${note}\n`);
};
const wanted = (mode) => only == null || only.includes(mode);

process.stdout.write(`deploy matrix: ${target.title} (--adapter ${target.adapter})\n`);

// 1. Refusals, one build per distinct copy.
const refusalBuilds = new Map();
for (const [mode, cell] of Object.entries(cells)) {
  if (cell.refusal == null || !wanted(mode)) continue;
  const { features, config, pattern } = cell.refusal;
  const key = `${config}:${[...features].sort().join(",")}`;
  if (!refusalBuilds.has(key)) {
    const dir = makeCopy(`${target.id}-refusal-${refusalBuilds.size}`, { features, config });
    refusalBuilds.set(key, build(dir, target.adapter));
  }
  const built = refusalBuilds.get(key);
  const matched = new RegExp(pattern, "s").test(built.output);
  const refused = built.status !== 0 && matched;
  const outcome = refused
    ? "refused, by name"
    : built.status === 0
      ? "built (expected a refusal)"
      : "refused, without naming it";
  // For a `rejected` cell this *is* the evidence. For a `verified` cell it is
  // the documented refusal of the configuration the target needs something
  // else for (a Lambda with no cache provider), recorded beside the check.
  results[`${mode}#refusal`] = { status: "rejected", ok: refused, outcome, ms: built.ms };
  process.stdout.write(
    `  ${refused ? "ok  " : "FAIL"} ${mode} refusal: ${outcome} (${built.ms} ms)\n`,
  );
  if (!refused)
    process.stdout.write(
      `        pattern: ${pattern}\n        output:\n${built.output.slice(-3000)}\n`,
    );
}
for (const [mode, cell] of Object.entries(cells)) {
  if (cell.status !== "rejected" || !wanted(mode)) continue;
  const refusal = results[`${mode}#refusal`];
  results[mode] = {
    status: "rejected",
    ok: refusal?.ok === true,
    outcome: refusal?.outcome ?? "no refusal to check",
  };
  delete results[`${mode}#refusal`];
}

// 2. The build the host serves.
const shape = target.build ?? {
  features: ALL_FEATURES.filter((feature) => feature !== "s3cache"),
  config: "full",
};
const dir = makeCopy(target.id, shape);
const built = build(dir, target.adapter);
process.stdout.write(`  build: exit ${built.status} in ${built.ms} ms\n`);
if (built.status !== 0) {
  process.stdout.write(built.output.slice(-6000));
  process.stderr.write(
    `deploy matrix: uf build --adapter ${target.adapter} failed for ${target.title}\n`,
  );
  process.exit(1);
}
const deployDir = path.join(dir, ".uf", "deploy", target.adapter);
if (argv.includes("--build-only")) {
  // For a machine that cannot bind a socket (the sandbox uf is developed in):
  // the refusals and the build are everything that runs without one.
  const failed = Object.entries(results).filter(([, result]) => !result.ok);
  process.stdout.write(`  built ${deployDir}; --build-only, so no host is started\n`);
  process.exit(failed.length > 0 ? 1 : 0);
}

// 3. The host, and 4. the checks. The two ISR checks restart the host, so they
// run last; everything else asks a host that has not been restarted.
const order = Object.keys(cells).sort(
  (left, right) => Number(left.startsWith("isr-")) - Number(right.startsWith("isr-")),
);
const host = await HOSTS[target.id](deployDir);
process.stdout.write(`  host: ${host.base}\n`);
try {
  for (const mode of order) {
    const cell = cells[mode];
    if (!wanted(mode) || cell.status === "rejected") continue;
    if (cell.status === "planned") {
      record(mode, { ok: true, outcome: "planned, not run" });
      continue;
    }
    const started = performance.now();
    let error = null;
    let notes = [];
    try {
      notes =
        mode === "hydration"
          ? await hydration(host.base, { actions: cells["action-json"]?.status === "verified" })
          : await CHECKS[mode]({
              base: host.base,
              target: target.id,
              expect: cell.expect,
              buildDir: dir,
              deployDir,
              restart: host.restart,
            });
    } catch (caught) {
      error = caught?.message ?? String(caught);
    }
    const ms = Math.round(performance.now() - started);
    if (cell.status === "verified") {
      record(mode, {
        ok: error == null,
        outcome: error == null ? `passed (${ms} ms)` : `failed (${ms} ms)`,
        error,
        notes,
        ms,
      });
    } else {
      // `not-emulated`: the failure is the documented gap; a pass means the
      // emulator caught up and the matrix should say so.
      record(mode, {
        ok: error != null,
        outcome:
          error != null
            ? `failed as documented (${ms} ms)`
            : "passed, so the emulator now supports it: mark it verified",
        error: error == null ? null : error.split("\n")[0],
        ms,
      });
    }
  }
} finally {
  const failed = Object.values(results).some((result) => !result.ok);
  if (failed) process.stdout.write(`\n--- host output (tail) ---\n${host.logs().slice(-8000)}\n`);
  await host.stop();
}

mkdirSync(outDir, { recursive: true });
const versions = {};
for (const [name, command] of [
  ["uf", [ufBinary, "--version"]],
  ["node", ["node", "--version"]],
]) {
  const answered = spawnSync(command[0], command.slice(1), { encoding: "utf8" });
  versions[name] = answered.stdout?.trim() || null;
}
writeFileSync(
  path.join(outDir, `${target.id}.json`),
  `${JSON.stringify({ target: target.id, versions, cells: results }, null, 2)}\n`,
);
const failures = Object.entries(results).filter(([, result]) => !result.ok);
if (failures.length > 0) {
  process.stderr.write(
    `\ndeploy matrix: ${target.title}: ${failures.map(([mode]) => mode).join(", ")} did not behave as tools/deploy-matrix/matrix.json says\n`,
  );
  process.exit(1);
}
process.stdout.write(`\ndeploy matrix: ${target.title}: every cell behaved as the matrix says\n`);
