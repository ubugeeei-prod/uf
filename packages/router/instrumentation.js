// @flow

export type NavigationTiming = {|
  readonly kind: "document" | "router",
  readonly pathname: string,
  readonly duration: number,
  readonly status: "complete" | "error",
|};

export type ClientInstrumentation = {|
  readonly register?: () => void | Promise<void>,
  readonly onError?: (
    error: mixed,
    context: {| readonly source: "error" | "unhandledrejection" | "navigation" | "startup" |},
  ) => void | Promise<void>,
  readonly onNavigation?: (timing: NavigationTiming) => void | Promise<void>,
|};

const observers: Set<ClientInstrumentation> = new Set();

function notify(body: () => mixed): void {
  Promise.resolve()
    .then(body)
    .catch((error) => console.error("uf client instrumentation failed", error));
}

/** Installed by the client entry before hydration, with disposal during HMR. */
export async function installClientInstrumentation(
  hooks: ClientInstrumentation,
): Promise<() => void> {
  const error = (event: ErrorEvent) =>
    notify(() => hooks.onError?.(event.error ?? event.message, { source: "error" }));
  const rejection = (event: PromiseRejectionEvent) =>
    notify(() => hooks.onError?.(event.reason, { source: "unhandledrejection" }));
  window.addEventListener("error", error);
  window.addEventListener("unhandledrejection", rejection);
  observers.add(hooks);
  let performanceObserver = null;
  if (
    typeof PerformanceObserver === "function" &&
    PerformanceObserver.supportedEntryTypes.includes("navigation")
  ) {
    performanceObserver = new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        const timing: NavigationTiming = {
          kind: "document",
          pathname: new URL(entry.name).pathname,
          duration: entry.duration,
          status: "complete",
        };
        notify(() => hooks.onNavigation?.(timing));
      }
    });
    performanceObserver.observe({ type: "navigation", buffered: true });
  }
  const dispose = () => {
    window.removeEventListener("error", error);
    window.removeEventListener("unhandledrejection", rejection);
    observers.delete(hooks);
    performanceObserver?.disconnect();
  };
  try {
    await hooks.register?.();
  } catch (failure) {
    notify(() => hooks.onError?.(failure, { source: "startup" }));
    dispose();
    throw failure;
  }
  return dispose;
}

/** The router's asynchronous navigation work; a hard navigation has browser timing entries. */
export async function observeNavigation<T>(to: string, body: () => Promise<T>): Promise<T> {
  if (observers.size === 0) return body();
  const started = performance.now();
  const pathname = new URL(to, window.location.href).pathname;
  let status: NavigationTiming["status"] = "complete";
  try {
    return await body();
  } catch (error) {
    status = "error";
    for (const hooks of observers) notify(() => hooks.onError?.(error, { source: "navigation" }));
    throw error;
  } finally {
    const timing: NavigationTiming = {
      kind: "router",
      pathname,
      duration: performance.now() - started,
      status,
    };
    for (const hooks of observers) notify(() => hooks.onNavigation?.(timing));
  }
}
