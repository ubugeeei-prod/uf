// @flow
//
// Whether a run of `bench.js` is slower than the baseline committed beside it.
//
//   node --import @uniflowed/host/register tools/bench/toolchain/regress.js \
//     [--baseline FILE] [--write-regressed FILE] RESULTS.json [MORE.json ...]
//
// `.github/workflows/bench.yml` runs this every night against the uf rows of
// `baseline.json`, and fails when one of them has got more than 20% slower.
//
// # What is compared
//
// Every uf row of the baseline, by id (`uf/build.cold/small`), except the
// installs. An install is timed against the npm registry, and how fast the
// registry answered on a given night is not something a change to uf can move
// or a gate should fail on; the install rows are still measured and published,
// and still printed here, and are not gated. The other tools' rows are not
// gated either: they are the comparison, not the thing being guarded.
//
// A row the baseline has and a run does not is a failure — a stage that
// stopped reporting is the regression that hides every other — and a row a run
// has and the baseline does not is printed as new and not gated until the
// baseline is refreshed.
//
// # Why this does not flap on a shared runner
//
// A hosted runner is somebody else's computer, and a wall clock on it moves by
// ten or twenty per cent from one run to the next with no change to the code.
// Four things keep that noise from failing the build, and each is here because
// the one before it is not enough on its own:
//
//   1. Every row is a median of seven timed runs after one thrown away. Noise
//      on a shared machine is one-sided — a neighbour only ever makes a run
//      slower — and a median ignores up to three slow runs of seven, where a
//      mean is pulled by every one.
//   2. The line is 20% *and* 25 ms. The HMR and format rows are a few tens of
//      milliseconds, where 20% is less than the jitter of starting a process;
//      without a floor those rows would fail on nothing. A real regression on
//      them still fails as soon as it is bigger than both.
//   3. A row over the line is measured again, in a new process, and the better
//      of the two medians is what is compared (`--write-regressed` names the
//      stages to measure again, and the workflow passes both files here). A
//      neighbour that slowed one run down seldom slows the next one too; a
//      change to uf slows both.
//   4. The baseline is a run on the same kind of runner the check runs on,
//      committed from the workflow's own artifact — never a laptop's numbers.
//      When the runner's CPU model differs from the baseline's, this says so,
//      because a different machine is a different baseline.
//
// What that leaves is a gate that misses a regression of under 20%, and one
// hidden by a runner that happens to be faster than the baseline's. Both are
// the cost of not failing every other night; the published numbers show the
// small drifts that the gate lets through.
//
// # Refreshing the baseline
//
// When a change makes uf faster (this prints the rows that got more than 20%
// faster) or deliberately slower, or the runner changes: run the `Bench`
// workflow by hand (Actions → Bench → Run workflow), download its
// `bench-comparison` artifact, copy its `results.json` over
// `tools/bench/toolchain/baseline.json`, run `uf fmt` (the repository's JSON
// is Biome-formatted, and `fmt:check` holds this file to it too; the numbers
// do not change), and commit it. The same file is what the manual's
// benchmarks page renders, so the numbers a reader sees are the numbers the
// gate holds uf to.

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

/** A regression is more than this much slower than the baseline… */
export const THRESHOLD = 0.2;
/** …and more than this many milliseconds slower. */
export const FLOOR_MS = 25;

/** The part of a result row this reads. */
export type Measured = {
  readonly id: string,
  readonly tool: string,
  readonly stage: string,
  readonly median: number,
  ...
};

/** The part of a result file this reads. */
export type Results = {
  readonly machine?: { readonly cpu?: string, readonly cores?: number, ... },
  readonly results: $ReadOnlyArray<Measured>,
  ...
};

export type Outcome = "ok" | "slower" | "faster" | "missing" | "new";

