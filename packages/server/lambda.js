// @flow
//
// `@uniflowed/server/lambda`: the AWS Lambda half of serving a build.
//
// A serverless target has no server. What it has is one function per
// invocation, whose signature belongs to the platform rather than to the Web —
// so this module is a translation, and the thing being translated is the same
// [`./fetch.js`] handler `uf start` and every other front door answer through.
//
// # One event shape, named
//
// **Payload format version 2.0**, which is what a Lambda Function URL always
// sends and what an API Gateway *HTTP* API sends by default. It is not what an
// API Gateway *REST* API sends (that is format 1.0, with `httpMethod`,
// `path` and `multiValueHeaders`), and it is not an ALB target-group event.
// Supporting all three by sniffing the event would be three untested code
// paths where the platform's own documentation says which one you get; this
// one is named in `docs/app/reference/cli/_uf.page.mdx`, and an event that is
// not it is refused by [`toRequest`] with a message saying so rather than
// answered from fields that happen to be undefined.
//
// # Buffered, and why that is a limitation rather than a choice
//
// `createFetchHandler` answers a document as a stream, so a `<Suspense>`
// fallback reaches the browser while the page behind it is still resolving.
// A Lambda response in this format is a JSON value, so the whole body is read
// before the invocation returns and none of that streaming survives. Response
// streaming exists — `awslambda.streamifyResponse`, on a Function URL whose
// invoke mode is `RESPONSE_STREAM` — and this module does not implement it:
// the wrapper is a global the managed runtime injects, so nothing outside a
// real invocation can drive it, and an untested streaming path is worse than a
// buffered one that says it is buffered. The 6 MB response payload limit is
// the platform's and applies to what this returns.
//
// # The static half is here rather than on a CDN, unless you put one there
//
// `staticDir` is served through `./node.js`'s `createStaticHandler`, from the
// deployment package itself, so the function answers a prerendered document
// and a hashed asset without any other infrastructure existing. That is the
// shape that works the moment it is uploaded; it is not the shape anybody
// should keep. Every byte of `static/` is then billed as invocation time and
// counted against the package limit, and a CloudFront distribution or an S3
// origin in front of the function is what the platform expects. Leaving
// `staticDir` out is how you say you have one — the application half then
// answers alone, exactly as it does on a worker.

import { Buffer } from "node:buffer";

import { createStaticHandler } from "./node.js";

import { Temporal } from "@uniflowed/core/temporal";
import type { CapabilityOptions, ServerCapabilities } from "./internal/capabilities.js";
import { assertCapable, capabilitiesFor } from "./internal/capabilities.js";
import type { RequestLifecycle } from "./internal/context.js";
import { elapsedMs, logRequest, processLogger } from "./log.js";

export type { RequestLifecycle } from "./internal/context.js";

/**
 * What a Lambda can do, which is neither of the two things this is asked.
 *
 * Both flags are `false`, and both are facts about the platform rather than
 * about this module. The response is a JSON value, so [`toResult`] reads the
 * whole body before the invocation returns — an event stream would be held in
 * memory until it closed, which for a stream that stays open is a timeout. And
 * the invocation is frozen the moment it answers, so work pushed into its own
 * memory is dropped rather than run.
 *
 * So an upgrader handed here is refused where the host is wired, before a
 * single connection is accepted and dropped, and so is a queue that does not
 * survive the process. That is the whole point of the pair being values: the
 * alternative is a deployment that accepts WebSocket handshakes all day and a
 * customer wondering why nothing arrives.
 *
 * A durable queue is not refused. Pushing to SQS from a Lambda is ordinary and
 * correct; what cannot be here is the *consumer*, which is a second function
 * or a container. `./queue.js` says which half is whose.
 */
export function lambdaCapabilities(options?: CapabilityOptions): ServerCapabilities {
  return assertCapable(
    capabilitiesFor("serverless", { stream: false, persistent: false }, options),
  );
}

/**
 * An HTTP API payload format 2.0 event, as much of it as this module reads.
 *
 * Inexact and almost entirely optional, because it arrives from the platform
 * rather than from a caller: the fields uf needs are checked in [`toRequest`],
 * where a missing one can be reported as the wrong event shape.
 */
