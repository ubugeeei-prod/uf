// @noflow
// A local, bounded snapshot of the same diagnostic channel the terminal reads.
import fs from "node:fs";
import path from "node:path";
import { randomUUID } from "node:crypto";
import { AsyncLocalStorage } from "node:async_hooks";
import { format } from "node:util";

const requests = new AsyncLocalStorage();
let receive = null;

export function recordDevEvent(event) {
  const request = requests.getStore();
  receive?.({ ...event, ...(request == null ? {} : request), time: Date.now() });
}

export function startDevState(root, metadata) {
  const directory = path.join(root, ".uf");
  if (fs.existsSync(directory) && fs.lstatSync(directory).isSymbolicLink())
    throw new Error("uf dev: .uf must not be a symlink for diagnostics");
  fs.mkdirSync(directory, { recursive: true, mode: 0o700 });
  const file = path.join(directory, "dev-state.json");
  if (fs.existsSync(file) && fs.lstatSync(file).isSymbolicLink())
    throw new Error("uf dev: diagnostic state must not be a symlink");
  const session = randomUUID();
  const temporary = path.join(directory, "dev-state-" + session + ".tmp");
  const state = {
    schema: 1,
    session,
    pid: process.pid,
    startedAt: Date.now(),
    updatedAt: 0,
    generation: 0,
    errors: [],
    logs: [],
    routes: [],
    actions: [],
  };
  let closed = false;
  const persist = (refresh = false) => {
    if (closed) return;
    if (refresh || state.updatedAt === 0) {
      let current;
      try {
        current = metadata();
        state.metadataError = null;
      } catch (error) {
        state.metadataError = String(error.message ?? error).slice(0, 4000);
        current = { routes: state.routes, actions: state.actions };
      }
      state.routes = current.routes.slice(0, 1000);
      state.actions = current.actions.slice(0, 1000);
      state.truncated = current.routes.length > 1000 || current.actions.length > 1000;
    }
    state.updatedAt = Date.now();
    let serialized = JSON.stringify(state);
    while (Buffer.byteLength(serialized) > 3 * 1024 * 1024) {
      const rows = state.logs.length
        ? state.logs
        : state.errors.length
          ? state.errors
          : state.routes.length
            ? state.routes
            : state.actions;
      if (rows.length === 0) break;
      rows.shift();
      state.truncated = true;
      serialized = JSON.stringify(state);
    }
    fs.writeFileSync(temporary, serialized, { mode: 0o600 });
    fs.renameSync(temporary, file);
  };
  const observer = (event) => {
    const safe = bound(event);
    if (event.event === "source-changed") {
      state.generation += 1;
      state.errors = [];
    }
    if (
      event.event === "error" ||
      (event.event === "log" && event.level === "error") ||
      (event.event === "diagnostic" && event.severity === "error")
    ) {
      safe.kind ??= /hydrat/i.test(event.message ?? "")
        ? "hydration"
        : event.event === "diagnostic"
          ? "runtime"
          : "build";
      state.errors.push(safe);
      state.errors = state.errors.slice(-100);
    }
    if (["log", "diagnostic", "error", "request"].includes(event.event)) {
      state.logs.push(safe);
      state.logs = state.logs.slice(-150);
    }
    persist(event.event === "source-changed");
  };
  receive = observer;
  persist();
  // The terminal sees application console output too; keep its original behavior.
  const originals = new Map();
  for (const level of ["log", "info", "warn", "error", "debug"]) {
    const original = console[level];
    const wrapper = (...args) => {
      recordDevEvent({ event: "log", level, kind: "runtime", message: format(...args) });
      original.apply(console, args);
    };
    originals.set(level, { original, wrapper });
    console[level] = wrapper;
  }
  const timer = setInterval(() => {
    try {
      persist(true);
    } catch {
      close();
    }
  }, 5000);
  timer.unref?.();
  const close = () => {
    if (closed) return;
    closed = true;
    clearInterval(timer);
    if (receive === observer) receive = null;
    for (const [level, { original, wrapper }] of originals) {
      if (console[level] === wrapper) console[level] = original;
    }
    try {
      if (JSON.parse(fs.readFileSync(file, "utf8")).session === session) fs.unlinkSync(file);
    } catch {
      /* A missing or unreadable channel is reported by the MCP reader. */
    }
    process.off("exit", close);
  };
  process.once("exit", close);
  return {
    close,
    middleware(request, response, next) {
      const requestId = randomUUID();
      const url = String(request.url ?? "/")
        .split("?")[0]
        .slice(0, 1000);
      response.setHeader("x-uf-request-id", requestId);
      requests.run({ requestId, url }, () => {
        observer({ event: "request", requestId, url, method: request.method, time: Date.now() });
        next();
      });
    },
  };
}

function bound(value, depth = 0) {
  if (typeof value === "string") return value.slice(0, 4000);
  if (value == null || typeof value !== "object") return value;
  if (depth > 4) return "[truncated]";
  if (Array.isArray(value)) return value.slice(0, 40).map((v) => bound(v, depth + 1));
  return Object.fromEntries(
    Object.entries(value)
      .slice(0, 32)
      .map(([key, v]) => [key, bound(v, depth + 1)]),
  );
}
