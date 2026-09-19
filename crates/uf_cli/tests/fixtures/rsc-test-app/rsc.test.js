// @flow
import { it, expect } from "@uniflowed/test";
import { createTestApp } from "@uniflowed/test/app";
import { createBrowser } from "@uniflowed/test/browser";

it("renders nested async RSC, preserves request scopes, and hydrates a client boundary", async () => {
  const app = await createTestApp({ root: new URL("./", import.meta.url), timeoutMs: 60000 });
  let page;
  try {
    const response = await app.render("/?name=Grace", { headers: { cookie: "visitor=first" } });
    const html = await response.text();
    if (response.status !== 200) throw new Error(html + "\n" + JSON.stringify(app.events()));
    expect(html).toContain("Grace");
    expect(html).toContain("first");
    expect(html).toContain("pending data");
    expect(html).toContain("nested async data");
    const [first, second] = await Promise.all([
      app.flight("/?name=One", { headers: { cookie: "visitor=one" } }),
      app.flight("/?name=Two", { headers: { cookie: "visitor=two" } }),
    ]);
    const a = await first.text(), b = await second.text();
    expect(a).toContain("One"); expect(a).not.toContain("Two");
    expect(b).toContain("Two"); expect(b).not.toContain("One");
    expect(a).toContain("one"); expect(a).not.toContain("two");
    expect(b).toContain("two"); expect(b).not.toContain("one");
    expect(a).toContain("nested async data");
    expect(a).toContain("Counter.js");
    page = await createBrowser();
    await page.visit(app.origin() + "/?name=Hydrated");
    await page.waitFor('#counter[data-ready="yes"]');
    expect(await page.text("h1")).toBe("Hydrated");
    expect(await page.text("#resolved")).toBe("nested async data");
    await page.click("#counter");
    expect(await page.text("#counter")).toBe("count: 1");
    const errors = (await page.events()).filter((event) => event.method === "Runtime.exceptionThrown" || event.type === "error");
    expect(errors).toEqual([]);
  } finally {
    await page?.close();
    await app.close();
  }
  await expect(app.fetch("/")).rejects.toThrow("closed");
}, { timeout: 90000 });
