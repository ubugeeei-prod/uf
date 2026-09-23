// @noflow
//
// HTTP for the deploy matrix: one request, read to the end, with the moment
// each piece of the body arrived.
//
// Every check in `./checks.mjs` asks its question through [`request`], and the
// arrival times are why it exists rather than a bare `fetch`: "this target
// streams" is a claim about *when* bytes arrive, and the only way to make it
// from outside is to timestamp them as they are read. Nothing else here is
// clever — redirects are never followed (a redirect's whole answer is its
// `Location`), and a request that hangs fails after a bound rather than holding
// the job until the runner gives up.

import { setTimeout as sleep } from "node:timers/promises";

/** How long one request may take, end to end, before it counts as hung. */
export const REQUEST_TIMEOUT_MS = 30_000;

/**
 * An answer, read whole.
 *
 * @typedef {object} Answer
 * @property {number} status
 * @property {Headers} headers
 * @property {string} body the whole body as UTF-8 text
 * @property {Array<{ at: number, text: string }>} chunks each read, with the
 *   milliseconds since the request was sent at which it arrived
 * @property {string[]} setCookies every `Set-Cookie`, kept separate
 */

/**
 * Ask `base` + `path` and read the whole answer, timing every chunk.
 *
 * `init` is `fetch`'s, with `redirect: "manual"` forced. Throws on a network
 * failure or a timeout; any HTTP status is an answer, not an error — the checks
 * decide what a status means.
 *
 * @param {string} base the origin the target answers on, e.g. `http://127.0.0.1:3000`
 * @param {string} path
 * @param {RequestInit} [init]
 * @returns {Promise<Answer>}
 */
export async function request(base, path, init = {}) {
  const started = performance.now();
  const response = await fetch(new URL(path, base), {
    ...init,
    redirect: "manual",
    signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
  });
  const chunks = [];
  const decoder = new TextDecoder();
  if (response.body != null) {
    for await (const bytes of response.body) {
      chunks.push({
        at: performance.now() - started,
        text: decoder.decode(bytes, { stream: true }),
      });
    }
  }
  const rest = decoder.decode();
  if (rest !== "") chunks.push({ at: performance.now() - started, text: rest });
  return {
    status: response.status,
    headers: response.headers,
    body: chunks.map((chunk) => chunk.text).join(""),
    chunks,
    setCookies: response.headers.getSetCookie(),
  };
}

/**
 * When `needle` had fully arrived, in milliseconds since the request was sent,
 * or `null` if it never did.
 *
 * Measured on the text received so far rather than per chunk, because a
 * string can straddle two reads.
 *
 * @param {Answer} answer
 * @param {string} needle
 */
export function arrivalOf(answer, needle) {
  let received = "";
  for (const chunk of answer.chunks) {
    received += chunk.text;
    if (received.includes(needle)) return chunk.at;
  }
  return null;
}

/**
 * Wait until `base` answers anything at all, or throw after `timeoutMs`.
 *
 * "Anything" — a `404` from a host that is up is ready. `alive` is asked
 * between attempts so a host process that died is reported as dead rather than
 * waited on for the whole timeout.
 *
 * @param {string} base
 * @param {{ timeoutMs?: number, alive?: () => boolean, path?: string, headers?: Record<string, string> }} [options]
 */
export async function waitUntilAnswering(base, options = {}) {
  const deadline = Date.now() + (options.timeoutMs ?? 90_000);
  let last = "no attempt";
  while (Date.now() < deadline) {
    if (options.alive != null && !options.alive()) {
      throw new Error(`the host process exited before ${base} answered (last: ${last})`);
    }
    try {
      const response = await fetch(new URL(options.path ?? "/", base), {
        headers: options.headers,
        redirect: "manual",
        signal: AbortSignal.timeout(5_000),
      });
      await response.arrayBuffer();
      return;
    } catch (error) {
      last = String(error?.cause?.code ?? error?.message ?? error);
    }
    await sleep(500);
  }
  throw new Error(`${base} did not answer within the timeout (last: ${last})`);
}
