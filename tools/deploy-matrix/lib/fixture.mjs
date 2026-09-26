// @noflow
//
// Copies of the deploy-matrix fixture, with some of its features left out.
//
// The fixture (`../fixture`) is one application with every rendering mode in
// it. A target that refuses a mode by design is asked twice: once with the
// whole application, to prove the refusal happens and names the mode, and once
// with a copy that leaves the refused mode out, to prove everything else works
// there. Both copies are made here, from one table of which files make up
// which feature, so "left out" is a list somebody can read rather than a
// directory somebody pruned by hand.
//
// Copies live under `../.work/`, inside the repository, because the fixture
// imports `@uniflowed/*` and resolves it from the repository's `node_modules`.

import { cpSync, existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

/** `tools/deploy-matrix`. */
export const MATRIX_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

/** The committed fixture. */
export const FIXTURE_DIR = path.join(MATRIX_DIR, "fixture");

/** Where copies are built. Ignored by git. */
export const WORK_DIR = path.join(MATRIX_DIR, ".work");

/**
 * Every file of the fixture, by the feature it belongs to.
 *
 * `base` is what every copy has: the document, the client component that
 * proves hydration, the prerendered pages and the 404 — the part a static host
 * can serve. Every other feature is one or more routes a server answers.
 */
export const FEATURES = {
  base: [
    "package.json",
    "app.js",
    "app/$layout.js",
    "app/_client",
    "app/$page.js",
    "app/posts",
    "app/$not-found.js",
  ],
  ssr: ["app/ssr"],
  streaming: ["app/stream"],
  ppr: ["app/ppr"],
  isr: ["app/isr", "app/tagged", "app/api/revalidate"],
  actions: ["app/actions"],
  handlers: ["app/api/echo", "app/api/cookies"],
  middleware: ["app/gated"],
  redirects: ["app/old"],
  errors: ["app/boom"],
  s3cache: ["s3-cache.js"],
};

/** Every feature, which is the whole application. */
export const ALL_FEATURES = Object.keys(FEATURES);

/**
 * Make a copy of the fixture named `name` with only `features`, and the
 * configuration `config` — `"full"` for `../fixture/uf.config.js`, or a name
 * under `../variants/` — and answer its directory.
 *
 * Replaces whatever copy of that name was there, including its build, so a
 * copy never answers with a previous run's output.
 *
 * @param {string} name
 * @param {{ features: string[], config: string }} shape
 */
export function makeCopy(name, { features, config }) {
  const dir = path.join(WORK_DIR, name);
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  for (const feature of new Set(["base", ...features])) {
    const files = FEATURES[feature];
    if (files == null) throw new Error(`deploy-matrix: no feature named ${feature}`);
    for (const file of files) {
      const from = path.join(FIXTURE_DIR, file);
      if (!existsSync(from)) throw new Error(`deploy-matrix: the fixture has no ${file}`);
      mkdirSync(path.dirname(path.join(dir, file)), { recursive: true });
      cpSync(from, path.join(dir, file), { recursive: true });
    }
  }
  const configFile =
    config === "full"
      ? path.join(FIXTURE_DIR, "uf.config.js")
      : path.join(MATRIX_DIR, "variants", config, "uf.config.js");
  cpSync(configFile, path.join(dir, "uf.config.js"));
  // Git ignores `.work/`, and a copy says why it is there to whoever finds one.
  writeFileSync(
    path.join(dir, "COPY.txt"),
    `A copy of tools/deploy-matrix/fixture made by tools/deploy-matrix/run.mjs.\n` +
      `features: ${["base", ...features].join(", ")}\nconfig: ${config}\n`,
  );
  return dir;
}