export type LambdaHttpEvent = {
  readonly version?: string,
  readonly rawPath?: string,
  readonly rawQueryString?: string,
  readonly cookies?: $ReadOnlyArray<string>,
  readonly headers?: { readonly [string]: string | void },
  readonly body?: string,
  readonly isBase64Encoded?: boolean,
  readonly requestContext?: {
    readonly domainName?: string,
    readonly http?: {
      readonly method?: string,
      readonly path?: string,
      ...
    },
    ...
  },
  ...
};

/** What Lambda expects back for payload format 2.0. */
export type LambdaHttpResult = {|
  readonly statusCode: number,
  readonly headers: { [string]: string },
  readonly cookies: $ReadOnlyArray<string>,
  readonly body: string,
  readonly isBase64Encoded: boolean,
|};

/** Everything the serverless half needs to answer an invocation. */
export type LambdaHandlerOptions = {|
  /** The application, from the generated `handler.js`. */
  readonly handle: (request: Request) => Promise<Response>,
  /** That same module's `beginRequest`; see [`./node.js`]'s header for why. */
  readonly beginRequest: (request: Request) => RequestLifecycle,
  /**
   * The directory holding the build's own files, if the package carries them.
   *
   * Omitted where a CDN answers for them; see the header.
   */
  readonly staticDir?: string,
|};

/**
 * The `getSetCookie` half of `Headers`.
 *
 * Declared rather than called straight off the value because iterating
 * `Headers` joins repeated fields with a comma, and `Set-Cookie` is the one
 * field where that is a corruption rather than a spelling: two cookies become
 * one header nothing can parse back apart. `getSetCookie` is the standard
 * answer and every runtime that can run this has it — but a runtime that does
 * not would throw here rather than at the point the cookies were set, so the
 * absence is checked.
 */
type SetCookieReader = {
  readonly getSetCookie?: () => $ReadOnlyArray<string>,
  ...
};

/**
 * Media types whose bodies are text, and therefore not base64.
 *
 * The same shape of decision `./node.js` makes with `CONTENT_TYPES`, and the
 * same default: what is not known to be text is returned base64-encoded, which
 * is lossless for every byte sequence. Getting it the other way round would
 * corrupt an image silently.
 */
const TEXT_TYPES: $ReadOnlyArray<string> = Object.freeze([
  "application/javascript",
  "application/json",
  "application/manifest+json",
  "application/xml",
  "image/svg+xml",
]);

/** Whether a response with this `content-type` can be returned as a string. */
function isText(contentType: string | null): boolean {
  if (contentType == null) return false;
  const media = contentType.split(";")[0].trim().toLowerCase();
  if (media.startsWith("text/")) return true;
  if (media.endsWith("+json") || media.endsWith("+xml")) return true;
  return TEXT_TYPES.includes(media);
}

/**
 * The event as a `Request`.
 *
 * The URL is rebuilt rather than taken from a field, because no field holds
 * one: the path is `rawPath`, the query is `rawQueryString`, and the authority
 * is the `host` header the client sent — falling back to the API's own domain,
 * which is what a health check with no `Host` arrives with. `https`, always:
 * both a Function URL and an HTTP API terminate TLS, and there is no spelling
 * of either that a browser reaches over `http`.
 *
 * Cookies come from `event.cookies` and not from a header, because that is
 * where format 2.0 puts them — an application reading `cookies()` would
 * otherwise see none of them, which is the kind of difference between a
 * deployment and `uf start` this whole seam exists to prevent.
 */
