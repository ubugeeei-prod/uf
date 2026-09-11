// @flow

import { ensuring, promise, runPromiseExit, sync, tryPromise } from "@uniflowed/effect";
import { cookies } from "@uniflowed/server";
import {
  authenticate,
  issueSession,
  revokeSession,
  SESSION_COOKIE,
} from "../../server/session.server.js";
import { inputEffect } from "../../server/input-effect.server.js";

function json(data: mixed, init?: ResponseOptions): Response {
  return new Response(JSON.stringify(data), {
    ...init,
    headers: { ...init?.headers, "Content-Type": "application/json", "Cache-Control": "no-store" },
  });
}

type ResponseOptions = { status?: number, headers?: { [string]: string } };

/**
 * Decode at most 4096 streamed bytes, cancelling an oversized body immediately.
 * Effect releases the reader lock on success, decoding failure, and cancellation.
 * The bound applies even when the sender omits or lies about Content-Length.
 */
async function readForm(request: Request): Promise<string | null> {
  // Flow's Request declaration does not yet expose the standard body stream.
  const stream: ReadableStream | null = (request as $FlowFixMe).body;
  if (stream == null) {
    return "";
  }
  const reader = stream.getReader();
  const decoder = new TextDecoder("utf-8", { fatal: true });
  let length = 0;
  let body = "";
  const result = await runPromiseExit(
    ensuring(
      promise(async () => {
        for (;;) {
          const step = await reader.read();
          if (step.done === true) break;
          const chunk: Uint8Array = step.value as $FlowFixMe;
          length += chunk.byteLength;
          if (length > 4096) {
            await reader.cancel("Form exceeds 4096 bytes");
            return null;
          }
          body += decoder.decode(chunk, { stream: true });
        }
        return body + decoder.decode();
      }),
      () => sync(() => reader.releaseLock()),
    ),
  );

  return match (result) {
    {kind: "success", value: const value} => value,
    {kind: "failure", ...} => null,
  };
}

/**
 * Accept same-origin form authentication and write cookies before headers leave.
 * Input failures become safe 400 responses; defects are logged only on the server.
 * Cookie rotation and revocation remain HTTP concerns, outside React Actions.
 */
export async function POST(request: Request): Promise<Response> {
  const url = new URL(request.url);
  const expectedOrigin = process.env.COMMONPLACE_ORIGIN ?? url.origin;
  if (
    request.headers.get("origin") !== expectedOrigin ||
    request.headers.get("sec-fetch-site") === "cross-site"
  )
    return json({ message: "This request is not allowed." }, { status: 403 });
  if (
    request.headers.get("content-type")?.split(";")[0].trim().toLowerCase() !==
    "application/x-www-form-urlencoded"
  )
    return json({ message: "Submit a form." }, { status: 415 });
  const body = await readForm(request);
  if (body == null) {
    return json({ message: "The form is too large." }, { status: 413 });
  }
  const form = new FormData();
  for (const [key, value] of new URLSearchParams(body)) form.append(key, value);
  const mode = form.get("mode");
  const secure = new URL(expectedOrigin).protocol === "https:";
  const token = cookies().get(SESSION_COOKIE);
  const result = await runPromiseExit(
    inputEffect(
      tryPromise({
        try: async () => {
          match (mode) {
            "logout" => {
              return revokeSession(token, secure);
            }
            "login" | "signup" as const operation => {
              const user = await authenticate(form, operation);
              return issueSession(user, token, secure);
            }
            _ => {
              return null;
            }
          }
        },
        catch: (error) => error,
      }),
    ),
  );

  match (result) {
    {kind: "success", value: const cookie} => {
      if (cookie == null) {
        return json({ message: "Unknown operation." }, { status: 400 });
      }

      return json(
        { status: "success", message: "You are ready.", location: "/" },
        { headers: { "Set-Cookie": cookie } },
      );
    }
    {kind: "failure", cause: {kind: "fail", error: const error}} => {
      return json(
        { status: "error", message: error.message, fields: error.fields },
        { status: 400 },
      );
    }
    {kind: "failure", cause: const cause} => {
      console.error("Commonplace authentication failed", cause);

      return json(
        { status: "error", message: "We could not complete that request. Please try again." },
        { status: 500 },
      );
    }
  }
}
