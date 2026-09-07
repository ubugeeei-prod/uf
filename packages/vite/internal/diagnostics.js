// @noflow
//
// Plain JavaScript: executed by the host that runs Vite, before any transform.
//
// The channel a browser reports on.
//
// Every other uf diagnostic — a type error, a lint finding, a failing test, a
// page that rendered its error boundary — arrives in the terminal the
// developer already has open. A diagnostic the *browser* produces had nowhere
// to go: the page is the only process that knows about it, and it has no
// channel back. So it lived in a browser overlay, which has to be noticed, in
// a window that may not be in front, by somebody who does not know to look.
//
// This is that channel, and it is deliberately one channel rather than one per
// feature. Two things report on it today:
//
//   * `POST /__uf/diagnostic` — a diagnostic a browser-side runtime produced
//     and wants a person to read. `@uniflowed/hmr`'s `reportDiagnostic` is the
//     client half; the hydration-mismatch report in `@uniflowed/router` is the
//     first caller.
//   * `POST /__uf/vitals`     — the five numbers `@uniflowed/web/vitals`
//     measures, posted by `vitalsBeacon()`. In production a project points the
//     beacon at an endpoint of its own; in development there was nothing at
//     the default path, so the one place the numbers are most useful — while
//     you are looking at the page — was the one place they went nowhere.
//
// Both end up as the same `diagnostic` event on the driver's control channel
// (see `./events.js`), which is what gets them uf's own rendering: a severity,
// a location, and a code frame when the browser had a position to give.
//
// # The terminal, and not a browser overlay
//
// ubugeeei-prod/uf#557 asked for the vitals to be shown in the overlay. They
// are shown in the terminal instead, and the later issue that generalised this
// — #583 — is the argument: a diagnostic that exists only in a browser window
// has to be noticed by somebody who does not know to look, which is the defect
// rather than the delivery. Sending these *back* to an overlay would also be
// circular for the diagnostic half, which arrived from the page in the first
// place, and an overlay covers the page a performance number is about. What it
// costs is that a reader watching the browser rather than the terminal sees
// nothing until they look, which is where every other uf diagnostic already
// is.
//
// # Nothing leaves the machine
//
// This module opens no connection. It reads a request that the page on the
// other end of the dev server's own socket made, writes a line to the terminal
// that started the dev server, and answers `204`. There is no destination, no
// third party and nothing to configure, in development or otherwise.
//
// # Why the paths are written out here
//
// `VITALS_ENDPOINT` in `@uniflowed/web/vitals` and `DIAGNOSTIC_ENDPOINT` in
// `@uniflowed/hmr` are the same two strings, and they are the contract. They
// cannot be *imported* here: this module is loaded by Vite before any Flow
// transform exists, and both of those are Flow. So they are written out, and
// `tests/library/dev-channel.test.js` asserts that all four spellings agree —
// a duplicated constant with a test on it is honest, and one without is how
// the browser ends up posting to a path nothing serves.
//
// # Why `/__uf/`
//
// A directory under `app/` whose name begins with `_` is not a route, so no
// application can put anything at this prefix and nothing here can shadow a
// path a project wrote. That is what makes it safe as a default destination
// and available to the dev server.

/** Where `@uniflowed/hmr`'s `reportDiagnostic` posts. */
export const DIAGNOSTIC_ENDPOINT = "/__uf/diagnostic";

/** Where `@uniflowed/web/vitals`'s `vitalsBeacon()` posts by default. */
export const VITALS_ENDPOINT = "/__uf/vitals";

/**
 * The most a report may weigh.
 *
 * A diagnostic is a headline and a few lines of context; a vitals report is
 * five numbers. Neither is close to this, and the ceiling is here because the
 * body arrives from a page — "no unbounded anything" in `docs/security.md`
 * covers a dev server reading a request as much as it covers a production one,
 * and a page with a runaway loop in it must not be able to make `uf dev` grow
 * without bound.
 */
export const MAX_BODY_BYTES = 64 * 1024;

/** The most detail lines one diagnostic prints. */
const MAX_DETAIL_LINES = 40;

/** The most characters any single line of a diagnostic prints. */
const MAX_LINE_CHARS = 400;

/** The most metrics one vitals report is read for; there are five names. */
const MAX_VITALS = 16;

/** The most characters a metric's name or its rating may print as. */
const MAX_NAME_CHARS = 32;

/** The severities the channel accepts, and the words the terminal uses. */
const SEVERITIES = new Set(["error", "warn", "info"]);

