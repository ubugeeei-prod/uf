// @flow

import { describe, expect, it } from "@uniflowed/test";
import { installDom } from "../react-testing/internal/dom.js";
import { installClientInstrumentation, observeNavigation } from "./instrumentation.js";

describe("client instrumentation", () => {
  it("observes startup, browser errors and navigation, and disposes its listeners", async () => {
    installDom();
    const events = [];
    const hooks = {
      async register() {
        events.push("started");
      },
      onError(error, context) {
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
    expect(events[3].pathname).toBe("/notes");
    expect(events[3].status).toBe("complete");
    expect(events[3].duration >= 0).toBe(true);
    dispose();
    window.dispatchEvent(new window.ErrorEvent("error", { error: new Error("ignored") }));
    await observeNavigation("/other", async () => {});
    await Promise.resolve();
    expect(events.length).toBe(4);
  });
});
