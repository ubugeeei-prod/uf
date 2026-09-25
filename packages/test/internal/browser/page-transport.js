// @flow
import type { BrowserTransport, ControlOptions } from "./cdp.js";
import { runnerFetch } from "./runner-fetch.js";

export async function createTransport(options: ControlOptions): Promise<BrowserTransport> {
  if (options.executable != null)
    throw new Error("uf test --browser selects its own UF_BROWSER executable");
  const token = (globalThis as $FlowFixMe)[Symbol.for("uf.test.browser.token")];
  if (typeof token !== "string")
    throw new Error("createBrowser needs uf test --browser or a Node test worker");
  async function command(
    id: string | null,
    method: string,
    args: $ReadOnlyArray<mixed> = [],
  ): Promise<$FlowFixMe> {
    // The platform's `fetch`, not the global a test's request mock replaced.
    const response = await runnerFetch("/uf-test/browser", {
      method: "POST",
      headers: { "content-type": "application/json", "uf-test-browser": token },
      body: JSON.stringify({ id, method, args }),
    });
    const result = await response.json();
    if (!response.ok || result.error)
      throw new Error(result.error ?? `browser command failed (${response.status})`);
    return result.value;
  }
  const id = await command(null, "create");
  let closed = false;
  return {
    id,
    command,
    close: async () => {
      if (closed) return;
      closed = true;
      await command(id, "close");
    },
  };
}
