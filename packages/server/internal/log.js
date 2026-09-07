// @flow
//
// Internal to `@uniflowed/server`: the logger, and what it refuses to print.
//
// uf had nowhere to put a sentence about a request. ubugeeei-prod/uf#405 is the
// symptom that made that concrete — a request Node's own parser refuses is
// answered with 400 by the runtime and recorded nowhere, so an operator with a
// client that cannot form a request sees an empty terminal and a browser saying
// nothing useful — but the shape of the problem is older than that one bug.
// Every module that had something to say reached for `console.error` with a
// string, which is fine for the one place a human is looking and useless
// everywhere else: a line that has to be read by a person and a line that has
// to be read by a log aggregator are different lines, and the way to have both
// is to build a *record* and format it twice, not to write one of them and
// parse it back.
//
// # What a record is
//
// A level, a time, a message, and fields. The message is a constant the author
// wrote; everything that varies is a field. That split is the whole reason a
// structured logger is worth having — `request` with `status=500` is a thing
// you can count, and `"GET /a/b failed with 500"` is a thing you can only grep
// — and it is a rule this package follows itself: nothing below interpolates a
// value into a message.
//
// # Two things a log line must not be able to do
//
// **It must not carry a credential.** A token in a log line is a token in
// whatever holds the logs, read by more people, kept for longer, and replicated
// further than the store it came from. [`REDACTED_FIELDS`] is a closed table of
// names whose value is never printed. It is a table of *names* rather than a
// test of values on purpose: a heuristic that recognises "this looks like a
// JWT" is a regex over untrusted input, which `docs/security.md` rule 5
// forbids, and a heuristic that misses once has printed the token forever.
// Redaction is not the only guard — `../oauth.js` never hands a credential to
// a logger at all — it is the one that holds when somebody forgets.
//
// **It must not forge another line.** A log is a sequence of records separated
// by newlines, and much of what a request line carries is text a client chose:
// a path, a user agent, an error message built from a header. A value holding a
// newline could otherwise close its own record and open a fabricated one, which
// is how a log becomes evidence of something that did not happen. Every string
// that reaches a record loses its control characters and is cut to a fixed
// length, exactly as `uf_pm::progress` does for registry text before drawing
// it.
//
// # Bounds
//
// `docs/security.md` rule 4: no unbounded anything. A field value is walked to
// a fixed depth, with a fixed number of keys per object and entries per array,
// and strings are cut. The cost of the cut is a truncated diagnostic; the cost
// of no cut is a request that logs a megabyte because a handler passed it one.
//
// # Why `console.error`, for every level
//
// `console` because every runtime uf targets has one and `process.stderr` is
// not — this package is imported by a Cloudflare Worker through `../edge.js`.
// `console.error` for *all four* levels because stdout is not free: in the
// process that runs `uf start` and `uf preview`, stdout is `@uniflowed/vite`'s
// control channel, one JSON object per line, read by the Rust side that owns
// the terminal. `console.info` goes to stdout on Node, so a logger that chose
// its method by level would put a log line in the middle of a protocol —
// intermittently, only in the hosts that share a process with the driver, and
// only under traffic.
//
// The alternative was for the sink to know which process it is in, and a logger
// that has to know that is a logger that will be wrong once. One stream, and the
// level is a field rather than a choice of file descriptor. A deployment that
// wants its logs on stdout installs a sink that writes there, which is what
// `sink` is for and is the same "every built-in provider must be replaceable"
// that `docs/red-lines.md` asks for everywhere else.

/** How much a record has to matter before it is written. */
export type LogLevel = "debug" | "info" | "warn" | "error";

/** The varying half of a record: everything that is not the constant message. */
export type LogFields = { +[string]: mixed };

/** One thing worth saying, before anybody has decided how to spell it. */
export type LogRecord = {|
  readonly level: LogLevel,
  /** Milliseconds since the epoch, from `Date.now()`. */
  readonly time: number,
  /** A constant the author wrote. Never interpolated; see the module header. */
  readonly message: string,
  readonly fields: LogFields,
|};

/** Where a record goes once it has been decided it is worth writing. */
export type LogSink = (record: LogRecord) => void;

/** How a record is spelled for whoever reads it. */
export type LogFormat = "json" | "text";

/**
 * Somewhere to say something.
 *
 * `child` is the reason this is an object rather than four functions: a request
 * has an id and a route that belong on every line it produces, and the
 * alternative to binding them once is passing them at every call, which is the
 * same as not having them.
 */