/**
 * The connect middleware that answers the channel.
 *
 * Mounted **before** the application middleware, so a request under `/__uf/`
 * never reaches a project's `_uf.middleware.js` or its route table. A guard
 * that ran for a page's own telemetry would be a guard asked a question the
 * application never asks, and one that redirected it would turn a report into
 * a login page.
 *
 * `report` is injected rather than reached for so that this module can be
 * driven without a terminal, a socket or a driver; `internal/events.js`'s
 * `emit` is what the plugin passes.
 *
 * @param {(diagnostic: object) => void} report
 */
export function createChannelMiddleware(report) {
  return async function channel(request, response, next) {
    const pathname = (request.url ?? "/").split("?")[0];
    const isVitals = pathname === VITALS_ENDPOINT;
    if (!isVitals && pathname !== DIAGNOSTIC_ENDPOINT) {
      next();
      return;
    }

    // A `GET` on either path is somebody checking whether the dev server has
    // them, and `405` with `Allow` answers that exactly. `404` would have said
    // the opposite of the truth.
    if (request.method !== "POST") {
      response.statusCode = 405;
      response.setHeader("allow", "POST");
      response.end();
      return;
    }

    let body;
    try {
      body = await readBody(request, MAX_BODY_BYTES);
    } catch {
      // A socket that went away mid-body. There is nothing to report and
      // nobody left to answer.
      response.statusCode = 400;
      response.end();
      return;
    }
    if (body == null) {
      response.statusCode = 413;
      response.end();
      return;
    }

    let payload = null;
    try {
      payload = JSON.parse(body);
    } catch {
      payload = null;
    }
    const diagnostic = isVitals ? vitalsDiagnostic(payload) : browserDiagnostic(payload);
    if (diagnostic == null) {
      // The body was not the shape this path promises. Refused rather than
      // guessed at: a diagnostic assembled out of a malformed report is a line
      // in somebody's terminal that describes nothing.
      response.statusCode = 400;
      response.end();
      return;
    }

    // Everything above either answered or handed the request on, so nothing
    // below can leave one hanging — and an exception from `report` is the dev
    // server's own failure rather than the page's, so it goes to Vite's error
    // handler like any other. An `async` connect middleware whose rejection
    // nobody catches is an unhandled rejection, which on a modern Node ends
    // the process: `uf dev` would exit on a malformed telemetry post.
    try {
      report(diagnostic);
    } catch (error) {
      next(error);
      return;
    }
    // No body, and nothing about the machine in the answer. The page posted
    // this and is not owed a reading of it back.
    response.statusCode = 204;
    response.end();
  };
}

/**
 * One diagnostic a browser-side runtime produced, or `null`.
 *
 * Every field is checked and every string is bounded, because all of it is
 * page-authored: a hydration mismatch on a page whose difference is in
 * somebody's comment carries that comment into this terminal. Nothing here is
 * interpreted — the terminal renderer prints text — but a report with a
 * thousand lines in it would still scroll the reason for it off the screen.
 *
 * @param {unknown} payload
 */
export function browserDiagnostic(payload) {
  if (payload == null || typeof payload !== "object" || Array.isArray(payload)) return null;
  const message = line(payload.message);
  if (message === "") return null;

  const severity = SEVERITIES.has(payload.severity) ? payload.severity : "error";
  const diagnostic = { severity, message };
  const origin = line(payload.url);
  if (origin !== "") diagnostic.origin = origin;
  const detail = lines(payload.detail);
  if (detail.length > 0) diagnostic.detail = detail;
  // A position, when the browser had one. It is what turns the status line
  // into a code frame on the other side, and a browser that only knows "this
  // component" rather than "this line" is expected: the frame is the better
  // rendering when it is available and never a requirement.
  const file = line(payload.file);
  if (file !== "" && Number.isInteger(payload.line) && payload.line > 0) {
    diagnostic.file = file;
    diagnostic.line = payload.line;
    if (Number.isInteger(payload.column) && payload.column >= 0) {
      diagnostic.column = payload.column;
    }
  }
  return diagnostic;
}

/**
 * A `VitalsReport` as one diagnostic, or `null` when it carries no metric.
 *
 * One diagnostic per report rather than one per metric, because the beacon
 * already coalesces across a microtask and a page load would otherwise be five
 * separate lines interleaved with whatever else the terminal is saying. The
 * severity is the worst rating in the report, which is the rule the issue asks
 * for: a rating that is not `good` is the interesting one and has to read as
 * one.
 *
 * @param {unknown} payload
 */
