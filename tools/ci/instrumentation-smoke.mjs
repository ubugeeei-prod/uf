// @noflow
import assert from "node:assert/strict";
import fs from "node:fs";
import net from "node:net";
import path from "node:path";
import { spawn, execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { setTimeout as sleep } from "node:timers/promises";

const root = fileURLToPath(new URL("../..", import.meta.url));
const fixture = path.join(root, "crates/uf_cli/tests/fixtures/instrumented-app");
const uf = process.env.UF_BINARY ?? path.join(root, "target/release/uf");
const logs = path.join(root, "output/instrumentation");
fs.mkdirSync(logs, { recursive: true });

function build(adapter) {
  execFileSync(uf, ["--cwd", fixture, "build", "--adapter", adapter], {
    cwd: root,
    stdio: "inherit",
  });
}

async function port() {
  const server = net.createServer();
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const chosen = server.address().port;
  await new Promise((resolve) => server.close(resolve));
  return chosen;
}

async function probe(label, command) {
  const chosen = await port();
  const origin = `http://127.0.0.1:${chosen}`;
  const { binary, args, cwd = fixture } = command(chosen);
  const log = fs.openSync(path.join(logs, `${label}.log`), "w");
  const child = spawn(binary, args, {
    cwd,
    env: { ...process.env, HOST: "127.0.0.1", PORT: String(chosen) },
    detached: true,
    stdio: ["ignore", log, log],
  });
  fs.closeSync(log);
  let failure;
  child.on("error", (error) => {
    failure = error;
  });
  async function request(url, init) {
    const response = await fetch(origin + url, { signal: AbortSignal.timeout(10000), ...init });
    return { status: response.status, text: await response.text() };
  }
  try {
    let ready = false;
    for (let attempt = 0; attempt < 120; attempt++) {
      if (failure || child.exitCode != null)
        throw failure ?? new Error(`${label} exited ${child.exitCode}`);
      try {
        ready = (await request("/observations")).status === 200;
      } catch {
        /* Wait for startup. */
      }
      if (ready) break;
      await sleep(250);
    }
    assert(ready, `${label} did not become ready`);
    assert.match(
      (await request("/", { headers: { accept: "text/html" } })).text,
      /instrumented RSC page/,
    );
    for (const route of ["fail-route", "fail-loader", "fail-render"]) {
      const response = await request(`/${route}`, { headers: { accept: "text/html" } });
      assert.equal(response.status, 500, `${label} ${route}`);
    }
    await request("/action", { headers: { accept: "text/html" } });
    const manifest = JSON.parse(
      fs.readFileSync(
        path.join(
          fixture,
          label === "dev" ? ".uf/rsc/uf-rsc-manifest.json" : ".uf/build/meta/uf-rsc-manifest.json",
        ),
        "utf8",
      ),
    );
    const action = manifest.serverActions.find((entry) => entry.export === "failAction");
    assert(action, "fixture action was not discovered");
    assert.equal(
      (
        await request("/action", {
          method: "POST",
          headers: { origin, "content-type": "application/json", "uf-action": action.id },
          body: JSON.stringify({ args: [] }),
        })
      ).status,
      500,
    );
    let observations;
    for (let attempt = 0; attempt < 30; attempt++) {
      observations = JSON.parse((await request("/observations")).text);
      if (observations.errors.length >= 4) break;
      await sleep(50);
    }
    assert.equal(observations.starts, 1, `${label}: startup should run once`);
    for (const phase of ["route", "loader", "render", "action"]) {
      const matching = observations.errors.filter((error) => error.message === `${phase} failure`);
      assert.equal(
        matching.length,
        1,
        `${label}: exactly one ${phase} error: ${JSON.stringify(observations.errors)}`,
      );
      assert.equal(matching[0].phase, phase);
      assert(matching[0].requestId);
    }
    for (const phase of ["request", "middleware", "route", "loader", "render", "action", "fetch"]) {
      assert(
        observations.spans.some((span) => span.name === `uf.${phase}`),
        `${label}: missing uf.${phase}`,
      );
    }
    const outgoing = observations.spans.find((span) => span.name === "uf.fetch");
    const render = observations.spans.find((span) => span.spanId === outgoing.parentId);
    assert.equal(render?.name, "uf.render", `${label}: async RSC fetch must belong to its render`);
    const incoming = observations.spans.find((span) => span.spanId === render.parentId);
    assert.equal(incoming?.name, "uf.request");
    assert.equal(incoming.attributes["http.request.method"], "GET");
    console.log(`ok ${label}: startup, all error phases and nested OpenTelemetry spans`);
  } catch (error) {
    console.error(fs.readFileSync(path.join(logs, `${label}.log`), "utf8"));
    throw error;
  } finally {
    if (child.pid) {
      try {
        process.kill(-child.pid, "SIGTERM");
      } catch {
        /* Already stopped. */
      }
      await Promise.race([new Promise((resolve) => child.once("close", resolve)), sleep(2000)]);
      try {
        process.kill(-child.pid, "SIGKILL");
      } catch {
        /* Already stopped. */
      }
    }
  }
}

build("node");
for (const mode of ["dev", "start", "preview"]) {
  await probe(mode, (port) => ({
    binary: uf,
    args: [mode, "--host", "127.0.0.1", "--port", String(port)],
  }));
}
await probe("node", () => ({
  binary: process.execPath,
  args: ["server.js"],
  cwd: path.join(fixture, ".uf/deploy/node"),
}));
if (process.env.UF_SKIP_EDGE !== "1") {
  build("edge");
  await probe("edge", (port) => ({
    binary: "npx",
    args: [
      "--yes",
      "wrangler@4.128.0",
      "dev",
      "--local",
      "--ip",
      "127.0.0.1",
      "--port",
      String(port),
    ],
    cwd: path.join(fixture, ".uf/deploy/edge"),
  }));
}
