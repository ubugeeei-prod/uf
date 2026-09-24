// @flow

import { describe, expect, it } from "@uniflowed/test";
import { installDom } from "../react-testing/internal/dom.js";
import { installClientInstrumentation, observeNavigation } from "./instrumentation.js";
import type { ClientInstrumentation, NavigationTiming } from "./instrumentation.js";

describe("client instrumentation", () => {
  it("reports a failed registration without rejecting the client entry", async () => {
    installDom();
    const failure = new Error("telemetry unavailable");
    const events = [];
    const dispose = await installClientInstrumentation({
      async register() {
        throw failure;
      },
      onError(error, context) {
        events.push({ error, source: context.source });
      },
    });
    expect(typeof dispose).toBe("function");
    await Promise.resolve();
    expect(events).toEqual([{ error: failure, source: "startup" }]);
    window.dispatchEvent(new window.ErrorEvent("error", { error: new Error("ignored") }));
    await Promise.resolve();
    expect(events.length).toBe(1);
    dispose();
  });

  it("observes startup, browser errors and navigation, and disposes its listeners", async () => {
    installDom();
    const events: Array<string | NavigationTiming> = [];
    const hooks: ClientInstrumentation = {
      async register() {
        events.push("started");
      },
      onError(_error, context) {
        events.push(context.source);
      },
      onNavigation(timing) {
        events.push(timing);
      },
    };
    const dispose = await installClientInstrumentation(hooks);
    expect(events).toEqual(["started"]);
    window.dispatchEvent(new window.ErrorEvent("error", { error: new Error("browser failure") }));
    const rejected = new window.Event("unhandledrejection");
    rejected.reason = new Error("promise failure");
    window.dispatchEvent(rejected);
    expect(await observeNavigation("/notes?secret=hidden", async () => "done")).toBe("done");
    await Promise.resolve();
    expect(events[1]).toBe("error");
    expect(events[2]).toBe("unhandledrejection");
    const timing = events[3];
    if (typeof timing === "string") {
      throw new Error(`expected a navigation timing, got ${timing}`);
    }
    expect(timing.pathname).toBe("/notes");
    expect(timing.status).toBe("complete");
    expect(timing.duration >= 0).toBe(true);
    dispose();
    window.dispatchEvent(new window.ErrorEvent("error", { error: new Error("ignored") }));
    await observeNavigation("/other", async () => {});
    await Promise.resolve();
    expect(events.length).toBe(4);
  });
});
