// @noflow
//
// Plain JavaScript: executed by the host that runs Vite, before any transform.
//
// The driver's control channel.
//
// `uf dev` and `uf build` in Rust own the terminal: banners, phase timings,
// code frames, the summary. The driver therefore never prints for a person. It
// writes one JSON object per line to stdout — `{"event": "...", ...}` — and the
// Rust side renders them. Anything a person would read on stderr is Vite's own
// logger, which is redirected here as well so nothing bypasses the channel.

import { createLogger } from "vite";

/**
 * Emit one event.
 *
 * Writes are synchronous so an event precedes a crash that follows it, and so
 * `done` is on the pipe before the process exits.
 */
export function emit(event, fields = {}) {
  process.stdout.write(`${JSON.stringify({ event, ...fields })}\n`);
}

/**
 * A Vite logger whose every message becomes a `log` event.
 *
 * `clearScreen` is a no-op: the Rust side decides what the terminal shows.
 */
export function eventLogger(level = "info") {
  const base = createLogger(level, { allowClearScreen: false });
  const forward = (kind) => (message, options) => {
    if (options?.timestamp === false && message.trim() === "") return;
    emit("log", { level: kind, message: stripAnsi(String(message)) });
  };
  return {
    ...base,
    info: forward("info"),
    warn: forward("warn"),
    warnOnce: forward("warn"),
    error: forward("error"),
    clearScreen() {},
    hasErrorLogged: base.hasErrorLogged,
    hasWarned: base.hasWarned,
  };
}

const ANSI = /\[[0-9;]*m/g;

export function stripAnsi(text) {
  return text.replace(ANSI, "");
}

/**
 * Describe an error for the channel: message, and a location when Babel or
 * Rolldown attached one.
 */
export function errorEvent(error) {
  const fields = { message: stripAnsi(error?.message ?? String(error)) };
  if (error?.loc && typeof error.loc === "object") {
    fields.file = error.loc.file ?? error.id ?? null;
    fields.line = error.loc.line ?? null;
    fields.column = error.loc.column ?? null;
  } else if (error?.id) {
    fields.file = error.id;
  }
  if (typeof error?.frame === "string") fields.frame = stripAnsi(error.frame);
  if (typeof error?.stack === "string") fields.stack = stripAnsi(error.stack);
  return fields;
}
