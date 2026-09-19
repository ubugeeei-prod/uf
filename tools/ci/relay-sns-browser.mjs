// @noflow
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { spawn } from "node:child_process";
import { setTimeout as sleep } from "node:timers/promises";

// Like docs-hydration.sh, use the CI image's Chromium through its public CDP.
// No browser test framework or downloaded browser is part of the example.
export async function checkBrowser(origin, label, output) {
  const browser =
    process.env.UF_BROWSER ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
  const profile = path.join(output, `chrome-${label}`);
  const chrome = spawn(
    browser,
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
  let socket;
  let transcript = "";
  let evaluate;
  let send;
  const errors = [];
  const events = [];
  try {
    const wsUrl = await new Promise((resolve, reject) => {
      const timer = setTimeout(
        () => reject(new Error(`browser startup timeout: ${transcript}`)),
        15000,
      );
      chrome.once("error", reject);
      chrome.stderr.on("data", (data) => {
        transcript += data;
        const match = transcript.match(/DevTools listening on (ws:\/\/\S+)/);
        if (match) {
          clearTimeout(timer);
          resolve(match[1]);
        }
      });
      chrome.once("exit", (code) => {
        clearTimeout(timer);
        reject(new Error(`browser exited: ${code}`));
      });
    });
    socket = new WebSocket(wsUrl);
    await new Promise((resolve, reject) => {
      socket.addEventListener("open", resolve, { once: true });
      socket.addEventListener("error", reject, { once: true });
    });
    const pending = new Map();
    let nextId = 1;
    let graphqlPosts = 0;
    const documents = new Map();
    const bodies = [];
    let readBody;
    const requests = new Map();
    let lastNetwork = Date.now();
    socket.addEventListener("message", ({ data }) => {
      const message = JSON.parse(String(data));
      if (
        [
          "Runtime.executionContextCreated",
          "Runtime.executionContextDestroyed",
          "Runtime.executionContextsCleared",
          "Runtime.exceptionThrown",
        ].includes(message.method)
      )
        events.push({ at: Date.now(), method: message.method, params: message.params });
      if (message.method === "Network.requestWillBeSent" && message.params.type === "Document")
        events.push({ at: Date.now(), method: "document", url: message.params.request.url });
      if (message.id) {
        const task = pending.get(message.id);
        if (task) {
          pending.delete(message.id);
          clearTimeout(task.timer);
          if (message.error) task.reject(new Error(message.error.message));
          else task.resolve(message.result);
        }
      } else if (message.method === "Runtime.exceptionThrown") {
        errors.push(message.params.exceptionDetails);
      } else if (message.method === "Runtime.consoleAPICalled" && message.params.type === "error") {
        errors.push(message.params.args);
      } else if (message.method === "Network.requestWillBeSent") {
        if (message.params.type === "Document")
          documents.set(message.params.requestId, message.params.request.url);
        if (["Document", "Script", "Stylesheet"].includes(message.params.type)) {
          requests.set(message.params.requestId, message.params.request.url);
          lastNetwork = Date.now();
        }
        if (
          message.params.request.method === "POST" &&
          message.params.request.url === `${origin}/graphql`
        )
          graphqlPosts++;
      } else if (
        message.method === "Network.loadingFinished" ||
        message.method === "Network.loadingFailed"
      ) {
        requests.delete(message.params.requestId);
        if (
          message.method === "Network.loadingFinished" &&
          documents.has(message.params.requestId) &&
          readBody
        )
          bodies.push(readBody(message.params.requestId, documents.get(message.params.requestId)));
        lastNetwork = Date.now();
      }
    });
    send = (method, params = {}, sessionId) =>
      new Promise((resolve, reject) => {
        const id = nextId++;
        const timer = setTimeout(() => {
          pending.delete(id);
          reject(new Error(`CDP timeout: ${method}`));
        }, 15000);
        pending.set(id, { resolve, reject, timer });
        socket.send(JSON.stringify({ id, method, params, sessionId }));
      });
    const { targetId } = await send("Target.createTarget", { url: "about:blank" });
    const { sessionId } = await send("Target.attachToTarget", { targetId, flatten: true });
    const page = (method, params) => send(method, params, sessionId);
    readBody = async (requestId, url) => {
      try {
        const response = await page("Network.getResponseBody", { requestId });
        const name = new URL(url).pathname.replaceAll("/", "_");
        fs.writeFileSync(
          path.join(output, `${label}-document${name}.html`),
          response.base64Encoded ? Buffer.from(response.body, "base64") : response.body,
        );
      } catch {
        /* A document can be discarded by a later navigation. */
      }
    };
    await page("Page.enable");
    await page("Runtime.enable");
    await page("Network.enable");
    await page("Emulation.setDeviceMetricsOverride", {
      width: 1440,
      height: 1000,
      deviceScaleFactor: 1,
      mobile: false,
    });
    evaluate = async (expression) => {
      const result = await page("Runtime.evaluate", {
        expression,
        returnByValue: true,
        awaitPromise: true,
      });
      if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails));
      return result.result.value;
    };
    const waitFor = async (expression) => {
      for (let attempt = 0; attempt < 120; attempt++) {
        if (errors.length) throw new Error(`browser errors: ${JSON.stringify(errors)}`);
        if (await evaluate(expression)) return;
        await sleep(100);
      }
      throw new Error(
        `page condition timed out: ${expression}\n${await evaluate("document.body.innerText")}`,
      );
    };
    const settle = async () => {
      // The SSR form exists before its client reference has hydrated. Wait for
      // its module requests to settle before interacting with the form.
      const deadline = Date.now() + 60000;
      while (requests.size > 0 || Date.now() - lastNetwork < 250) {
        if (Date.now() > deadline)
          throw new Error(
            `client modules did not settle: ${JSON.stringify([...requests.values()])}`,
          );
        await sleep(50);
      }
    };
    const navigate = async (route, condition) => {
      await page("Page.navigate", { url: `${origin}${route}` });
      await waitFor(condition);
      await waitFor("document.readyState === 'complete'");
      await settle();
    };
    const fill = (selector, value) =>
      evaluate(
        `(() => { const element = document.querySelector(${JSON.stringify(selector)}); if (!element) throw Error('missing input'); const prototype = element.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype; Object.getOwnPropertyDescriptor(prototype, 'value').set.call(element, ${JSON.stringify(value)}); element.dispatchEvent(new Event('input', { bubbles: true })); })()`,
      );
    const click = (selector) =>
      evaluate(`document.querySelector(${JSON.stringify(selector)}).click()`);
    await navigate("/", "document.querySelectorAll('article').length >= 4");
    assert.equal(
      graphqlPosts,
      0,
      "RSC preload must hydrate without a duplicate browser GraphQL fetch",
    );
    await navigate("/signup", "document.querySelector('input[name=email]') != null");
    await fill("input[name=name]", `Smoke ${label}`);
    await fill("input[name=handle]", `smoke_${label}`);
    await fill("input[name=email]", `${label}@example.test`);
    await fill("input[name=password]", "local-smoke-passphrase");
    await click(".auth-form button");
    await waitFor("document.querySelector('#compose textarea') != null");
    await settle();
    const note = `Relay ${label} note`;
    await fill("#compose textarea", note);
    await waitFor(`document.querySelector('#compose').textContent.includes('${note.length}/500')`);
    await click("#compose button");
    await waitFor(
      `document.querySelector('article')?.textContent.includes(${JSON.stringify(note)})`,
    );
    await click("article button");
    await waitFor(
      "document.querySelector('article button')?.getAttribute('aria-pressed') === 'true'",
    );
    await waitFor("document.querySelector('article button')?.disabled === false");
    await click("article button");
    await waitFor(
      "document.querySelector('article button')?.getAttribute('aria-pressed') === 'false'",
    );
    // A client route transition exercises a second Flight payload in one Relay environment.
    await click('nav[aria-label="Primary navigation"] a[href="/messages"]');
    await waitFor("document.querySelector('textarea[aria-label=Message]') != null");
    await fill('textarea[aria-label="Message"]', `private ${label} message`);
    await click(".message-composer button");
    await waitFor(
      `document.querySelector('[role=log]')?.textContent.includes('private ${label} message')`,
    );
    await click('nav[aria-label="Primary navigation"] a[href="/settings"]');
    await waitFor(`document.querySelector('input[name=email]')?.value === '${label}@example.test'`);
    // Anonymous SSR must never receive the authenticated user's private settings.
    const anonymous = await fetch(`${origin}/settings`, { headers: { accept: "text/html" } });
    assert.equal(anonymous.status, 200);
    assert.equal((await anonymous.text()).includes(`${label}@example.test`), false);
    await navigate("/", "document.querySelector('#compose') != null");
    await page("Emulation.setDeviceMetricsOverride", {
      width: 390,
      height: 844,
      deviceScaleFactor: 1,
      mobile: true,
    });
    assert.equal(
      await evaluate("document.documentElement.scrollWidth <= innerWidth"),
      true,
      "mobile layout overflows",
    );
    const shot = await page("Page.captureScreenshot", { format: "png" });
    fs.writeFileSync(path.join(output, `${label}-mobile.png`), Buffer.from(shot.data, "base64"));
    await page("Emulation.setDeviceMetricsOverride", {
      width: 1440,
      height: 1000,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await click('button[aria-label="Sign out"]');
    await waitFor(
      "document.querySelector('a[href=\"/signup\"]') != null && document.querySelector('#compose') == null",
    );
    assert.deepEqual(errors, []);
    assert.ok(graphqlPosts >= 6, "expected real Relay mutations and a feed refresh");
    console.log(
      `${label}: RSC preload, signup, post, optimistic reaction, Flight navigation, private DM/settings, logout and mobile layout passed`,
    );
  } catch (error) {
    if (evaluate)
      fs.writeFileSync(
        path.join(output, `${label}-failure.txt`),
        String(await evaluate("document.body.innerText").catch(() => "page unavailable")),
      );
    if (evaluate)
      fs.writeFileSync(
        path.join(output, `${label}-failure.html`),
        String(
          await evaluate("document.documentElement.outerHTML").catch(() => "page unavailable"),
        ),
      );
    throw error;
  } finally {
    socket?.close();
    chrome.kill("SIGTERM");
    fs.writeFileSync(path.join(output, `${label}-chrome.log`), transcript);
    fs.writeFileSync(path.join(output, `${label}-events.json`), JSON.stringify(events, null, 2));
    if (chrome.exitCode == null && chrome.signalCode == null) {
      await Promise.race([
        new Promise((resolve) => chrome.once("exit", resolve)),
        sleep(2000).then(() => chrome.kill("SIGKILL")),
      ]);
    }
    fs.rmSync(profile, { recursive: true, force: true });
  }
}
