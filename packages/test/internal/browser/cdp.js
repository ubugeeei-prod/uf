// @flow
import { spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { compareScreenshot } from "./screenshots.js";

// CDP replies are JSON selected by the command name. Keep that untyped wire
// boundary here; the application-facing page API has concrete Flow types.
type WireValue = $FlowFixMe;
export type ControlOptions = {|
  readonly root?: string,
  readonly baselines?: string,
  readonly threshold?: number,
  readonly executable?: string,
  readonly timeoutMs?: number,
|};
export type BrowserTransport = {|
  readonly id: string,
  readonly command: (
    id: string | null,
    method: string,
    args?: $ReadOnlyArray<WireValue>,
  ) => Promise<WireValue>,
  readonly close: () => Promise<void>,
|};
/** The page commands `connectPipe` answers over one browser connection. */
export type PipeController = {|
  readonly command: (
    id: string | null,
    method: string,
    args?: $ReadOnlyArray<WireValue>,
  ) => Promise<WireValue>,
  readonly closePages: () => Promise<void>,
|};
type Pending = {|
  timer: TimeoutID,
  resolve: (value: WireValue) => void,
  reject: (error: Error) => void,
|};

/** CDP over the private pipe: closing the test worker also closes Chromium. */
export function connectPipe(child: $FlowFixMe, options: ControlOptions = {}): PipeController {
  let sequence = 0;
  let buffered = "";
  let closed = false;
  const pending: Map<number, Pending> = new Map();
  const pages: Map<string, string> = new Map();
  const events: Map<string, Array<WireValue>> = new Map();
  const loaded: Map<string, Set<string>> = new Map();
  const output = child.stdio[4];
  output.setEncoding("utf8");
  output.on("data", (chunk) => {
    buffered += chunk;
    for (let end; (end = buffered.indexOf("\0")) !== -1; ) {
      const raw = buffered.slice(0, end);
      buffered = buffered.slice(end + 1);
      if (!raw) continue;
      const message = JSON.parse(raw);
      const task = pending.get(message.id);
      if (task) {
        pending.delete(message.id);
        clearTimeout(task.timer);
        if (message.error) task.reject(new Error(message.error.message));
        else task.resolve(message.result);
      } else if (message.sessionId && events.has(message.sessionId)) {
        if (message.method === "Page.lifecycleEvent" && message.params.name === "load")
          loaded.get(message.sessionId)?.add(message.params.loaderId);
        if (["Runtime.exceptionThrown", "Runtime.consoleAPICalled"].includes(message.method)) {
          const history = events.get(message.sessionId);
          if (history == null) continue;
          history.push({ method: message.method, ...message.params });
          if (history.length > 100) history.shift();
        }
      }
    }
  });
  const disconnect = () => {
    closed = true;
    for (const task of pending.values()) {
      clearTimeout(task.timer);
      task.reject(new Error("test browser closed"));
    }
    pending.clear();
  };
  child.once("exit", disconnect);
  child.once("error", disconnect);
  child.stdio[3].on("error", disconnect);
  function send(
    method: string,
    params: { readonly [string]: mixed } = {},
    sessionId?: string,
  ): Promise<WireValue> {
    if (closed) return Promise.reject(new Error("test browser closed"));
    const id = ++sequence;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        pending.delete(id);
        reject(new Error(`CDP timed out: ${method}`));
      }, options.timeoutMs ?? 15000);
      pending.set(id, { resolve, reject, timer });
      child.stdio[3].write(JSON.stringify({ id, method, params, sessionId }) + "\0");
    });
  }
  async function evaluate(sessionId: string, expression: string): Promise<WireValue> {
    const result = await send(
      "Runtime.evaluate",
      { expression, returnByValue: true, awaitPromise: true },
      sessionId,
    );
    if (result.exceptionDetails)
      throw new Error(
        result.exceptionDetails.exception?.description ?? result.exceptionDetails.text,
      );
    return result.result.value;
  }
  async function until(
    session: string,
    expression: string,
    description: string,
  ): Promise<WireValue> {
    const end = Date.now() + (options.timeoutMs ?? 15000);
    while (Date.now() < end) {
      try {
        const result = await evaluate(session, expression);
        if (result) return result;
      } catch (error) {
        if (!/context|navigat/i.test(error.message)) throw error;
      }
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
    throw new Error(`test browser timed out waiting for ${description}`);
  }
  async function point(session: string, selector: string): Promise<{| x: number, y: number |}> {
    return until(
      session,
      `(() => {
      const element = document.querySelector(${JSON.stringify(selector)});
      if (!element || element.disabled) return null;
      element.scrollIntoView({block:"center", inline:"center", behavior:"instant"});
      const box = element.getBoundingClientRect();
      if (!box.width || !box.height || getComputedStyle(element).visibility !== "visible") return null;
      const x = box.x + box.width / 2, y = box.y + box.height / 2;
      const top = document.elementFromPoint(x, y);
      return top && (top === element || element.contains(top)) ? {x,y} : null;
    })()`,
      `actionable ${selector}`,
    );
  }
  async function createPage(): Promise<string> {
    const { targetId } = await send("Target.createTarget", { url: "about:blank" });
    const { sessionId } = await send("Target.attachToTarget", { targetId, flatten: true });
    pages.set(targetId, sessionId);
    events.set(sessionId, []);
    loaded.set(sessionId, new Set());
    await send("Page.enable", {}, sessionId);
    await send("Page.setLifecycleEventsEnabled", { enabled: true }, sessionId);
    await send("Runtime.enable", {}, sessionId);
    return targetId;
  }
  async function command(
    id: string | null,
    method: string,
    args: $ReadOnlyArray<WireValue> = [],
  ): Promise<WireValue> {
    if (method === "create") return createPage();
    if (id == null) throw new Error("unknown test page");
    const session = pages.get(id);
    if (!session) throw new Error("unknown or closed test page");
    const [first, second] = args;
    switch (method) {
      case "visit": {
        const url = new URL(first);
        if (!["http:", "https:", "about:"].includes(url.protocol))
          throw new Error("test page visit supports HTTP(S) URLs and about:blank");
        const result = await send("Page.navigate", { url: url.href }, session);
        if (result.errorText) throw new Error(result.errorText);
        if (result.loaderId) {
          const deadline = Date.now() + (options.timeoutMs ?? 15000);
          while (loaded.get(session)?.has(result.loaderId) !== true) {
            if (closed || Date.now() >= deadline)
              throw new Error("test browser timed out waiting for document load");
            await new Promise((resolve) => setTimeout(resolve, 25));
          }
        }
        await until(session, "document.readyState === 'complete'", "document load");
        return;
      }
      case "setContent": {
        const { frameTree } = await send("Page.getFrameTree", {}, session);
        await send(
          "Page.setDocumentContent",
          { frameId: frameTree.frame.id, html: first },
          session,
        );
        return;
      }
      case "waitFor":
        await until(session, `!!document.querySelector(${JSON.stringify(first)})`, first);
        return;
      case "text":
        return evaluate(
          session,
          `document.querySelector(${JSON.stringify(first)})?.textContent ?? null`,
        );
      case "value":
        return evaluate(session, `document.querySelector(${JSON.stringify(first)})?.value ?? null`);
      case "url":
        return evaluate(session, "location.href");
      case "events":
        return [...(events.get(session) ?? [])];
      case "viewport": {
        const { width, height, deviceScaleFactor = 1 } = first;
        if (
          ![width, height, deviceScaleFactor].every(
            (value) => Number.isFinite(value) && value > 0,
          ) ||
          width > 8192 ||
          height > 8192 ||
          deviceScaleFactor > 4
        )
          throw new Error("invalid test viewport");
        await send(
          "Emulation.setDeviceMetricsOverride",
          { width, height, deviceScaleFactor, mobile: false },
          session,
        );
        return;
      }
      case "click": {
        const position = await point(session, first);
        await send(
          "Input.dispatchMouseEvent",
          { type: "mousePressed", ...position, button: "left", clickCount: 1 },
          session,
        );
        await send(
          "Input.dispatchMouseEvent",
          { type: "mouseReleased", ...position, button: "left", clickCount: 1 },
          session,
        );
        return;
      }
      case "fill": {
        await command(id, "click", [first]);
        await evaluate(
          session,
          `(() => {const input = document.querySelector(${JSON.stringify(first)}); input.focus(); if (!input.matches('input,textarea')) throw new Error('fill needs an input or textarea'); input.select();})()`,
        );
        await send("Input.insertText", { text: second }, session);
        return;
      }
      case "press": {
        const keys = {
          Enter: 13,
          Tab: 9,
          Escape: 27,
          Backspace: 8,
          ArrowLeft: 37,
          ArrowUp: 38,
          ArrowRight: 39,
          ArrowDown: 40,
          " ": 32,
        };
        if (!(first in keys)) throw new Error(`unsupported test key ${first}`);
        const params = {
          key: first,
          windowsVirtualKeyCode: keys[first],
          ...(first === "Enter" ? { text: "\r" } : first === " " ? { text: " " } : {}),
        };
        await send("Input.dispatchKeyEvent", { type: "keyDown", ...params }, session);
        await send(
          "Input.dispatchKeyEvent",
          { type: "keyUp", key: first, windowsVirtualKeyCode: keys[first] },
          session,
        );
        return;
      }
      case "tap": {
        const position = await point(session, first);
        await send("Emulation.setTouchEmulationEnabled", { enabled: true }, session);
        try {
          await send(
            "Input.dispatchTouchEvent",
            { type: "touchStart", touchPoints: [{ ...position, id: 1 }] },
            session,
          );
          await send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] }, session);
        } finally {
          await send("Emulation.setTouchEmulationEnabled", { enabled: false }, session);
        }
        return;
      }
      case "screenshot": {
        await evaluate(
          session,
          "document.fonts.ready.then(() => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r))))",
        );
        const { data } = await send(
          "Page.captureScreenshot",
          { format: "png", captureBeyondViewport: false },
          session,
        );
        return compareScreenshot(data, first, {
          root: options.root,
          threshold: second?.threshold ?? options.threshold,
          baselines: second?.baselines ?? options.baselines,
        });
      }
      case "close":
        await send("Target.closeTarget", { targetId: id });
        pages.delete(id);
        events.delete(session);
        loaded.delete(session);
        return;
      default:
        throw new Error(`unknown test browser command ${method}`);
    }
  }
  return {
    command,
    closePages: async () => {
      for (const id of [...pages.keys()]) await command(id, "close");
    },
  };
}

