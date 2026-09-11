// @flow
// Cookie writes happen in the HTTP layer, before response headers are sent.
import { cookies } from "@uniflowed/server";
import {
  authenticate,
  issueSession,
  revokeSession,
  SESSION_COOKIE,
} from "../../server/session.server.js";
import { InputError } from "../../server/validation.server.js";

function json(data: mixed, init?: ResponseOptions): Response {
  return new Response(JSON.stringify(data), {
    ...init,
    headers: { ...init?.headers, "Content-Type": "application/json", "Cache-Control": "no-store" },
  });
}
type ResponseOptions = { status?: number, headers?: { [string]: string } };

async function readForm(request: Request): Promise<string | null> {
  // Flow's Request declaration does not yet expose the standard body stream.
  const stream: ReadableStream | null = (request as $FlowFixMe).body;
  if (stream == null) return "";
  const reader = stream.getReader();
  const decoder = new TextDecoder("utf-8", { fatal: true });
  let length = 0;
  let body = "";
  try {
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
  } catch {
    return null;
  } finally {
    reader.releaseLock();
  }
}

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
  if (body == null) return json({ message: "The form is too large." }, { status: 413 });
  const form = new FormData();
  for (const [key, value] of new URLSearchParams(body)) form.append(key, value);
  const mode = form.get("mode");
  const secure = new URL(expectedOrigin).protocol === "https:";
  const token = cookies().get(SESSION_COOKIE);
  try {
    const cookie =
      mode === "logout"
        ? revokeSession(token, secure)
        : mode === "login" || mode === "signup"
          ? issueSession(await authenticate(form, mode), token, secure)
          : null;
    if (cookie == null) return json({ message: "Unknown operation." }, { status: 400 });
    return json(
      { status: "success", message: "You are ready.", location: "/" },
      { headers: { "Set-Cookie": cookie, "Cache-Control": "no-store" } },
    );
  } catch (error) {
    if (error instanceof InputError)
      return json(
        { status: "error", message: error.message, fields: error.fields },
        { status: 400, headers: { "Cache-Control": "no-store" } },
      );
    console.error("Commonplace authentication failed", error);
    return json(
      { status: "error", message: "We could not complete that request. Please try again." },
      { status: 500, headers: { "Cache-Control": "no-store" } },
    );
  }
}
