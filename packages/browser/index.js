// @flow
import { createBrowser } from "@uniflowed/test/browser";
import type { BrowserOptions, TestPage, Viewport, ScreenshotResult } from "@uniflowed/test/browser";
export type { Viewport, ScreenshotResult } from "@uniflowed/test/browser";
export type BrowserPlan = TestPage;
export type VisualSnapshot = {|
  readonly storyId: string,
  readonly viewport: string,
  readonly baseline: string,
|};
export function browser(options?: BrowserOptions): Promise<TestPage> {
  return createBrowser(options);
}
export function viewport(page: TestPage, size: Viewport): Promise<void> {
  return page.viewport(size);
}
export function visit(page: TestPage, url: string): Promise<void> {
  return page.visit(url);
}
export function screenshot(page: TestPage, name: string): Promise<ScreenshotResult> {
  return page.screenshot(name);
}
