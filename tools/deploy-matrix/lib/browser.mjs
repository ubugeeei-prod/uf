// @noflow
//
// The deploy matrix's one browser question: does the page a target served
// hydrate, stay interactive, and navigate on the client?
//
// Driven over the Chrome DevTools Protocol directly, like
// `tools/ci/relay-sns-browser.mjs` and `tools/ci/docs-hydration.sh`: the
// repository has no browser test framework and this does not add one. The
// browser is the pinned Chrome for Testing that `tools/ci/pinned-browser.sh`
// installs and names in `UF_BROWSER`.
//
// What "hydrated" means here is observable, not inferred:
//
// 1. `#hydration[data-hydrated="yes"]`, which only an effect sets, and an
//    effect runs only in a tree React hydrated;
// 2. a click on `#clicker` moves `#clicks`, so event handlers are attached;
// 3. a click on a `<Link>` changes the page **without a document request**,
//    with a window property set before the click still there after it, and
//    with a request for the destination's `__uf.flight` payload — the target
//    answered the RSC navigation;
// 4. optionally, the server actions from a hydrated page: the JSON path
//    (`#add`) and the enhanced form (`#save`).
//
// Any uncaught exception or `console.error` in the page fails the check,
// because a hydration mismatch is reported exactly that way.

import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { setTimeout as sleep } from "node:timers/promises";

/** How long any one wait in the page may take. */
const WAIT_MS = 20_000;

/** Start Chrome headless and resolve with its DevTools WebSocket URL. */
function launch(binary, profile) {
  const chrome = spawn(
    binary,
    [
      "--headless=new",
      "--no-first-run",
      "--no-default-browser-check",
      "--disable-dev-shm-usage",
      "--remote-debugging-port=0",
      `--user-data-dir=${profile}`,
      "about:blank",
    ],
    { stdio: ["ignore", "ignore", "pipe"] },
  );
  let transcript = "";
  const ready = new Promise((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new Error(`the browser did not start: ${transcript}`)),
      20_000,
    );
    chrome.once("error", reject);
    chrome.stderr.on("data", (data) => {
      transcript += data;
      const match = /DevTools listening on (ws:\/\/\S+)/.exec(transcript);
      if (match) {
        clearTimeout(timer);
        resolve(match[1]);
      }
    });
    chrome.once("exit", (code) => {
      clearTimeout(timer);
      reject(new Error(`the browser exited (${code}): ${transcript}`));
    });
  });
  return { chrome, ready };
}

/** A minimal CDP session over one page target. */
async function connect(url) {
  const socket = new WebSocket(url);
  await new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });
  let next = 1;
  const pending = new Map();
  const listeners = [];
  socket.addEventListener("message", ({ data }) => {
    const message = JSON.parse(String(data));
    if (message.id != null) {
      const task = pending.get(message.id);
      if (task == null) return;
      pending.delete(message.id);
      if (message.error) task.reject(new Error(`${task.method}: ${message.error.message}`));
      else task.resolve(message.result);
      return;
    }
    for (const listener of listeners) listener(message);
  });
  const send = (method, params = {}, sessionId) =>
    new Promise((resolve, reject) => {
      const id = next++;
      pending.set(id, { resolve, reject, method });
      socket.send(JSON.stringify({ id, method, params, sessionId }));
    });
  return { socket, send, on: (listener) => listeners.push(listener) };
}

/**
 * Run the browser check against `base`.
 *
 * @param {string} base the target's origin
 * @param {{ actions: boolean, binary?: string }} options `actions`: also click
 *   the two server actions (off where the matrix does not expect them to work)
 * @returns {Promise<string[]>} observations for the report
 */