export type Verdict = {
  readonly id: string,
  readonly stage: string,
  readonly baseline: number,
  /** The best median among the runs given, or null if none of them had the row. */
  readonly current: number | null,
  readonly gated: boolean,
  readonly outcome: Outcome,
};

/** Whether a row of the baseline is one the gate holds uf to. */
export function gated(row: Measured): boolean {
  return row.tool === "uf" && !row.stage.startsWith("install.");
}

/**
 * Every row of `baseline`, and every uf row the runs have that it does not,
 * with what happened to it. Pure.
 */
export function compare(
  baseline: Results,
  runs: $ReadOnlyArray<Results>,
  threshold: number = THRESHOLD,
  floorMs: number = FLOOR_MS,
): Array<Verdict> {
  const best: Map<string, number> = new Map();
  const stages: Map<string, string> = new Map();
  for (const run of runs) {
    for (const row of run.results) {
      const seen = best.get(row.id);
      best.set(row.id, seen == null ? row.median : Math.min(seen, row.median));
      stages.set(row.id, row.stage);
    }
  }
  const verdicts: Array<Verdict> = [];
  const known: Set<string> = new Set();
  for (const row of baseline.results) {
    known.add(row.id);
    const current = best.get(row.id) ?? null;
    const isGated = gated(row);
    let outcome: Outcome = "ok";
    if (current == null) {
      outcome = isGated ? "missing" : "ok";
    } else if (current > row.median * (1 + threshold) && current - row.median > floorMs) {
      outcome = "slower";
    } else if (current < row.median * (1 - threshold) && row.median - current > floorMs) {
      outcome = "faster";
    }
    if (current == null && !isGated) {
      continue;
    }
    verdicts.push({
      id: row.id,
      stage: row.stage,
      baseline: row.median,
      current,
      gated: isGated,
      outcome,
    });
  }
  for (const [id, current] of best) {
    if (!known.has(id) && id.startsWith("uf/")) {
      verdicts.push({
        id,
        stage: stages.get(id) ?? "",
        baseline: Number.NaN,
        current,
        gated: false,
        outcome: "new",
      });
    }
  }
  return verdicts;
}

/** The verdicts that fail the gate. */
export function failures(verdicts: $ReadOnlyArray<Verdict>): Array<Verdict> {
  return verdicts.filter(
    (verdict) => verdict.gated && (verdict.outcome === "slower" || verdict.outcome === "missing"),
  );
}

function readResults(file: string): Results {
  const parsed: mixed = JSON.parse(fs.readFileSync(file, "utf8"));
  if (
    parsed == null ||
    typeof parsed !== "object" ||
    Array.isArray(parsed) ||
    !Array.isArray(parsed.results)
  ) {
    throw new Error(`${file} is not a result file of bench.js: it has no "results" array`);
  }
  // $FlowFixMe[incompatible-type] - the shape of a bench.js file, checked above as far as this reads it
  return parsed;
}

function milliseconds(value: number | null): string {
  if (value == null || Number.isNaN(value)) {
    return "-";
  }
  return value >= 1000 ? `${(value / 1000).toFixed(2)} s` : `${String(Math.round(value))} ms`;
}

function change(verdict: Verdict): string {
  const current = verdict.current;
  if (current == null || Number.isNaN(verdict.baseline)) {
    return "";
  }
  const percent = ((current - verdict.baseline) / verdict.baseline) * 100;
  return `${percent >= 0 ? "+" : ""}${percent.toFixed(1)}%`;
}

const USAGE = `usage: regress.js [--baseline FILE] [--write-regressed FILE] RESULTS.json [MORE.json ...]

  --baseline FILE          the run to compare with (tools/bench/toolchain/baseline.json)
  --write-regressed FILE   write the stages that regressed, comma-separated, for
                           \`bench.js --stages\` to measure again`;