export async function launchBrowser(options: ControlOptions = {}): Promise<BrowserTransport> {
  const executable = options.executable ?? process.env.UF_BROWSER;
  if (!executable) throw new Error("Set UF_BROWSER to a Chromium executable for createBrowser()");
  const profile = mkdtempSync(path.join(tmpdir(), "uf-test-cdp-"));
  const child = spawn(
    executable,
    [
      "--headless=new",
      "--remote-debugging-pipe",
      `--user-data-dir=${profile}`,
      "--no-first-run",
      "--no-default-browser-check",
      "--disable-dev-shm-usage",
      "--disable-background-networking",
      "--password-store=basic",
      "about:blank",
    ],
    { stdio: ["ignore", "ignore", "pipe", "pipe", "pipe"] },
  );
  let stderr = "";
  child.stderr.on("data", (chunk) => {
    stderr = (stderr + chunk).slice(-8192);
  });
  const controller = connectPipe(child, options);
  const exited = new Promise((resolve) => {
    child.once("exit", resolve);
    child.once("error", resolve);
  });
  let closing: Promise<void> | void;
  const close = (): Promise<void> => {
    if (closing != null) return closing;
    const done = (async (): Promise<void> => {
      child.kill("SIGKILL");
      await exited;
      rmSync(profile, { recursive: true, force: true, maxRetries: 3 });
    })();
    closing = done;
    return done;
  };
  try {
    return { id: await controller.command(null, "create"), command: controller.command, close };
  } catch (error) {
    await close();
    throw new Error(`${error.message}\n${stderr}`);
  }
}
