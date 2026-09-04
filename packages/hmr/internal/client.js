// @flow
//
// `@uniflowed/core/hmr` — the browser half of `uf dev`'s update channel.
//
// The server decides *what* went stale; this module only applies the answer.
// It opens one `EventSource` on `/__uf/hmr`, re-imports the modules an update
// names, and hands each accepting module to the refresh handler it registered.
// A module that cannot be swapped in place falls back to a full reload — and
// says so, loudly, because a page that reloads for reasons nobody can see is
// how people stop trusting hot reloading.
//
// Nothing here runs when the module is imported. `connect()` is the only thing
// that opens a socket, so an application that never calls it pays nothing, and
// a bundler that finds no call can drop the whole module.

import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/hmr";

/** The request target `uf dev` serves the update stream on. */
export const HMR_ENDPOINT: string = "/__uf/hmr";

/** The `event:` name every update frame carries. */
export const UPDATE_EVENT: string = "uf:update";

/** Prefix on every line this runtime writes to the console. */
export const LOG_PREFIX: string = "[uf hmr]";

/** What happened to the file the server saw change. */
export type ChangeKind = "created" | "modified" | "deleted";

/** What the browser and the server have to do about it. */
export type UpdateKind = "inert" | "hot" | "route" | "hot-and-route" | "full-reload";

/** Why an update could not be applied in place. */
export type ReloadReason =
  | "no-accepting-boundary"
  | "module-removed"
  | "depth-exceeded"
  | "unservable"
  | "too-many-modules";

/** Why a module is in an update. */
export type UpdateRole = "boundary" | "dependency";

/** One module to re-fetch. */
export type UpdateModule = {
  readonly path: string,
  readonly url: string,
  readonly role: UpdateRole,
};

/** One update, exactly as `uf_devserver::hmr::HmrUpdate` serializes it. */
export type HmrUpdate = {
  readonly id: number,
  readonly path: string,
  readonly change: ChangeKind,
  readonly kind: UpdateKind,
  readonly reason?: ReloadReason,
  readonly modules: $ReadOnlyArray<UpdateModule>,
  readonly routes: $ReadOnlyArray<string>,
  readonly elapsedMicros: number,
};

/**
 * Re-render a module in place.
 *
 * Receives the freshly evaluated module namespace. A React component module
 * registers one of these; anything else does not, and the update escalates to a
 * reload rather than leaving a stale binding behind.
 */
export type RefreshHandler = (next: mixed) => void;

/** What `connect` hands back. */
export type HmrClient = {
  readonly accept: (modulePath: string, handler: RefreshHandler) => void,
  readonly apply: (update: HmrUpdate) => Promise<UpdateKind>,
  readonly close: () => void,
  readonly applied: () => number,
};

/** How to open the channel. */
export type ConnectOptions = {
  readonly endpoint?: string,
  readonly onUpdate?: (update: HmrUpdate) => void,
  readonly onReload?: (reason: string) => void,
};

/**
 * Parse one `data:` payload into an update.
 *
 * Returns `null` for anything that is not the shape the server sends, so a
 * malformed frame is dropped rather than half-applied. The channel is
 * same-origin and server-controlled, but a client that trusts its input shape
 * is a client that throws inside an event handler.
 */
export function parseUpdate(data: string): HmrUpdate | null {
  let parsed: mixed = null;
  try {
    parsed = JSON.parse(data);
  } catch (error) {
    return null;
  }
  if (parsed == null || typeof parsed !== "object") {
    return null;
  }
  const candidate: { readonly [string]: mixed } = parsed as $FlowFixMe;
  if (typeof candidate.kind !== "string" || typeof candidate.path !== "string") {
    return null;
  }
  if (!Array.isArray(candidate.modules) || !Array.isArray(candidate.routes)) {
    return null;
  }
  return parsed as $FlowFixMe;
}

/**
 * Whether an update means the page has to be thrown away and rebuilt.
 */
export function isFullReload(update: HmrUpdate): boolean {
  return update.kind === "full-reload";
}

/**
 * Open the update channel and apply what arrives.
 *
 * Callers get an [`HmrClient`] back. Registering a refresh handler with
 * `accept` is what makes a module hot-swappable: without one, an update naming
 * that module escalates to a reload.
 */