export type Logger = {|
  readonly debug: (message: string, fields?: LogFields) => void,
  readonly info: (message: string, fields?: LogFields) => void,
  readonly warn: (message: string, fields?: LogFields) => void,
  readonly error: (message: string, fields?: LogFields) => void,
  /** A logger that adds `fields` to everything written through it. */
  readonly child: (fields: LogFields) => Logger,
  /** The threshold this logger writes at, for a caller that wants to skip work. */
  readonly level: LogLevel,
|};

/** What `createLogger` may be told. */
export type LoggerOptions = {|
  /** The lowest level written. Below it, nothing is formatted at all. */
  readonly level?: LogLevel,
  /** How a record is spelled. Ignored when `sink` is given, which spells it itself. */
  readonly format?: LogFormat,
  /** Where records go. The default writes to `console`; see the module header. */
  readonly sink?: LogSink,
|};

/**
 * The levels, in order.
 *
 * A table rather than the numbers themselves at each call site, so "is this
 * worth writing" is one comparison and an unknown level is a question with an
 * answer instead of a `NaN` that silently writes everything.
 */
const SEVERITY: { readonly [string]: number } = Object.freeze({
  debug: 10,
  info: 20,
  warn: 30,
  error: 40,
});

/**
 * Field names whose value is never printed.
 *
 * Compared after lowercasing and dropping `-` and `_`, so `Set-Cookie`,
 * `set_cookie` and `setCookie` are one entry rather than three chances to miss
 * one.
 *
 * `code` is deliberately absent and `authorizationcode` is present. A field
 * called `code` is an error code far more often than it is an authorization
 * code — `ECONNRESET`, `EAI_AGAIN`, an HTTP status — and a table that redacted
 * it would make the ordinary diagnostic useless in order to catch a value this
 * package never passes to a logger in the first place.
 */
const REDACTED_FIELDS: Set<string> = new Set([
  "accesstoken",
  "apikey",
  "assertion",
  "authorization",
  "authorizationcode",
  "clientsecret",
  "cookie",
  "credential",
  "credentials",
  "idtoken",
  "password",
  "privatekey",
  "proxyauthorization",
  "refreshtoken",
  "secret",
  "sessionid",
  "setcookie",
  "token",
  "codeverifier",
]);

/** What stands in for a value that was not printed. */
const REDACTION = "[redacted]";

/** The bounds of rule 4, in one place so a reader can see all of them at once. */
const MAX_MESSAGE = 2048;
const MAX_STRING = 1024;
const MAX_DEPTH = 4;
const MAX_KEYS = 64;
const MAX_ENTRIES = 32;

/**
 * A logger.
 *
 * The threshold is checked before anything is built, so a `debug` call on an
 * `info` logger costs an object lookup and a comparison rather than a walk over
 * whatever was passed to it. That matters because the cheapest way to end up
 * with no debug logging at all is for debug logging to be expensive.
 */
export function createLogger(options?: LoggerOptions): Logger {
  // Before anything is built, because `silent` is the one setting whose whole
  // point is that nothing happens — a threshold above `error` would still
  // format and hand a record to a sink that threw it away.
  if (options?.level == null && environment("UF_LOG_LEVEL") === "silent") {
    return silentLogger();
  }
  const level = options?.level ?? defaultLevel();
  const sink = options?.sink ?? consoleSink(options?.format ?? defaultFormat());
  return loggerAt(level, sink, Object.freeze({}));
}

/**
 * A logger at `level`, writing `bound` on every record.
 *
 * Separate from [`createLogger`] because `child` needs it and must not
 * re-resolve the defaults: a child of a logger a host configured has to be that
 * logger with more fields, not a second logger that happened to agree.
 */
function loggerAt(level: LogLevel, sink: LogSink, bound: LogFields): Logger {
  const threshold = SEVERITY[level] ?? SEVERITY.info;

  const write = (at: LogLevel, message: string, fields?: LogFields): void => {
    if ((SEVERITY[at] ?? 0) < threshold) {
      return;
    }
    sink({
      level: at,
      time: Date.now(),
      message: safeString(message, MAX_MESSAGE),
      fields: safeFields({ ...bound, ...fields }),
    });
  };

  return {
    level,
    debug: (message, fields) => write("debug", message, fields),
    info: (message, fields) => write("info", message, fields),
    warn: (message, fields) => write("warn", message, fields),
    error: (message, fields) => write("error", message, fields),
    child: (fields) => loggerAt(level, sink, { ...bound, ...fields }),
  };
}

