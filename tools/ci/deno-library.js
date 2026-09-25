// @noflow
// Fixture processes need Node-equivalent filesystem/process access (including
// symlinks). The shim grants that only to this CI suite. Application permissions
// remain covered independently by crates/uf_cli/tests/deno_host.rs.
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

// Each exception below is tracked in ubugeeei-prod/uf#1433; remove it there
// when its cause is fixed.
const NATIVE =
  "Deno cannot load Rolldown's native addon after registerHooks; the Node library lanes cover these tests.";
const nativeFiles = new Set([
  "packages/router/base-path.test.js",
  "packages/router/error-boundary.test.js",
  "packages/router/navigation-cache.test.js",
  "packages/router/strict-mode.test.js",
  "packages/vite/a11y-audit.test.js",
  "packages/vite/barrel-imports.test.js",
  // Added in #1384, after the rest of this list; it imports Vite like them (#1433).
  "packages/vite/build-passes.test.js",
  "packages/vite/devtools.test.js",
  // Added in #1486; it builds the route table through Vite like the rest (#1433).
  "packages/vite/error-boundaries.test.js",
  "packages/vite/flight-dev-urls.test.js",
  "packages/vite/flight.test.js",
  "packages/vite/rsc-requirements.test.js",
  // The RSC HMR hook is in the Vite plugin and imports Rolldown (#1433).
  "packages/vite/rsc-hmr.test.js",
  "packages/vite/rsc-split.test.js",
  "tests/library/server-actions.test.js",
]);
const nativeCases = new Set([
  "packages/server/flight.test.js|the payload URL > is the same segment in the Vite plugin, which answers it under uf dev",
  "packages/vite/dev-head-transform.test.js|the head chunk `uf dev` hands to Vite > keeps the transform working on a document that stops inside the body",
  "packages/vite/dev-head-transform.test.js|the head chunk `uf dev` hands to Vite > moves an `injectTo: body` tag to the top of the body, and nothing else",
]);

export function classify(report) {
  const failures = [],
    skips = [];
  for (const file of report.fileReports) {
    if (file.status === "completed") continue;
    if (
      nativeFiles.has(file.file) &&
      file.status === "load-failed" &&
      file.reason.includes("Cannot find native binding.")
    ) {
      skips.push({ file: file.file, reason: NATIVE });
    } else failures.push({ file: file.file, reason: file.reason });
  }
  for (const test of report.tests) {
    if (test.status !== "failed") continue;
    const message = test.failures.map((f) => f.message).join("\n");
    let reason;
    if (
      test.failures.length === 1 &&
      nativeCases.has(`${test.file}|${test.name}`) &&
      message.includes("Cannot find native binding.")
    )
      reason = NATIVE;
    if (
      test.failures.length === 1 &&
      test.file === "packages/vite/relay.test.js" &&
      message.includes("globalsBuiltinLower is not iterable")
    )
      reason =
        "Deno's CommonJS interop does not expose Babel helper-globals while hooks are installed; Relay compilation is covered on Node.";
    if (
      test.failures.length === 1 &&
      test.file === "packages/react-testing/dom-storage.test.js" &&
      message.includes("Loading unprepared module:") &&
      message.includes("/packages/react/react,")
    )
      reason =
        "Deno cannot prepare this concurrent dynamic import of a Flow re-export from an external fixture; document storage probes are covered on Node.";
    (reason ? skips : failures).push({
      file: test.file,
      name: test.name,
      reason: reason ?? message,
    });
  }
  return { passed: report.passed, skipped: report.skipped, runtimeSkips: skips, failures };
}