export async function hydration(base, options) {
  const binary = options.binary ?? process.env.UF_BROWSER;
  assert.ok(binary, "no browser: set UF_BROWSER (CI runs tools/ci/pinned-browser.sh)");
  const profile = mkdtempSync(path.join(os.tmpdir(), "uf-deploy-matrix-chrome-"));
  const { chrome, ready } = launch(binary, profile);
  let cdp = null;
  try {
    cdp = await connect(await ready);
    const { targetId } = await cdp.send("Target.createTarget", { url: "about:blank" });
    const { sessionId } = await cdp.send("Target.attachToTarget", { targetId, flatten: true });
    const page = (method, params) => cdp.send(method, params, sessionId);

    const problems = [];
    const documents = [];
    const payloads = [];
    cdp.on((message) => {
      if (message.sessionId !== sessionId) return;
      if (message.method === "Runtime.exceptionThrown") {
        problems.push(
          message.params.exceptionDetails?.exception?.description ??
            message.params.exceptionDetails?.text,
        );
      } else if (message.method === "Runtime.consoleAPICalled" && message.params.type === "error") {
        problems.push(message.params.args.map((arg) => arg.value ?? arg.description).join(" "));
      } else if (message.method === "Network.requestWillBeSent") {
        const url = message.params.request.url;
        if (message.params.type === "Document") documents.push(url);
        if (url.includes("__uf.flight")) payloads.push(url);
      }
    });
    await page("Page.enable");
    await page("Runtime.enable");
    await page("Network.enable");

    const evaluate = async (expression) => {
      const result = await page("Runtime.evaluate", {
        expression,
        returnByValue: true,
        awaitPromise: true,
      });
      if (result.exceptionDetails)
        throw new Error(`evaluating ${expression}: ${JSON.stringify(result.exceptionDetails)}`);
      return result.result.value;
    };
    const waitFor = async (expression, what) => {
      const deadline = Date.now() + WAIT_MS;
      while (Date.now() < deadline) {
        assert.deepEqual(problems, [], `the page reported errors while waiting for ${what}`);
        if (await evaluate(expression)) return;
        await sleep(100);
      }
      const html = await evaluate("document.documentElement.outerHTML.slice(0, 1500)");
      assert.fail(`timed out waiting for ${what}\n  page: ${html}`);
    };
    const click = (selector) =>
      evaluate(`document.querySelector(${JSON.stringify(selector)}).click()`);
    const text = (selector) =>
      `(document.querySelector(${JSON.stringify(selector)})?.textContent ?? "")`;

    const notes = [];
    const started = performance.now();
    await page("Page.navigate", { url: new URL("/", base).href });
    await waitFor(
      `document.querySelector("#hydration")?.dataset.hydrated === "yes"`,
      "the home page to hydrate",
    );
    notes.push(`hydrated ${Math.round(performance.now() - started)} ms after navigating`);
    await click("#clicker");
    await waitFor(`${text("#clicks")} === "1"`, "a click to move the counter");

    // A client navigation: the marker survives, no document is requested, and
    // the destination's payload is.
    await evaluate("window.__matrixMarker = 'still here'");
    const documentsBefore = documents.length;
    await click('a[href="/posts/first"]');
    await waitFor(`${text("main h1")} === "post: first"`, "the link to render /posts/first");
    assert.equal(
      await evaluate("window.__matrixMarker"),
      "still here",
      "following the link reloaded the page",
    );
    assert.equal(
      documents.length,
      documentsBefore,
      `following the link requested a document: ${documents.slice(documentsBefore).join(", ")}`,
    );
    assert.ok(
      payloads.some((url) => url.includes("/posts/first")),
      `following the link fetched no payload for /posts/first (payloads: ${payloads.join(", ") || "none"})`,
    );
    notes.push("client navigation to /posts/first fetched its Flight payload, no document");

    if (options.actions) {
      await page("Page.navigate", { url: new URL("/actions", base).href });
      await waitFor(
        `document.querySelector("#hydration")?.dataset.hydrated === "yes" && document.querySelector("#add") != null`,
        "/actions to hydrate",
      );
      await click("#add");
      await waitFor(`${text("#total")} === "42"`, "the add action to answer 42");
      await evaluate(
        `(() => { const input = document.querySelector("#note"); const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set; set.call(input, "browser"); input.dispatchEvent(new Event("input", { bubbles: true })); })()`,
      );
      await click("#save");
      await waitFor(`${text("#saved")} === "saved: resworb"`, "the form action to save the note");
      notes.push("server actions answered from the hydrated page (JSON call and enhanced form)");
    }
    assert.deepEqual(problems, [], "the page reported errors");
    return notes;
  } finally {
    cdp?.socket.close();
    chrome.kill("SIGTERM");
    await Promise.race([new Promise((resolve) => chrome.once("exit", resolve)), sleep(3000)]);
    if (chrome.exitCode == null && chrome.signalCode == null) chrome.kill("SIGKILL");
    rmSync(profile, { recursive: true, force: true });
  }
}