/**
 * A logger that writes nothing.
 *
 * For a caller that must have a logger and has been told not to log — a test,
 * and a host whose operator set `UF_LOG_LEVEL=silent`. It is a real logger
 * rather than `null` so that no call site has to test for one; a `null` logger
 * is a `?.` on every line, and a `?.` that is forgotten once is a crash in the
 * path that was already going wrong.
 */
export function silentLogger(): Logger {
  const nothing = () => {};
  const logger: Logger = {
    level: "error",
    debug: nothing,
    info: nothing,
    warn: nothing,
    error: nothing,
    child: () => logger,
  };
  return logger;
}

/**
 * The sink that writes to `console.error`, spelled as `format` says.
 *
 * Every level, one stream. See the module header: the process that runs
 * `uf start` has a protocol on stdout, and `console.info` writes there.
 */
export function consoleSink(format: LogFormat): LogSink {
  const spell = format === "json" ? formatJson : formatText;
  return (record) => {
    console.error(spell(record));
  };
}

/**
 * One record as a line of JSON.
 *
 * `time` is an ISO string rather than the number it is held as, because that is
 * what every log aggregator sorts on without being told; `msg` rather than
 * `message` for the same reason. The fields are spread at the top level, so a
 * query is `status:500` rather than `fields.status:500` — and a field named
 * `level`, `time` or `msg` cannot displace the record's own, because the
 * record's are written after it.
 */
export function formatJson(record: LogRecord): string {
  return JSON.stringify({
    ...record.fields,
    level: record.level,
    time: new Date(record.time).toISOString(),
    msg: record.message,
  });
}

/**
 * One record as a line for a person.
 *
 * The time is the wall clock without the date: this format is for a terminal
 * that has been open for a few minutes, and the date in front of every line is
 * a column nobody reads. `key=value` for the fields rather than JSON, because
 * the thing a reader does with this line is scan it.
 */
export function formatText(record: LogRecord): string {
  const at = new Date(record.time).toISOString().slice(11, 23);
  const parts = [at, record.level.padEnd(5), record.message];
  for (const name of Object.keys(record.fields)) {
    parts.push(`${name}=${textValue(record.fields[name])}`);
  }
  return parts.join(" ");
}