export function toRequest(event: LambdaHttpEvent): Request {
  const method = event.requestContext?.http?.method;
  const rawPath = event.rawPath;
  if (typeof method !== "string" || typeof rawPath !== "string") {
    throw new Error(
      "uf: this handler reads AWS Lambda payload format 2.0 — a Lambda Function URL, " +
        "or an API Gateway HTTP API — and the event it was given has no " +
        "`requestContext.http.method` and `rawPath`. An API Gateway REST API sends " +
        "format 1.0 and an ALB sends its own shape; neither is supported.",
    );
  }

  const headers = new Headers();
  const source = event.headers ?? {};
  for (const name of Object.keys(source)) {
    const value = source[name];
    if (typeof value === "string") headers.set(name, value);
  }
  const cookies = event.cookies ?? [];
  if (cookies.length > 0) headers.set("cookie", cookies.join("; "));

  const authority =
    headers.get("host") ?? event.requestContext?.domainName ?? "lambda.amazonaws.com";
  const query = event.rawQueryString ?? "";
  const url = new URL(`https://${authority}${rawPath}${query === "" ? "" : `?${query}`}`);

  const init: { [string]: mixed } = { method: method.toUpperCase(), headers };
  // `Request` refuses a body on a `GET` or a `HEAD`, and API Gateway is under
  // no obligation not to send one: a client can, and the invocation would then
  // fail with a `TypeError` from the constructor rather than answer.
  if (typeof event.body === "string" && !["GET", "HEAD"].includes(method.toUpperCase())) {
    init.body =
      event.isBase64Encoded === true ? Buffer.from(event.body, "base64") : Buffer.from(event.body);
  }
  return new Request(url, init);
}

/** A `Response` as the JSON value Lambda returns to the client. */
export async function toResult(response: Response): Promise<LambdaHttpResult> {
  const headers: { [string]: string } = {};
  for (const [name, value] of response.headers) {
    // Skipped here and carried in `cookies` below: see [`SetCookieReader`].
    if (name.toLowerCase() === "set-cookie") continue;
    headers[name] = value;
  }
  const reader: SetCookieReader = response.headers;
  const cookies = typeof reader.getSetCookie === "function" ? [...reader.getSetCookie()] : [];

  const body = Buffer.from(await response.arrayBuffer());
  const text = isText(response.headers.get("content-type"));
  return {
    statusCode: response.status,
    headers,
    cookies,
    body: text ? body.toString("utf8") : body.toString("base64"),
    isBase64Encoded: !text,
  };
}

/**
 * A built uf application as a Lambda handler.
 *
 * Static files first, then the application — the order `uf preview` cannot
 * deviate from and therefore the order every other front door matches. The
 * request is begun here and settled once the body has been read into the
 * result, which on this target is genuinely "the response has been produced":
 * an invocation that returned before its `after()` callbacks ran would have
 * them killed with the sandbox, so `settle` is awaited rather than deferred.
 */
export function createLambdaHandler(
  options: LambdaHandlerOptions,
): (event: LambdaHttpEvent) => Promise<LambdaHttpResult> {
  const { handle, beginRequest, staticDir } = options;
  const serveStatic = staticDir == null ? null : createStaticHandler({ root: staticDir });

  return async function lambdaHandler(event: LambdaHttpEvent): Promise<LambdaHttpResult> {
    const request = toRequest(event);
    const lifecycle = beginRequest(request);
    const started = Temporal.Now.instant();
    // Declared out here so the `finally` can say what this invocation answered.
    // A Lambda has no terminal, so the line it leaves in CloudWatch is the only
    // account of the request there will ever be.
    let status = 500;
    try {
      const result = await lifecycle.run(async () => {
        const asset = serveStatic == null ? null : await serveStatic(request);
        return await toResult(asset ?? (await handle(request)));
      });
      status = result.statusCode;
      return result;
    } catch (error) {
      // The same 500 `./node.js`'s `nodeListener` writes, and for the same
      // reasons: the body must not carry the stack, and the log — which on
      // Lambda is CloudWatch — is where the operator is already looking. A
      // rejected invocation would be a 502 from API Gateway instead, which is
      // a different answer from `uf start`'s for the same failure.
      //
      // `toRequest` above is deliberately outside this: an event in the wrong
      // format is a misconfigured function rather than a failed request, and
      // answering it 500 forever would hide that.
      processLogger().error("request failed", { error });
      return {
        statusCode: 500,
        headers: { "content-type": "text/plain; charset=utf-8" },
        cookies: [],
        body: "500 Internal Server Error\n",
        isBase64Encoded: false,
      };
    } finally {
      logRequest(processLogger(), {
        requestId: lifecycle.context.id,
        method: request.method.toUpperCase(),
        path: new URL(request.url).pathname,
        route: lifecycle.context.route,
        status,
        durationMs: elapsedMs(started),
      });
      await lifecycle.settle();
    }
  };
}
