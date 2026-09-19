// @noflow
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { it, expect } from "@uniflowed/test";
import { createTestApp } from "@uniflowed/test/app";
import { createBrowser } from "@uniflowed/test/browser";

it("the official MCP client reads real SSR and browser hydration errors", async () => {
  const root = fileURLToPath(new URL("./", import.meta.url));
  fs.writeFileSync(new URL("AGENTS.md", import.meta.url), "Keep the application's own instructions.\n");
  const app = await createTestApp({ root, timeoutMs: 60000 });
  const client = new Client({name:"uf-fixture", version:"1"});
  let page;
  const call = async (name, args = {}) => {
    const result = await client.callTool({name, arguments: args});
    if (result.isError) throw new Error(JSON.stringify(result.content));
    return JSON.parse(result.content[0].text);
  };
  try {
    await client.connect(new StdioClientTransport({command:process.env.UF_BINARY, args:["--cwd", root, "mcp"]}));
    const tools = await client.listTools();
    expect(tools.tools.some((tool) => tool.name === "uf_dev_errors")).toBe(true);
    const response = await app.fetch("/boom?private=not-in-logs");
    await response.text();
    let errors = await call("uf_dev_errors");
    const runtime = errors.errors.find((entry) => entry.kind === "runtime" && entry.message.includes("mcp-runtime-fixture"));
    expect(runtime).toBeDefined();
    expect(runtime.requestId).toBe(response.headers.get("x-uf-request-id"));
    expect(runtime.stack).toContain("app/boom/$page.js");
    const logs = await call("uf_dev_logs");
    expect(logs.logs.some((entry) => entry.requestId === runtime.requestId)).toBe(true);
    expect(logs.logs.find((entry) => entry.event === "request" && entry.requestId === runtime.requestId).url).toBe("/boom");
    page = await createBrowser();
    await page.visit(app.origin() + "/");
    const until = Date.now() + 30000;
    let hydration;
    while (Date.now() < until) {
      errors = await call("uf_dev_errors");
      hydration = errors.errors.find((entry) => entry.kind === "hydration");
      if (hydration) break;
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    if (!hydration) throw new Error("missing hydration diagnostic: " + JSON.stringify({errors, events:app.events(), browser:await page.events()}));
    expect(hydration.detail.length).toBeGreaterThan(0);
    expect(typeof hydration.requestId).toBe("string");
    expect(await page.text("#mismatch")).toBe("client value");
    const routes = await call("uf_dev_routes");
    expect(routes.routes.map((route) => route.path)).toContain("/boom");
    const snapshot = JSON.parse(fs.readFileSync(new URL(".uf/dev-state.json", import.meta.url), "utf8"));
    const action = snapshot.actions.find((entry) => entry.export === "save");
    expect(action).toBeDefined();
    expect((await call("uf_dev_action", {id:action.id})).action.module).toBe(action.module);
    expect((await client.callTool({name:"uf_dev_action", arguments:{id:"0".repeat(64)}})).isError).toBe(true);
    const agents = fs.readFileSync(new URL("AGENTS.md", import.meta.url), "utf8");
    expect(agents).toContain("Keep the application's own instructions.");
    expect(agents).toContain("uf_dev_action");
    expect(agents).toContain("/tree/uf%40");
    await app.close();
    expect((await client.callTool({name:"uf_dev_errors", arguments:{}})).isError).toBe(true);
  } finally {
    await page?.close();
    await client.close();
    await app.close();
  }
}, { timeout: 120000 });
