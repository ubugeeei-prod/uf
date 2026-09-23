// @flow
//
// The deploy guide's compatibility matrix is `tools/deploy-matrix/matrix.json`,
// rendered — never a table somebody edited by hand.
//
// A cell in that table says "Verified in CI", "Unsupported (rejected)", "Not
// emulated" or "Planned", and each of those is a promise about what CI's
// `Deploy matrix` job does: runs the check and needs it to pass, builds the
// feature and needs the build to refuse it by name, runs the check and needs it
// to fail the documented way, or does nothing yet. The job reads the JSON; this
// test makes the page read it too, so the two cannot drift apart. The other half
// — that every cell's evidence exists in a run — is
// `tools/deploy-matrix/report.mjs --results`, the job's last step.
//
// It also holds the matrix to its own rules, which `loadMatrix` enforces: a
// rejected cell names the refusal that proves it, and a cell that is not
// verified says why.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "@uniflowed/test";

// $FlowFixMe[untyped-import] -- Node scripts under tools/, deliberately not Flow.
import { CHECKS } from "../../tools/deploy-matrix/lib/checks.mjs";
// $FlowFixMe[untyped-import]
import {
  cellsOf,
  docsSection,
  loadMatrix,
  renderDocs,
} from "../../tools/deploy-matrix/lib/matrix.mjs";

const REPO = path.resolve(path.dirname(fileURLToPath(String(import.meta.url))), "..", "..");
const GUIDE = path.join(REPO, "docs/app/guide/deploy/$page.mdx");

describe("the deployment compatibility matrix", () => {
  const matrix = loadMatrix();

  it("is the table in the deploy guide", () => {
    const section = docsSection(fs.readFileSync(GUIDE, "utf8"));
    expect(section).toBe(renderDocs(matrix));
  });

  it("covers every adapter uf implements", () => {
    // `static` included: a target that can only refuse is still a column, and
    // its refusals are cells.
    expect(matrix.targets.map((target) => target.adapter).sort()).toEqual(
      ["bun", "container", "deno", "edge", "node", "serverless", "static", "vercel"].sort(),
    );
  });

  it("has a check or a refusal behind every cell that claims one", () => {
    // The browser's check is `lib/browser.mjs`; every other one is in `CHECKS`.
    const checks = new Set([...Object.keys(CHECKS), "hydration"]);
    const unbacked = [];
    for (const target of matrix.targets) {
      for (const [mode, cell] of Object.entries(cellsOf(matrix, target))) {
        const status = (cell: $FlowFixMe).status;
        if ((status === "verified" || status === "not-emulated") && !checks.has(mode)) {
          unbacked.push(`${target.id} × ${mode}`);
        }
      }
    }
    expect(unbacked).toEqual([]);
  });
});