export function connect(options?: ConnectOptions): HmrClient {
  const settings: ConnectOptions = options == null ? {} : options;
  const endpoint: string = typeof settings.endpoint === "string" ? settings.endpoint : HMR_ENDPOINT;
  const handlers: Map<string, RefreshHandler> = new Map();
  const global = getGlobal();
  const source = openStream(global, endpoint);
  let appliedCount = 0;

  const reload = (reason: string): void => {
    report(global, "warn", "full reload: " + reason);
    if (settings.onReload != null) {
      settings.onReload(reason);
      return;
    }
    reloadPage(global);
  };

  const apply = (update: HmrUpdate): Promise<UpdateKind> => {
    if (settings.onUpdate != null) {
      settings.onUpdate(update);
    }
    if (update.kind === "inert") {
      return Promise.resolve(update.kind);
    }
    if (update.kind === "full-reload") {
      reload(update.reason == null ? "the server asked for one" : update.reason);
      return Promise.resolve(update.kind);
    }
    if (update.modules.length === 0) {
      report(global, "info", describe(update));
      return Promise.resolve(update.kind);
    }
    return applyModules(update, handlers, global).then((ok: boolean) => {
      if (!ok) {
        reload("a module could not be re-evaluated");
        return "full-reload";
      }
      appliedCount += 1;
      report(global, "info", describe(update));
      return update.kind;
    });
  };

  if (source != null) {
    source.addEventListener(UPDATE_EVENT, (event: $FlowFixMe) => {
      const update = parseUpdate(String(event.data));
      if (update != null) {
        apply(update);
      }
    });
  }

  return {
    accept: (modulePath: string, handler: RefreshHandler): void => {
      handlers.set(modulePath, handler);
    },
    apply,
    close: (): void => {
      if (source != null) {
        source.close();
      }
      handlers.clear();
    },
    applied: (): number => appliedCount,
  };
}

/**
 * Re-import every module an update names, in the order it names them.
 *
 * The server lists dependencies before the boundaries that import them, so a
 * sequential walk evaluates a changed helper before the component that reads it
 * re-renders.
 */
function applyModules(
  update: HmrUpdate,
  handlers: Map<string, RefreshHandler>,
  global: $FlowFixMe,
): Promise<boolean> {
  let chain: Promise<boolean> = Promise.resolve(true);
  for (const module of update.modules) {
    chain = chain.then((ok: boolean) => {
      if (!ok) {
        return false;
      }
      return importModule(module.url).then((next: mixed) => {
        if (next == null) {
          return false;
        }
        if (module.role !== "boundary") {
          return true;
        }
        const handler = handlers.get(module.path);
        if (handler == null) {
          report(global, "warn", module.path + " has no refresh handler registered");
          return false;
        }
        handler(next);
        return true;
      });
    });
  }
  return chain;
}

/**
 * Dynamically import one module, resolving to `null` when the fetch is refused.
 *
 * The dev server answers a refused target with a status, not a module, so a
 * rejection here is the access-control layer doing its job and the correct
 * response is a reload rather than a retry.
 */
function importModule(url: string): Promise<mixed> {
  return import(url).then(
    (namespace: mixed) => namespace,
    () => null,
  );
}

/** One line describing what was applied. */
function describe(update: HmrUpdate): string {
  const milliseconds = Math.round(update.elapsedMicros / 100) / 10;
  return (
    update.path +
    " " +
    update.kind +
    " (" +
    String(update.modules.length) +
    " modules, " +
    String(update.routes.length) +
    " routes, " +
    String(milliseconds) +
    "ms)"
  );
}

/** The global object, or `null` where there is none. */
function getGlobal(): $FlowFixMe {
  return typeof globalThis === "undefined" ? null : globalThis;
}

/** Open the event stream, or return `null` outside a browser. */
function openStream(global: $FlowFixMe, endpoint: string): $FlowFixMe {
  if (global == null || typeof global.EventSource !== "function") {
    return null;
  }
  return new global.EventSource(endpoint);
}

/** Reload the page, where there is one. */
function reloadPage(global: $FlowFixMe): void {
  if (global != null && global.location != null) {
    global.location.reload();
  }
}

/** Write one line, prefixed, where a console exists. */
function report(global: $FlowFixMe, level: string, message: string): void {
  if (global == null || global.console == null) {
    return;
  }
  const sink = global.console[level];
  if (typeof sink === "function") {
    sink.call(global.console, LOG_PREFIX + " " + message);
  }
}

/**
 * The native runtime's own update entry point.
 *
 * Reserved for the in-process runtime, which drives updates without a socket.
 * Calling it outside that runtime raises through the shared helper, like every
 * other native binding in this package.
 */
export function nativeChannel(): HmrClient {
  return nativeRuntimeRequired(MODULE, "nativeChannel");
}
