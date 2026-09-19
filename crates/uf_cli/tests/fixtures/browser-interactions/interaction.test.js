// @flow
import { it, expect } from "@uniflowed/test";
import { createBrowser } from "@uniflowed/test/browser";
it("uses trusted pointer, keyboard and touch input and survives navigation", async () => {
  const page = await createBrowser();
  try {
    await page.viewport({ width: 640, height: 480 });
    await page.visit(new URL("./first.html", import.meta.url).href);
    await page.click("#button");
    await page.fill("#name", "Flow");
    expect(await page.value("#name")).toBe("Flow");
    await page.press("Enter");
    await page.tap("#tap");
    const events = await page.text("#events");
    for (const event of ["click:true", "input:true", "keydown:true", "touchstart:true"]) expect(events).toContain(event);
    expect(events).not.toContain(":false");
    await page.visit(new URL("./second.html", import.meta.url).href);
    expect(await page.text("#next")).toBe("second document");
    expect(await page.url()).toContain("second.html");
  } finally { await page.close(); }
}, { timeout: 30000 });