function main(): void {
  const here = import.meta.url;
  if (typeof here !== "string") {
    throw new Error("import.meta.url is not a string here, so the baseline cannot be found");
  }
  const argv = process.argv.slice(2).filter((given) => given !== "--");
  let baselineFile = path.join(path.dirname(fileURLToPath(here)), "baseline.json");
  let regressedFile: string | null = null;
  const files: Array<string> = [];
  for (let at = 0; at < argv.length; at += 1) {
    const flag = argv[at];
    if (flag === "--baseline" || flag === "--write-regressed") {
      const value = argv[at + 1];
      if (value == null) {
        throw new Error(`${flag} needs a value\n\n${USAGE}`);
      }
      at += 1;
      if (flag === "--baseline") {
        baselineFile = value;
      } else {
        regressedFile = value;
      }
    } else if (flag.startsWith("--")) {
      throw new Error(`${flag} is not an option\n\n${USAGE}`);
    } else {
      files.push(flag);
    }
  }
  if (files.length === 0) {
    throw new Error(USAGE);
  }
  if (!fs.existsSync(baselineFile)) {
    throw new Error(
      `there is no baseline at ${baselineFile}. It is a run of the Bench workflow, committed: ` +
        "see the top of tools/bench/toolchain/regress.js for how to make one",
    );
  }
  const baseline = readResults(baselineFile);
  const runs = files.map(readResults);

  const lines = [
    `baseline  ${baselineFile}`,
    `runs      ${files.join(", ")} (the best median of each row is compared)`,
    `line      ${String(THRESHOLD * 100)}% and ${String(FLOOR_MS)} ms slower than the baseline`,
  ];
  const theirs = baseline.machine;
  for (const [index, run] of runs.entries()) {
    const ours = run.machine;
    if (
      theirs != null &&
      ours != null &&
      (theirs.cpu !== ours.cpu || theirs.cores !== ours.cores)
    ) {
      lines.push(
        `warning   ${files[index]} ran on ${String(ours.cpu)} x${String(ours.cores)}, and the ` +
          `baseline on ${String(theirs.cpu)} x${String(theirs.cores)}: a different machine is a ` +
          "different baseline",
      );
    }
  }
  const verdicts = compare(baseline, runs);
  const width = Math.max(...verdicts.map((verdict) => verdict.id.length), 2);
  lines.push("");
  for (const verdict of verdicts) {
    const note =
      verdict.outcome === "ok" && !verdict.gated
        ? "not gated"
        : verdict.outcome === "faster"
          ? "faster: refresh the baseline"
          : verdict.outcome === "new"
            ? "new: not gated until the baseline has it"
            : verdict.outcome === "missing"
              ? "MISSING: the stage no longer reports"
              : verdict.outcome === "slower"
                ? verdict.gated
                  ? "SLOWER"
                  : "slower, not gated"
                : "";
    lines.push(
      `${verdict.id.padEnd(width)}  ${milliseconds(verdict.baseline).padStart(8)}  ` +
        `${milliseconds(verdict.current).padStart(8)}  ${change(verdict).padStart(7)}  ${note}`.trimEnd(),
    );
  }
  const failed = failures(verdicts);
  lines.push("");
  lines.push(
    failed.length === 0
      ? `no uf stage is more than ${String(THRESHOLD * 100)}% and ${String(FLOOR_MS)} ms slower`
      : `${String(failed.length)} uf ${failed.length === 1 ? "stage is" : "stages are"} slower ` +
          `than the baseline allows: ${failed.map((verdict) => verdict.id).join(", ")}`,
  );
  process.stdout.write(`${lines.join("\n")}\n`);
  if (regressedFile != null) {
    const stages = [...new Set(failed.map((verdict) => verdict.stage))];
    fs.writeFileSync(regressedFile, stages.join(","));
  }
  if (failed.length > 0) {
    process.exitCode = 1;
  }
}

if (process.argv[1] != null && process.argv[1].endsWith(path.join("toolchain", "regress.js"))) {
  try {
    main();
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 2;
  }
}
