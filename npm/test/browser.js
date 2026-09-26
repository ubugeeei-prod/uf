// @flow
import { createTransport } from "./internal/browser/transport.js";

export type Viewport = {|
  readonly width: number,
  readonly height: number,
  readonly deviceScaleFactor?: number,
|};
export type ScreenshotResult = {|
  readonly baseline: string,
  readonly updated: boolean,
  readonly differentPixels: number,
|};
export type BrowserOptions = {|
  readonly executable?: string,
  readonly timeoutMs?: number,
|};
export type TestPage = {|
  readonly visit: (url: string) => Promise<void>,
  readonly setContent: (html: string) => Promise<void>,
  readonly waitFor: (selector: string) => Promise<void>,
  readonly text: (selector: string) => Promise<string | null>,
  readonly value: (selector: string) => Promise<string | null>,
  readonly url: () => Promise<string>,
  readonly click: (selector: string) => Promise<void>,
  readonly fill: (selector: string, text: string) => Promise<void>,
  readonly press: (key: string) => Promise<void>,
  readonly tap: (selector: string) => Promise<void>,
  readonly viewport: (viewport: Viewport) => Promise<void>,
  readonly screenshot: (
    name: string,
    options?: {| readonly threshold?: number, readonly baselines?: string |},
  ) => Promise<ScreenshotResult>,
  readonly events: () => Promise<$ReadOnlyArray<{ readonly [string]: mixed }>>,
  readonly close: () => Promise<void>,
|};

/** An isolated Chromium page; the test harness stays alive across navigations. */
export async function createBrowser(options?: BrowserOptions): Promise<TestPage> {
  const transport = await createTransport(options ?? {});
  const call = (method: string, args: $ReadOnlyArray<mixed> = []) =>
    transport.command(transport.id, method, args);
  return {
    visit: (url) => call("visit", [url]),
    setContent: (html) => call("setContent", [html]),
    waitFor: (selector) => call("waitFor", [selector]),
    text: (selector) => call("text", [selector]),
    value: (selector) => call("value", [selector]),
    url: () => call("url"),
    click: (selector) => call("click", [selector]),
    fill: (selector, text) => call("fill", [selector, text]),
    press: (key) => call("press", [key]),
    tap: (selector) => call("tap", [selector]),
    viewport: (viewport) => call("viewport", [viewport]),
    screenshot: (name, options) => call("screenshot", [name, options]),
    events: () => call("events"),
    close: transport.close,
  };
}
