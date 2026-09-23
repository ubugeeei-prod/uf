#!/usr/bin/env node
// @noflow
//
// Join the deploy matrix's columns, and keep the documentation's table true.
//
//   node tools/deploy-matrix/report.mjs --results <dir>   every column's results against matrix.json
//   node tools/deploy-matrix/report.mjs --docs write      regenerate the deploy guide's table
//   node tools/deploy-matrix/report.mjs --docs check      fail when the guide's table is stale
//
// `--results` is the last job of CI's `deploy-matrix`: each target job wrote
// `<target>.json` (`./run.mjs`), and this fails unless every target has one
// and every cell in it has the status `./matrix.json` gives and behaved as that
// status requires. So a cell marked "Verified in CI" in the guide is one whose
// check passed in this run — the guide's table is `--docs` of the same JSON,
// and `tests/library/deploy-matrix.test.js` fails when it is not.

import { readFileSync, writeFileSync, appendFileSync, existsSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import { STATUSES, cellsOf, docsSection, loadMatrix, renderDocs } from "./lib/matrix.mjs";

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const GUIDE = path.join(REPO, "docs/app/guide/deploy/$page.mdx");

const argv = process.argv.slice(2);
const option = (name) => {
  const at = argv.indexOf(`--${name}`);
  return at === -1 ? null : (argv[at + 1] ?? null);
};

const matrix = loadMatrix();

const docs = option("docs");
if (docs != null) {
  const page = readFileSync(GUIDE, "utf8");
  const current = docsSection(page);
  if (current == null) {
    process.stderr.write(`${GUIDE} has no deploy-matrix section to ${docs}\n`);
    process.exit(1);
  }
  const wanted = renderDocs(matrix);
  if (docs === "write") {
    writeFileSync(GUIDE, page.replace(current, wanted));
    process.stdout.write(
      current === wanted
        ? "the deploy guide's matrix was already current\n"
        : "rewrote the deploy guide's matrix\n",
    );
  } else if (current !== wanted) {
    process.stderr.write(
      "the deploy guide's matrix is not tools/deploy-matrix/matrix.json; run `node tools/deploy-matrix/report.mjs --docs write`\n",
    );
    process.exit(1);
  } else {
    process.stdout.write("the deploy guide's matrix is current\n");
  }
  process.exit(0);
}

const resultsDir = option("results");
if (resultsDir == null) {
  process.stderr.write(
    "usage: node tools/deploy-matrix/report.mjs --results <dir> | --docs write|check\n",
  );
  process.exit(2);
}

const problems = [];
const lines = [
  "## Deployment compatibility matrix",
  "",
  `| Mode | ${matrix.targets.map((target) => target.title).join(" | ")} |`,
  `| --- | ${matrix.targets.map(() => "---").join(" | ")} |`,
];
const columns = new Map();
for (const target of matrix.targets) {
  const file = path.join(resultsDir, `${target.id}.json`);
  if (!existsSync(file)) {
    problems.push(`${target.id}: no results (${file})`);
    continue;
  }
  columns.set(target.id, JSON.parse(readFileSync(file, "utf8")));
}
for (const mode of matrix.modes) {
  const row = [];
  for (const target of matrix.targets) {
    const cell = cellsOf(matrix, target)[mode.id];
    const result = columns.get(target.id)?.cells?.[mode.id];
    if (result == null) {
      if (columns.has(target.id)) problems.push(`${target.id} × ${mode.id}: no result`);
      row.push("missing");
      continue;
    }
    if (result.status !== cell.status) {
      problems.push(
        `${target.id} × ${mode.id}: ran as ${result.status}, matrix.json says ${cell.status}`,
      );
    }
    if (!result.ok) problems.push(`${target.id} × ${mode.id}: ${result.outcome}`);
    const refusal = columns.get(target.id)?.cells?.[`${mode.id}#refusal`];
    if (refusal != null && !refusal.ok)
      problems.push(`${target.id} × ${mode.id} refusal: ${refusal.outcome}`);
    row.push(result.ok ? STATUSES[cell.status] : `**${result.outcome}**`);
  }
  lines.push(`| ${mode.title} | ${row.join(" | ")} |`);
}
const summary = `${lines.join("\n")}\n`;
process.stdout.write(summary);
if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, summary);
if (problems.length > 0) {
  process.stderr.write(`\nthe matrix does not hold:\n  ${problems.join("\n  ")}\n`);
  process.exit(1);
}
process.stdout.write("\nevery cell behaved as tools/deploy-matrix/matrix.json says\n");
