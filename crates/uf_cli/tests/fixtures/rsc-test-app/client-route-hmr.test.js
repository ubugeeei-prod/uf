// @flow
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { it, expect } from "@uniflowed/test";
import { createTestApp } from "@uniflowed/test/app";
import { createBrowser } from "@uniflowed/test/browser";

it("keeps a client route's state across Fast Refresh in RSC mode", async () => {
  const directory = fileURLToPath(new URL("./app/hot-route/", import.meta.url));
  const file = path.join(directory, "$page.js");
  const source = (version: string) => `"use client";
import { useState, useEffect } from "react";
export default component HotRoute() {
  const [count, setCount] = useState(0);
  const [ready, setReady] = useState(false);
  useEffect(() => { setReady(true); }, []);
  return <button id="hot-route" data-ready={ready ? "yes" : "no"} data-version="${version}" onClick={() => setCount(count + 1)}>count: {count}</button>;
}
`;
  fs.mkdirSync(directory, { recursive: true });
  fs.writeFileSync(file, source("before"));
  let app;
  let page;
  try {
    app = await createTestApp({ root: new URL("./", import.meta.url), timeoutMs: 60000 });
    page = await createBrowser({ timeoutMs: 30000 });
    await page.visit(app.origin() + "/hot-route");
    await page.waitFor('#hot-route[data-ready="yes"]');
    await page.click("#hot-route");
    expect(await page.text("#hot-route")).toBe("count: 1");
    fs.writeFileSync(file, source("after"));
    await page.waitFor('#hot-route[data-version="after"]');
    expect(await page.text("#hot-route")).toBe("count: 1");
    const errors = (await page.events()).filter((event) => event.method === "Runtime.exceptionThrown" || event.type === "error");
    expect(errors).toEqual([]);
  } finally {
    await page?.close();
    await app?.close();
    fs.rmSync(directory, { recursive: true, force: true });
  }
}, { timeout: 90000 });
