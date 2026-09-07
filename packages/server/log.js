// @flow
//
// `@uniflowed/server/log`: the logger a host configures.
//
// Two audiences again, the same split the root and `./host.js` already make.
// An application says things through `logger()` from `@uniflowed/server`, which
// answers about the request it is inside and knows its id and its route. This
// module is the other half: how the process decides where those lines go, at
// what level, and in what spelling. A page importing it would be a page
// deciding the deployment's logging policy from inside a render.
//
// # There is a process logger, and it is not the request's
//
// [`processLogger`] is a module-level value, and that is not the mistake
// `./internal/context.js` spends its header warning about. What must be scoped
// to a request is anything *about* a request: two requests in flight have
// different headers, different cookies and different ids, so a module-level
// variable holding any of those is a bug that only appears under load. Where
// the bytes go is not about a request. It is one decision for the process,
// made once at startup, shared by every request on purpose — and the
// request-scoped logger is that logger with the request's fields bound onto it.
//
// # Replacing it
//
// [`installLogger`] is the seam. `docs/red-lines.md` line 3 asks that every
// built-in provider be replaceable, and a logger is the clearest case there
// is: a deployment already has somewhere it puts logs, and a framework that
// could only write its own shape to its own stream would be a framework the
// operator has to work around. Hand it any [`Logger`] — one built by
// [`createLogger`] with a different sink, or an adapter over whatever the
// platform provides — and every line uf writes goes there instead, including
// the ones written from inside a render.
//
// It is deliberately a whole-process call with no way to install a logger for
// one request. A per-request installer would be a second thing scoped to a
// request that is not on the context, and the way to give one request its own
// fields is `child`, which is what the request-scoped logger already is.

import type { LogLevel, LogRecord, Logger } from "./internal/log.js";
import { createLogger, silentLogger } from "./internal/log.js";

export type {
  LogFields,
  LogFormat,
  LogLevel,
  LogRecord,
  LogSink,
  Logger,
  LoggerOptions,
} from "./internal/log.js";

export {
  consoleSink,
  createLogger,
  elapsedMs,
  formatJson,
  formatText,
  isRedacted,
  silentLogger,
} from "./internal/log.js";

/**
 * The logger this process writes through, until a host installs another.
 *
 * `null` until something asks, so the environment that decides the level and
 * the format is read when the process is running rather than when this module
 * was first imported — a bundler can hoist an import a long way from where it
 * is used, and a default that had already been decided by then would ignore a
 * host that set `UF_LOG_LEVEL` in its own startup.
 */
let installed: Logger | null = null;

/**
 * The process logger, building the default one if nobody installed any.
 *
 * Every uf module that logs goes through this rather than holding a logger of
 * its own, so [`installLogger`] reaches all of them — including modules that
 * were imported before it was called.
 */
export function processLogger(): Logger {
  return (installed ??= createLogger());
}

/**
 * Send everything uf logs to `logger` instead.
 *
 * Called by a host at startup, before it takes a socket. Calling it later is
 * allowed and does what it says — the lines after it go to the new logger —
 * but the lines before it have already gone somewhere, which is worth knowing
 * rather than discovering.
 *
 * Passing `null` puts the default back, which is what a test needs at the end
 * of a case that installed one: a logger left installed by one test is a
 * logger the next test writes through, and a shared sink between two tests is
 * the sort of coupling that fails only when the order changes.
 */
export function installLogger(logger: Logger | null): void {
  installed = logger;
}

/**
 * A logger that records what it was told instead of writing it.
 *
 * Here rather than in the test suite because two suites and any application
 * asserting on its own logging need the same thing, and because a hand-rolled
 * one tends to capture the arguments rather than the record — which is the
 * half where redaction and truncation happen, and so the half worth asserting
 * on.
 */
export function recordingLogger(options?: {| readonly level?: LogLevel |}): {|
  readonly logger: Logger,
  readonly records: Array<LogRecord>,
|} {
  const records: Array<LogRecord> = [];
  const logger = createLogger({
    level: options?.level ?? "debug",
    sink: (record) => {
      records.push(record);
    },
  });
  return { logger, records };
}

/**
 * A logger that writes nothing, as the process logger.
 *
 * The shape a host reaches for when it has its own front-of-house output and
 * does not want uf's underneath it — `uf dev` owns its terminal and renders it
 * from an event channel rather than from lines. Spelled as one call so a caller
 * does not have to know that silence is `silentLogger()` rather than a level.
 */
export function silenceLogging(): void {
  installed = silentLogger();
}

/** Everything an access line carries, so the hosts cannot disagree about it. */
export type RequestLogFields = {|
  readonly requestId: string,
  readonly method: string,
  /** The path, and never the query string; see [`logRequest`]. */
  readonly path: string,
  /** The route pattern that matched, or `null` when nothing did. */
  readonly route: string | null,
  readonly status: number,
  readonly durationMs: number,
|};

/**
 * Write the one line a finished request leaves behind.
 *
 * Here rather than in each host because there is more than one of them —
 * `./node.js` for `uf start`, `uf preview` and the `server.js` an adapter
 * writes, `./edge.js` for a worker, `./lambda.js` for a serverless invocation —
 * and three copies of a log shape is three field names that agree until
 * somebody fixes one of them. The message is the constant `request`; a caller
 * filtering a log asks for `msg:request status:>=500`, which is only a question
 * because none of this is interpolated into a sentence.
 *
 * `./standalone.js` is the one front door that does not write this line yet.
 * Its handler answers an embedded asset and a prerendered document before it
 * begins a request at all, so there is no single place with both the status and
 * the context, and restructuring the streaming path to make one is a change
 * that deserves its own review rather than a paragraph in this one.
 *
 * # The level comes from the status
 *
 * A 500 is the operator's problem, a 404 is usually the client's, and a 200 is
 * neither. Deciding that here rather than at each call site is what makes
 * `UF_LOG_LEVEL=warn` a useful setting: it leaves exactly the requests that
 * went wrong.
 *
 * # The query string is not in it
 *
 * `path` is `url.pathname`, and the search is dropped rather than trimmed for
 * length. A query string is where a redirect target, a search term, a signed
 * download URL and — in this very package — an OAuth `code` and `state` all
 * live. `./oauth.js` keeps a code away from a logger at its own end as well,
 * but a host writing this line has no idea which route it is logging, so the
 * only rule it can apply is the one that is right for every route.
 *
 * # And the route is
 *
 * A log of paths says `/orders/8813` was slow. A log of routes says
 * `/orders/:id` is slow, which is the question an operator actually has — one
 * fact instead of a million. The route is what matched in `@uniflowed/router`,
 * put on the request context by whichever of the dispatcher and the renderer
 * claimed it, and `null` for a request that matched neither: a static asset, a
 * 404, a request a guard refused above any route.
 */
export function logRequest(logger: Logger, fields: RequestLogFields): void {
  const at = fields.status >= 500 ? "error" : fields.status >= 400 ? "warn" : "info";
  logger[at]("request", {
    requestId: fields.requestId,
    method: fields.method,
    path: fields.path,
    route: fields.route,
    status: fields.status,
    durationMs: fields.durationMs,
  });
}