/** One field value, as a person reads it. */
function textValue(value: mixed): string {
  if (typeof value === "string") {
    // Quoted only when it would otherwise run into the next `key=`, so the
    // common case — an identifier, a path, a number — stays unadorned.
    return value.includes(" ") ? JSON.stringify(value) : value;
  }
  if (value == null || typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  return JSON.stringify(value) ?? String(value);
}

/**
 * `fields`, with the credentials gone and everything else made safe to print.
 *
 * A new object rather than the caller's: a logger that mutated what it was
 * handed would be a logger that redacted a value out of the application's own
 * data structure.
 */
export function safeFields(fields: LogFields): LogFields {
  return safeObject(fields, 0);
}

function safeObject(value: { +[string]: mixed }, depth: number): { [string]: mixed } {
  const out: { [string]: mixed } = {};
  let kept = 0;
  for (const name of Object.keys(value)) {
    if (kept >= MAX_KEYS) {
      out["…"] = "[truncated]";
      break;
    }
    const entry = safeValue(name, value[name], depth);
    if (entry === DROPPED) {
      continue;
    }
    out[safeString(name, 128)] = entry;
    kept += 1;
  }
  return out;
}

/**
 * What a value that is not worth printing turns into.
 *
 * A sentinel rather than `undefined`, because `undefined` is also a value a
 * caller can pass and the two mean different things: one is "there was nothing
 * here", which is worth a key, and the other is "a function, which no format
 * can spell", which is not.
 */
const DROPPED: symbol = Symbol("dropped");

function safeValue(name: string, value: mixed, depth: number): mixed {
  if (isRedacted(name)) {
    // Whatever it was. A redacted name is redacted at every depth and in every
    // shape, so a token nested inside an object under a safe name is still
    // caught by the name it is stored under.
    return value == null ? value : REDACTION;
  }
  if (value == null || typeof value === "boolean") {
    return value;
  }
  if (typeof value === "number") {
    // `NaN` and the infinities are not JSON, and `JSON.stringify` writes them
    // as `null` — which reads as "there was no number" rather than "the number
    // was not finite".
    return Number.isFinite(value) ? value : String(value);
  }
  if (typeof value === "string") {
    return safeString(value, MAX_STRING);
  }
  if (typeof value === "bigint") {
    return value.toString();
  }
  if (typeof value === "function" || typeof value === "symbol") {
    return DROPPED;
  }
  if (value instanceof Error) {
    // The three things anybody wants from an exception, and no more: an error
    // carries whatever properties were hung on it, and those have not been
    // through this function.
    return {
      name: safeString(value.name, 128),
      message: safeString(value.message, MAX_STRING),
      stack: value.stack == null ? undefined : safeString(value.stack, MAX_STRING * 4),
    };
  }
  if (depth >= MAX_DEPTH) {
    return "[deep]";
  }
  if (Array.isArray(value)) {
    const out = value
      .slice(0, MAX_ENTRIES)
      .map((entry) => safeValue("", entry, depth + 1))
      .filter((entry) => entry !== DROPPED);
    return value.length > MAX_ENTRIES ? [...out, "[truncated]"] : out;
  }
  if (typeof value === "object") {
    return safeObject(value as $FlowFixMe, depth + 1);
  }
  return DROPPED;
}

/**
 * Whether a field of this name is a credential.
 *
 * Normalised before the lookup so one entry covers every spelling a caller
 * might reach for; see [`REDACTED_FIELDS`].
 */
export function isRedacted(name: string): boolean {
  let normalised = "";
  for (const character of name.toLowerCase()) {
    if (character !== "-" && character !== "_" && character !== ".") {
      normalised += character;
    }
  }
  return REDACTED_FIELDS.has(normalised);
}

/**
 * `value`, with nothing in it that could forge a record, cut to `limit`.
 *
 * Control characters become a space rather than being dropped, so a path
 * carrying one is still the same length and the same shape — a value that lost
 * its bytes silently would be a value that reads as though the client sent
 * something it did not.
 *
 * The cut is by code unit and could land inside a surrogate pair. That is
 * accepted: the alternative is a scan for a boundary on every string a request
 * logs, and a lone surrogate in a log line is a mojibake, not a vulnerability.
 */
export function safeString(value: string, limit: number): string {
  let out = "";
  for (let at = 0; at < value.length && at < limit; at += 1) {
    const code = value.charCodeAt(at);
    out += code < 0x20 || code === 0x7f ? " " : value[at];
  }
  return value.length > limit ? `${out}…` : out;
}

/**
 * The level a logger uses when nobody said.
 *
 * `UF_LOG_LEVEL` because a level is an operational decision, made where the
 * process is started rather than where it is written. A name that is not a
 * level falls back to `info` rather than to silence: a typo in a deployment's
 * environment must not be the reason a production incident left no trace.
 * `silent` is the one word that means nothing at all, and [`createLogger`]
 * answers it before it reaches here.
 *
 * Read here rather than at module scope: a shipped module may only declare,
 * import and export at its top level, and a default resolved at import time is
 * a default that ignores an environment set afterwards — which is exactly what
 * a test that installs its own logger does.
 */
export function defaultLevel(): LogLevel {
  const named = environment("UF_LOG_LEVEL");
  return named != null && Object.hasOwn(SEVERITY, named) ? (named as $FlowFixMe) : "info";
}

/**
 * The format a logger uses when nobody said.
 *
 * JSON where the output is going to a collector and a readable line where it is
 * going to a person, decided by the one signal every host in this repository
 * already sets: `uf dev` and `uf preview` run with `NODE_ENV` unset or
 * `development`, and `uf build`'s output runs with it `production`.
 * `UF_LOG_FORMAT` overrides, for the deployment that pipes its terminal into a
 * collector or the developer debugging a JSON pipeline locally.
 */
export function defaultFormat(): LogFormat {
  const named = environment("UF_LOG_FORMAT");
  if (named === "json" || named === "text") {
    return named;
  }
  return environment("NODE_ENV") === "production" ? "json" : "text";
}

/**
 * One environment variable, or `null` where there is no environment.
 *
 * A worker has no `process`, and a module that assumed one would throw on
 * import in the runtime `../edge.js` exists for.
 */
function environment(name: string): string | null {
  const runtime = globalThis.process;
  if (runtime == null || typeof runtime !== "object") {
    return null;
  }
  const values = runtime.env;
  if (values == null || typeof values !== "object") {
    return null;
  }
  const value = values[name];
  return typeof value === "string" && value !== "" ? value : null;
}
