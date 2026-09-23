// @flow
//
// What `bench.js` prints, and the file it writes.
//
// # The file
//
// One JSON document a run. Everything needed to reproduce a number, or to
// distrust it, is in the same document as the number:
//
//   {
//     "schema": 1,
//     "arguments": ["--preset", "small"],     // what bench.js was given
//     "startedAt": "2026-09-14T05:00:00.000Z",
//     "finishedAt": "2026-09-14T05:09:12.000Z",
//     "provisional": true,                    // false only if the machine was quiet
//                                             // for a minute before the run started
//     "machine": {
//       "platform": "darwin 23.6.0", "arch": "arm64", "cpu": "Apple M3",
//       "cores": 8, "memoryBytes": 25769803776,
//       "ci": null,                           // "github-actions <runner> (<os> <arch>, <image>)"
//       "before": { "load": [0.9, 1.1, 1.2], "rustc": 0, "quiet": true },
//       "after":  { "load": [3.1, 2.0, 1.5], "rustc": 0, "quiet": false }
//     },
//     "versions": {                           // every tool a row ran, by name
//       "uf": "0.0.0-alpha.32", "commit": "45e291e9…", "binary": "target/release/uf",
//       "node": "v26.8.1", "npm": "11.19.0", "vite": "8.2.2", "react": "19.3.0",
//       "host": "node",                       // the Capability JS Host uf ran the tests on
//       "vp": "1.0.0-rc.0", "next": "16.3.6", "bun": "1.3.14"   // one per tool measured
//     },
//     "settings": { "runs": 5, "warmup": 1, "hmrEdits": 10 },
//     "tools": ["uf", "vp", "next", "bun"],   // what --tools asked for
//     "skipped": [                            // what was asked for and not measured
//       { "tool": "eslint", "stage": null, "reason": "… has no eslint: run `npm ci …`" },
//       { "tool": "next", "stage": "hmr", "reason": "next dev pushes its updates …" }
//     ],
//     "fixtures": {                           // what each row's `fixture` names
//       "small": { "preset": "small", "routes": 10, "components": 31,
//                  "clientComponents": 11, "modules": 20, "testFiles": 20,
//                  "tests": 200, "files": 86, "lines": 4321, "bytes": 123456 }
//     },
//     "suite": { "files": 50, "cases": 20, "tests": 1000, "assertions": 2000 },
//                                             // the fixture called "suite"; null without it
//     "install": { "dependencies": 14, "manager": "npm" },   // null without install rows;
//                                             // `manager` is the one `uf install` chose
//     "results": [
//       {
//         "id": "uf/build.cold/small",        // tool/stage/fixture: unique in a file,
//                                             // and the same row in every file
//         "tool": "uf",                       // a key of `versions`
//         "stage": "build.cold",
//         "fixture": "small",                 // a key of `fixtures`, "suite" or "install"
//         "title": "production build, cold",
//         "command": "uf build",              // as typed, in the fixture's directory
//         "cache": "removes .uf, dist and node_modules/.vite before every run",
//         "unit": "ms",
//         "warmup": 1,                        // runs made and thrown away first
//         "runs": 5,                          // how many `samples` there are
//         "samples": [3012.4, 2987.1, 3050.9, 2999.0, 3021.7],   // in the order they ran
//         "median": 3012.4, "min": 2987.1, "max": 3050.9,
//         "mean": 3014.2, "stddev": 21.9,
//         "cpu": {                            // user + system time of the command and
//           "samples": [9120.3, …],           // everything under it, for a command that
//           "median": 9120.3,                 // runs to completion; null for a dev
//           "min": 8990.0, "max": 9301.2      // server and an HMR edit
//         }
//       }
//     ]
//   }
//
// `schema` changes when a field changes meaning or goes away. Adding a field
// does not change it, and a reader of schema 1 ignores fields it does not know.
// What a regression check or a page reads is `results[].id` and `median`; the
// rest is there for the person deciding whether to believe it. `tools`,
// `skipped`, `suite` and `cpu` arrived after the first files were written, and
// a reader treats a file without them as uf alone, nothing skipped, no suite
// and no CPU time.
//
// # The table
//
// The same rows, for a terminal: the machine and versions first, because a
// number read without them is a number without a unit.

import fs from "node:fs";
import path from "node:path";

import type { FixtureSummary } from "./fixture.js";
import type { Quietness } from "./measure.js";

export type Row = {
  readonly id: string,
  readonly tool: string,
  readonly stage: string,
  readonly fixture: string,
  readonly title: string,
  readonly command: string,
  readonly cache: string,
  readonly unit: "ms",
  readonly warmup: number,
  readonly runs: number,
  readonly samples: $ReadOnlyArray<number>,
  readonly median: number,
  readonly min: number,
  readonly max: number,
  readonly mean: number,
  readonly stddev: number,
  readonly cpu: CpuSummary | null,
};

export type CpuSummary = {
  readonly samples: $ReadOnlyArray<number>,
  readonly median: number,
  readonly min: number,
  readonly max: number,
};