async function main() {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
  const uf = path.resolve(process.env.UF_BINARY ?? path.join(root, "target/release/uf"));
  const realDeno =
    process.env.UF_DENO_BINARY ||
    spawnSync("sh", ["-c", "command -v deno"], { encoding: "utf8" }).stdout.trim();
  if (!realDeno || !fs.existsSync(uf)) throw new Error("UF_BINARY and Deno must be installed");
  const version = spawnSync(realDeno, ["--version"], { encoding: "utf8" });
  if (version.status !== 0 || !/^deno 2\.9\.7(?:\s|$)/.test(version.stdout))
    throw new Error(
      "This lane is pinned to Deno 2.9.7; re-audit named exceptions before changing the runtime",
    );
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), "uf-deno-library-"));
  const out = path.resolve(process.env.UF_DENO_REPORT_DIR ?? path.join(root, ".uf/deno-library"));
  fs.mkdirSync(out, { recursive: true });
  const quote = (value) => "'" + value.replaceAll("'", "'\\''") + "'";
  fs.writeFileSync(
    path.join(temporary, "deno"),
    `#!/usr/bin/env bash
if [ "$1" != run ]; then exec ${quote(realDeno)} "$@"; fi
shift
args=()
for arg in "$@"; do
  case "$arg" in --allow-read=*|--allow-write=*|--allow-env=*|--allow-run=*) ;; *) args+=("$arg") ;; esac
done
exec ${quote(realDeno)} run --allow-read --allow-write --allow-env --allow-run --allow-ffi --allow-sys=uid,homedir --allow-net=127.0.0.1,localhost,[::1] "\${args[@]}"
`,
    { mode: 0o755 },
  );
  const env = { ...process.env, PATH: `${temporary}${path.delimiter}${process.env.PATH ?? ""}` };
  for (const name of Object.keys(env)) if (/^(LD_|DYLD_)/.test(name)) delete env[name];
  const paths = process.argv.length > 2 ? process.argv.slice(2) : ["packages", "tests/library"];
  const discovery = spawnSync(uf, ["test", "--list", "--json", ...paths], {
    cwd: root,
    env,
    encoding: "utf8",
    maxBuffer: 8 * 1024 * 1024,
  });
  if (discovery.status !== 0) throw new Error("Could not discover library tests");
  const files = JSON.parse(discovery.stdout).files;
  if (
    !Array.isArray(files) ||
    files.length === 0 ||
    !files.every((file) => typeof file === "string")
  )
    throw new Error("Discovery returned no test files");
  fs.writeFileSync(path.join(out, "discovery.json"), discovery.stdout);
  // React renderer context and Deno's module graph can outlive a file. A fresh
  // process per file gives this lane isolation; explicit multi-file worker
  // protocol fixtures still exercise reuse inside their own Deno process.
  const results = new Array(files.length);
  let next = 0;
  const runOne = async (index) => {
    const stdout = fs.openSync(path.join(out, `${index}.json`), "w");
    const stderr = fs.openSync(path.join(out, `${index}.stderr.log`), "w");
    try {
      const status = await new Promise((resolve, reject) => {
        const child = spawn(
          uf,
          ["test", "--host", "deno", "--threads", "1", files[index], "--json"],
          {
            cwd: root,
            env,
            stdio: ["ignore", stdout, stderr],
            detached: true,
          },
        );
        const timeout = setTimeout(() => {
          try {
            process.kill(-child.pid, "SIGKILL");
          } catch {}
        }, 120_000);
        child.once("error", (error) => {
          clearTimeout(timeout);
          reject(error);
        });
        child.once("exit", (code, signal) => {
          clearTimeout(timeout);
          resolve({ code, signal });
        });
      });
      if (status.signal || ![0, 1].includes(status.code))
        throw new Error(`Deno process failed for ${files[index]}: ${status.signal ?? status.code}`);
      const result = JSON.parse(fs.readFileSync(path.join(out, `${index}.json`), "utf8"));
      if (result.host.kind !== "deno" || result.bailed)
        throw new Error(`Not a complete Deno run: ${files[index]}`);
      if (
        Boolean(result.success) !== (status.code === 0) ||
        (!result.success &&
          result.failed === 0 &&
          result.fileReports.every((file) => file.status === "completed"))
      )
        throw new Error(`uf refused a run without a classified test failure: ${files[index]}`);
      results[index] = result;
    } finally {
      fs.closeSync(stdout);
      fs.closeSync(stderr);
    }
  };
  const errors = [];
  try {
    await Promise.all(
      Array.from({ length: 4 }, async () => {
        while (next < files.length) {
          const index = next++;
          try {
            await runOne(index);
          } catch (error) {
            errors.push(String(error));
          }
        }
      }),
    );
  } finally {
    fs.rmSync(temporary, { recursive: true, force: true });
  }
  if (errors.length) throw new Error(errors.join("\n"));
  const raw = {
    command: "uf test --host deno",
    host: results[0].host,
    files: files.length,
    passed: results.reduce((sum, result) => sum + result.passed, 0),
    skipped: results.reduce((sum, result) => sum + result.skipped, 0),
    fileReports: results.flatMap((result) => result.fileReports),
    tests: results.flatMap((result) => result.tests),
  };
  fs.writeFileSync(path.join(out, "raw.json"), JSON.stringify(raw, null, 2) + "\n");
  const summary = { runtime: version.stdout.trim(), ...classify(raw) };
  fs.writeFileSync(path.join(out, "summary.json"), JSON.stringify(summary, null, 2) + "\n");
  for (const skip of summary.runtimeSkips)
    console.log(`SKIP ${skip.file}${skip.name ? ` > ${skip.name}` : ""}: ${skip.reason}`);
  for (const failure of summary.failures)
    console.error(
      `FAIL ${failure.file}${failure.name ? ` > ${failure.name}` : ""}: ${failure.reason.slice(0, 600)}`,
    );
  console.log(
    `Deno 2.9.7: ${summary.passed} passed; ${summary.skipped} declared skips; ${summary.runtimeSkips.length} named runtime exceptions; ${summary.failures.length} failures. Reports: ${out}`,
  );
  process.exitCode = summary.failures.length ? 1 : 0;
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url))
  await main();