export function vitalsDiagnostic(payload) {
  if (payload == null || typeof payload !== "object" || Array.isArray(payload)) return null;
  if (!Array.isArray(payload.vitals)) return null;

  const measured = [];
  for (const vital of payload.vitals.slice(0, MAX_VITALS)) {
    if (vital == null || typeof vital !== "object") continue;
    if (typeof vital.name !== "string" || typeof vital.value !== "number") continue;
    if (!Number.isFinite(vital.value)) continue;
    // The name and the rating are the page's strings, not this module's, even
    // though a beacon written by `@uniflowed/web/vitals` only ever sends the
    // five names and the three ratings. Anything can post here, so they go
    // through the same bounding and control-character scrub as a diagnostic's
    // own text, and a *word* has no business being longer than a word.
    const name = line(vital.name).slice(0, MAX_NAME_CHARS);
    if (name === "") continue;
    const rating = line(vital.rating).slice(0, MAX_NAME_CHARS) || "unknown";
    measured.push({ name, value: vital.value, rating });
  }
  if (measured.length === 0) return null;

  // Worst first, so the line the reader needs is the line under the headline
  // rather than wherever the browser happened to finish measuring.
  measured.sort((left, right) => severityOf(right.rating) - severityOf(left.rating));
  const worst = measured[0];
  const severity = ratingSeverity(worst.rating);
  const message =
    severity === "info"
      ? `web vitals: ${measured.map((vital) => vital.name).join(", ")} good`
      : `web vitals: ${worst.name} is ${worst.rating} (${formatValue(worst)})`;

  const diagnostic = {
    severity,
    message,
    detail: measured.map((vital) => `${vital.name} ${formatValue(vital)} — ${vital.rating}`),
  };
  const origin = line(payload.url);
  if (origin !== "") diagnostic.origin = origin;
  return diagnostic;
}

/** How bad a rating is, for ordering; an unknown word sorts as the worst. */
function severityOf(rating) {
  if (rating === "good") return 0;
  if (rating === "needs-improvement") return 1;
  return 2;
}

/** The channel severity a rating maps to. */
function ratingSeverity(rating) {
  if (rating === "good") return "info";
  if (rating === "needs-improvement") return "warn";
  return "error";
}

/**
 * A metric's value with its unit.
 *
 * CLS is a unitless layout-shift score and everything else is milliseconds,
 * which is the one thing a reader has to know to act on the number — a `0.24`
 * printed as `0.24 ms` reads as the best result in the report rather than a
 * failing one.
 */
function formatValue(vital) {
  if (vital.name === "CLS") return String(Math.round(vital.value * 1000) / 1000);
  return `${Math.round(vital.value)} ms`;
}

/** One bounded single-line string, or `""` for anything that is not one. */
function line(value) {
  if (typeof value !== "string") return "";
  return printable(value).trim();
}

/** A bounded list of bounded lines, from a string or an array of them. */
function lines(value) {
  const source = typeof value === "string" ? value.split("\n") : value;
  if (!Array.isArray(source)) return [];
  const kept = [];
  for (const entry of source) {
    if (kept.length === MAX_DETAIL_LINES) {
      kept.push("…");
      break;
    }
    if (typeof entry !== "string") continue;
    kept.push(printable(entry));
  }
  return kept;
}

/**
 * One line of page-authored text, safe to write to a terminal and bounded.
 *
 * Every control character becomes a space, `ESC` included, and that is the
 * point rather than tidiness: what is being rendered was written by a page, and
 * a page that could put `ESC [` into a diagnostic could move the cursor,
 * recolour the rest of the session or overwrite the line above its own report.
 * `uf` owns this terminal — see `internal/events.js` — and nothing that arrives
 * over a socket gets to draw on it.
 *
 * A scan rather than a regular expression, per `docs/security.md`'s "no regex
 * on untrusted input": the rule is about backtracking and a character class
 * cannot backtrack, but a loop needs no argument at all and is no longer.
 */
function printable(value) {
  let text = "";
  for (const character of value.slice(0, MAX_LINE_CHARS)) {
    const code = character.codePointAt(0);
    text += code < 0x20 || code === 0x7f ? " " : character;
  }
  return text;
}

/**
 * The whole request body, or `null` when it is over `limit`.
 *
 * Counted as it arrives rather than trusting `content-length`: the header is
 * the sender's claim and the bytes are the fact, and `sendBeacon` sends
 * neither a length this side should rely on nor a content type worth reading —
 * a string payload goes out as `text/plain`, so the type says nothing about
 * whether the body is the JSON both endpoints document.
 */
async function readBody(request, limit) {
  let size = 0;
  const chunks = [];
  for await (const chunk of request) {
    size += chunk.length;
    if (size > limit) return null;
    chunks.push(chunk);
  }
  return Buffer.concat(chunks).toString("utf8");
}