/** A tool, or one stage of it, that was asked for and not measured, and why. */
export type Skipped = {
  readonly tool: string,
  readonly stage: string | null,
  readonly reason: string,
};

export type SuiteSummary = {
  readonly files: number,
  readonly cases: number,
  readonly tests: number,
  readonly assertions: number,
};

export type Machine = {
  readonly platform: string,
  readonly arch: string,
  readonly cpu: string,
  readonly cores: number,
  readonly memoryBytes: number,
  readonly ci: string | null,
  readonly before: Quietness,
  readonly after: Quietness,
};

export type InstallFixture = { readonly dependencies: number, readonly manager: string };

export type Report = {
  readonly schema: 1,
  readonly arguments: $ReadOnlyArray<string>,
  readonly startedAt: string,
  readonly finishedAt: string,
  readonly provisional: boolean,
  readonly machine: Machine,
  readonly versions: { readonly [string]: string },
  readonly settings: {
    readonly runs: number,
    readonly warmup: number,
    readonly hmrEdits: number,
  },
  readonly tools: $ReadOnlyArray<string>,
  readonly skipped: $ReadOnlyArray<Skipped>,
  readonly fixtures: { readonly [string]: FixtureSummary },
  readonly suite: SuiteSummary | null,
  readonly install: InstallFixture | null,
  readonly results: $ReadOnlyArray<Row>,
};

/** Milliseconds as a person reads them: `8.4 ms`, `412 ms`, `3.01 s`. */
export function formatDuration(ms: number): string {
  if (ms >= 1000) {
    return `${(ms / 1000).toFixed(2)} s`;
  }
  if (ms >= 10) {
    return `${String(Math.round(ms))} ms`;
  }
  return `${ms.toFixed(1)} ms`;
}

function gigabytes(bytes: number): string {
  return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
}

function describeFixture(fixture: FixtureSummary): string {
  return (
    `${String(fixture.routes)} routes, ${String(fixture.components)} components ` +
    `(${String(fixture.clientComponents)} client), ${String(fixture.modules)} modules, ` +
    `${String(fixture.tests)} tests in ${String(fixture.testFiles)} files; ` +
    `${String(fixture.files)} files, ${String(fixture.lines)} lines`
  );
}

/** The report as a table, headed by the machine and the versions. */
export function formatTable(report: Report): string {
  const { machine } = report;
  const lines = [
    "",
    report.provisional
      ? "uf toolchain benchmark — PROVISIONAL: the machine was not quiet when the run started"
      : "uf toolchain benchmark",
    `  machine   ${machine.cpu}, ${String(machine.cores)} cores, ${gigabytes(machine.memoryBytes)}, ` +
      `${machine.platform} ${machine.arch}${machine.ci == null ? "" : `, ${machine.ci}`}`,
    `  load      ${machine.before.load.join(" ")} before (${String(machine.before.rustc)} rustc), ` +
      `${machine.after.load.join(" ")} after`,
    `  versions  ${Object.keys(report.versions)
      .map((name) => `${name} ${report.versions[name]}`)
      .join(", ")}`,
    `  runs      ${String(report.settings.runs)} timed after ${String(report.settings.warmup)} ` +
      `thrown away; ${String(report.settings.hmrEdits)} timed HMR edits`,
  ];
  for (const name of Object.keys(report.fixtures)) {
    lines.push(`  fixture   ${name}: ${describeFixture(report.fixtures[name])}`);
  }
  const suite = report.suite;
  if (suite != null) {
    lines.push(
      `  fixture   suite: ${String(suite.tests)} tests in ${String(suite.files)} files, ` +
        `${String(suite.assertions)} assertions`,
    );
  }
  const installed = report.install;
  if (installed != null) {
    lines.push(
      `  fixture   install: ${String(installed.dependencies)} direct dependencies, ` +
        `installed by ${installed.manager}`,
    );
  }
  for (const skipped of report.skipped) {
    lines.push(
      `  skipped   ${skipped.tool}${skipped.stage == null ? "" : ` ${skipped.stage}`}: ` +
        skipped.reason,
    );
  }
  lines.push("");

  const header = ["stage", "fixture", "command", "median", "min", "max", "cpu", "runs"];
  const body = report.results.map((row) => [
    row.stage,
    row.fixture,
    row.command,
    formatDuration(row.median),
    formatDuration(row.min),
    formatDuration(row.max),
    row.cpu == null ? "-" : formatDuration(row.cpu.median),
    String(row.runs),
  ]);
  const widths = header.map((cell, column) =>
    Math.max(cell.length, ...body.map((cells) => cells[column].length)),
  );
  const render = (cells: $ReadOnlyArray<string>): string =>
    `  ${cells
      .map((cell, column) =>
        column >= 3 ? cell.padStart(widths[column]) : cell.padEnd(widths[column]),
      )
      .join("  ")}`.trimEnd();
  lines.push(render(header));
  for (const cells of body) {
    lines.push(render(cells));
  }
  return `${lines.join("\n")}\n`;
}

/** Write `report` to `file` as the document described at the top of this file. */
export function writeReport(file: string, report: Report): void {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, `${JSON.stringify(report, null, 2)}\n`);
}
