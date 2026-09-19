// @flow
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import path from "node:path";

export type AppTestOptions = {|
  readonly root: string | URL,
  readonly binary?: string,
  readonly env?: { readonly [string]: string },
  readonly timeoutMs?: number,
|};

export type AppRequest = {|
  readonly method?: string,
  readonly headers?: { readonly [string]: string },
  readonly body?: string,
  readonly signal?: AbortSignal,
|};

export type TestApp = {|
  /** The live URL, including after an environment-triggered restart. */
  readonly origin: () => string,
  readonly fetch: (pathname: string, options?: AppRequest) => Promise<Response>,
  readonly render: (pathname: string, options?: AppRequest) => Promise<Response>,
  /** The actual React Flight stream, with request headers and Suspense intact. */
  readonly flight: (pathname: string, options?: AppRequest) => Promise<Response>,
  readonly events: () => $ReadOnlyArray<{ readonly [string]: mixed }>,
  readonly close: () => Promise<void>,
|};

/** Start the application's real web pipeline in an isolated process for `uf test`. */
export async function createTestApp(options: AppTestOptions): Promise<TestApp> {
  const root = path.resolve(
    options.root instanceof URL ? fileURLToPath(options.root.href) : options.root,
  );
  const timeout = options.timeoutMs ?? 30000;
  if (!Number.isFinite(timeout) || timeout <= 0)
    throw new TypeError("test app timeout must be positive");
  const child = spawn(
    options.binary ?? process.env.UF_BINARY ?? "uf",
    [
      "--cwd",
      root,
      "dev",
      "--json",
      "--parent-pipe",
      "--target",
      "web",
      "--host",
      "127.0.0.1",
      "--port",
      "0",
    ],
    { env: { ...process.env, ...options.env }, stdio: ["pipe", "pipe", "pipe"] },
  );
  const events: Array<{ readonly [string]: mixed }> = [];
  let transcript = "";
  let origin = "";
  let ended = false;
  const exited = new Promise<void>((resolve) =>
    child.once("exit", () => {
      ended = true;
      resolve();
    }),
  );
  let closing = null;
  const close = (): Promise<void> => {
    if (closing != null) return closing;
    closing = (async () => {
      if (ended) return;
      child.stdin?.end();
      const timer = setTimeout(() => child.kill("SIGKILL"), 3000);
      try {
        await exited;
      } finally {
        clearTimeout(timer);
      }
    })();
    return closing;
  };
  child.stderr?.on("data", (chunk) => {
    transcript = (transcript + String(chunk)).slice(-16384);
  });
  const lines = createInterface({ input: child.stdout });
  try {
    await new Promise<void>((resolve, reject) => {
      const timer = setTimeout(
        () => reject(new Error(`test app did not start within ${timeout}ms\n${transcript}`)),
        timeout,
      );
      const fail = (error: mixed) => {
        clearTimeout(timer);
        reject(error);
      };
      child.once("error", (error) => {
        ended = true;
        fail(error);
      });
      child.once("exit", (code) =>
        fail(new Error(`test app exited (${String(code)})\n${transcript}`)),
      );
      lines.on("line", (line) => {
        let event;
        try {
          event = JSON.parse(line);
        } catch {
          transcript = (transcript + line + "\n").slice(-16384);
          return;
        }
        events.push(event);
        if (events.length > 200) events.shift();
        if (event.event === "listening" && typeof event.local?.[0] === "string") {
          origin = new URL(event.local[0]).origin;
          clearTimeout(timer);
          resolve();
        } else if (event.event === "error") {
          fail(new Error(`test app: ${String(event.message)}`));
        }
      });
    });
  } catch (error) {
    await close();
    throw error;
  }
  function address(pathname: string): URL {
    if (ended || closing != null) throw new Error("test app is closed");
    if (!pathname.startsWith("/") || pathname.startsWith("//") || pathname.includes("\\")) {
      throw new TypeError("test app needs an absolute path on its own origin");
    }
    const url = new URL(pathname, origin);
    if (url.origin !== origin) throw new TypeError("test app cannot fetch another origin");
    return url;
  }
  const request = async (url: URL, init?: AppRequest): Promise<Response> => {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), timeout);
    try {
      return await fetch(url.href, {
        ...init,
        headers: { ...init?.headers },
        signal:
          init?.signal == null
            ? controller.signal
            : AbortSignal.any([init.signal, controller.signal]),
      });
    } finally {
      clearTimeout(timer);
    }
  };
  return {
    origin: () => origin,
    fetch: async (pathname, init) => request(address(pathname), init),
    render: async (pathname, init) => {
      const headers = new Headers({ ...init?.headers });
      headers.set("accept", "text/html");
      return request(address(pathname), { ...init, headers: Object.fromEntries(headers) });
    },
    flight: async (pathname, init) => {
      const url = address(pathname);
      url.pathname = `${url.pathname.replace(/\/$/, "")}/__uf.flight`;
      const response = await request(url, init);
      if (!(response.headers.get("content-type") ?? "").includes("text/x-component")) {
        throw new Error(
          `test app returned ${response.status} without a Flight stream; enable RSC for this route`,
        );
      }
      return response;
    },
    events: () => [...events],
    close,
  };
}
