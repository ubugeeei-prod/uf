// Metro's device protocol, against the app's own CLI outside the workspace.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const net = require("node:net");
const { spawn } = require("node:child_process");
const { setTimeout: delay } = require("node:timers/promises");

async function main() {
  const [binary, app, provider] = process.argv.slice(2);
  const reservation = net.createServer();
  await new Promise((resolve) => reservation.listen(0, "127.0.0.1", resolve));
  const port = reservation.address().port;
  await new Promise((resolve) => reservation.close(resolve));
  const env = { ...process.env, EXPO_NO_TELEMETRY: "1", EXPO_OFFLINE: "1", BROWSER: "none" };
  // Metro disables its watcher in CI mode. Exercise its development mode.
  delete env.CI;
  const args = ["dev", "--target", "native", "--port", String(port)];
  if (provider === "expo") args.push("--", "--localhost");
  const child = spawn(binary, args, {
    cwd: app,
    env,
    detached: true,
    stdio: ["pipe", "pipe", "pipe"],
  });
  let log = "";
  for (const stream of [child.stdout, child.stderr])
    stream.on("data", (data) => {
      log = (log + data).slice(-1000000);
    });
  let socket;
  const page = path.join(app, "app/$page.js");
  const original = fs.readFileSync(page, "utf8");
  const base = `http://localhost:${port}`;
  async function until(
    probe /*: () => boolean | Promise<boolean> */,
    what /*: string */,
    budget /*: number */ = 90000,
  ) /*: Promise<void> */ {
    const deadline = Date.now() + budget;
    while (Date.now() < deadline) {
      if (child.exitCode != null) throw new Error(`native server exited: ${child.exitCode}`);
      if (await probe()) return;
      await delay(100);
    }
    throw new Error(`timed out: ${what}`);
  }
  try {
    await until(async () => {
      try {
        return (
          await (await fetch(`${base}/status`, { signal: AbortSignal.timeout(3000) })).text()
        ).includes("packager-status:running");
      } catch {
        return false;
      }
    }, "Metro status");
    const bundleURL = `${base}/index.bundle?platform=ios&dev=true&hot=true&minify=false`;
    const response = await fetch(bundleURL);
    const bundle = await response.text();
    assert.equal(response.status, 200, bundle.slice(0, 500));
    assert(bundle.includes("native-shared-page"));
    assert(/\$RefreshReg\$\([^,\n]+,\s*"Page"\)/.test(bundle), "no Page refresh registration");
    const messages = [];
    socket = new WebSocket(`ws://localhost:${port}/hot`);
    socket.addEventListener("message", (event) => messages.push(JSON.parse(String(event.data))));
    await new Promise((resolve, reject) => {
      socket.addEventListener("open", resolve, { once: true });
      socket.addEventListener("error", reject, { once: true });
    });
    socket.send(JSON.stringify({ type: "register-entrypoints", entryPoints: [bundleURL] }));
    await until(
      () => messages.some((event) => event.type === "bundle-registered"),
      "HMR registration",
    );
    messages.length = 0;
    fs.writeFileSync(page, original.replace("native-shared-page", "native-edited-page"));
    await until(
      () =>
        messages.some(
          (event) =>
            event.type === "update" && JSON.stringify(event.body).includes("native-edited-page"),
        ),
      "edited Flow module on /hot",
    );
    const update = messages.find(
      (event) =>
        event.type === "update" && JSON.stringify(event.body).includes("native-edited-page"),
    );
    if (update == null) throw new Error("the edit sent no update over /hot");
    assert(
      JSON.stringify(update.body).includes("$RefreshReg$"),
      "the edit has no refresh boundary",
    );
    assert(
      JSON.stringify(update.body).includes("compiler-runtime"),
      "the edit skipped the React Compiler",
    );

    const added = path.join(app, "app/added/$page.native.js");
    fs.mkdirSync(path.dirname(added), { recursive: true });
    fs.writeFileSync(added, "export default component Added() { return null; }\n");
    const table = path.join(app, "router.ios.js");
    await until(
      () => fs.existsSync(table) && fs.readFileSync(table, "utf8").includes('path: "/added"'),
      "added native route",
    );
    fs.rmSync(path.dirname(added), { recursive: true });
    await until(
      () => !fs.readFileSync(table, "utf8").includes('path: "/added"'),
      "removed native route",
    );
    console.log(
      `${provider}: /status, development bundle, compiled refresh update, and route add/remove passed`,
    );
  } catch (error) {
    process.stderr.write(log);
    throw error;
  } finally {
    socket?.close();
    fs.writeFileSync(page, original);
    try {
      process.kill(-child.pid, "SIGTERM");
    } catch {}
    child.stdin.destroy();
    child.stdout.destroy();
    child.stderr.destroy();
  }
}
main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
