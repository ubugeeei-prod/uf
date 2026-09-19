// @noflow
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawn, execFileSync } from "node:child_process";
import { setTimeout as sleep } from "node:timers/promises";
import { checkBrowser } from "./relay-sns-browser.mjs";

const root = fileURLToPath(new URL("../..", import.meta.url));
const app = path.join(root, "examples/simple-sns-graphql");
const uf = process.env.UF_BINARY ?? path.join(root, "target/release/uf");
const output = path.join(root, "output/relay-sns");
fs.mkdirSync(output, { recursive: true });
const children = [];
const env = {
  ...process.env,
  SNS_GRAPHQL_ENDPOINT: "http://127.0.0.1:4193",
  SNS_GRAPHQL_LISTEN: "127.0.0.1:4193",
};
function run(binary, args, cwd = app) {
  execFileSync(binary, args, { cwd, env, stdio: "inherit" });
}
function start(name, binary, args, cwd = app) {
  const log = fs.openSync(path.join(output, `${name}.log`), "w");
  const child = spawn(binary, args, { cwd, env, detached: true, stdio: ["ignore", log, log] });
  fs.closeSync(log);
  children.push(child);
  child.once("error", (error) => {
    console.error(error);
    process.exitCode = 1;
  });
  return child;
}
async function ready(url, child, method = "GET") {
  for (let attempt = 0; attempt < 120; attempt++) {
    if (child.exitCode != null) throw new Error(`${url}: server exited ${child.exitCode}`);
    try {
      const response = await fetch(url, {
        method,
        headers: { "content-type": "application/json", accept: "text/html" },
        signal: AbortSignal.timeout(10000),
      });
      await response.text();
      if (method === "GET" ? response.ok : response.status === 400) return;
    } catch {
      /* Startup is bounded below. */
    }
    await sleep(250);
  }
  throw new Error(`${url}: server did not become ready; see ${output}`);
}
try {
  run("npm", ["ci", "--no-audit", "--no-fund"]);
  run(process.execPath, ["tools/relay.mjs", "--validate"]);
  run("go", ["test", "-race", "./..."], path.join(app, "backend"));
  run(uf, ["check"]);
  run(uf, ["lint"]);
  run(uf, ["build"]);
  const backend = start("backend", "go", ["run", "."], path.join(app, "backend"));
  await ready(env.SNS_GRAPHQL_ENDPOINT, backend, "POST");
  for (const [label, command, port] of [
    ["dev", "dev", 4191],
    ["production", "start", 4192],
  ]) {
    const server = start(label, uf, [command, "--host", "127.0.0.1", "--port", String(port)]);
    const origin = `http://127.0.0.1:${port}`;
    await ready(origin, server);
    const csrf = await fetch(`${origin}/graphql`, {
      method: "POST",
      headers: { "content-type": "application/json", origin: "https://another.example" },
      body: '{"query":"{viewer{id}}"}',
    });
    assert.equal(csrf.status, 403);
    const oversized = await fetch(`${origin}/graphql`, {
      method: "POST",
      headers: { "content-type": "application/json", origin },
      body: "x".repeat(65537),
    });
    assert.equal(oversized.status, 413);
    await checkBrowser(origin, label, output);
  }
} finally {
  for (const child of children.reverse()) {
    if (child.pid) {
      try {
        process.kill(-child.pid, "SIGTERM");
      } catch {
        /* Already exited. */
      }
    }
  }
}
