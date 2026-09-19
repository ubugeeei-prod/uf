// @flow
import { it, expect } from "@uniflowed/test";
import { createBrowser } from "@uniflowed/test/browser";
import { browser, viewport, visit } from "@uniflowed/browser";
import { plan, snapshot, diff } from "@uniflowed/vrt";
it("compares the browser pixels with an explicit baseline", async () => {
  const page = await createBrowser();
  try {
    await page.viewport({ width: 320, height: 240 });
    await page.visit(new URL("./visual.html", import.meta.url).href);
    const result = await page.screenshot("solid-shape");
    expect(result.differentPixels).toBe(0);
  } finally { await page.close(); }
}, { timeout: 30000 });

it("executes a VRT plan through the shared browser front door", async () => {
  const value = { ...plan([snapshot("solid", "desktop")]), baselines: "__screenshots__" };
  const results = await diff(value, async () => {
    const page = await browser();
    await viewport(page, { width: 320, height: 240 });
    await visit(page, new URL("./visual.html", import.meta.url).href);
    return page;
  });
  expect(results.length).toBe(1);
  expect(results[0].differentPixels).toBe(0);
}, { timeout: 30000 });
