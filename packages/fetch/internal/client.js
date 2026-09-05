// @flow
//
// Typed HTTP over the platform's `fetch`.
//
// Not a wrapper for its own sake. Three things every application writes by
// hand around `fetch`, each of which is easy to get subtly wrong:
//
//   * **A failed response is not a failed promise.** `fetch` resolves for a
//     500. Code that forgets to check `response.ok` treats an error page as
//     data, and the failure surfaces later as an unrelated type error.
//   * **A timeout.** `fetch` has none. A request to a host that accepts the
//     connection and never answers hangs until the process ends.
//   * **Retrying the right things.** A 500 is worth retrying and a 400 is not,
//     and retrying a POST that may have succeeded is how you charge a card
//     twice.
//
// A schema is optional and, when given, is applied to the parsed body — so a
// response that does not match is a failure at the boundary rather than a
// `TypeError` three call frames later.

/** What went wrong, as a value rather than a string. */
export type FetchFailure =
  | {|
      readonly kind: "http",
      readonly status: number,
      readonly statusText: string,
      readonly response: Response,
    |}
  | {| readonly kind: "network", readonly cause: mixed |}
  | {| readonly kind: "timeout", readonly millis: number |}
  | {| readonly kind: "parse", readonly cause: mixed |}
  | {| readonly kind: "invalid", readonly issues: $ReadOnlyArray<mixed> |};

/**
 * A request that failed, carrying why.
 *
 * One error class with a typed `failure`, rather than a class per case: a
 * caller that wants to branch reads `failure.kind`, and one that does not gets
 * a message that already says what happened.
 */
export class FetchError extends Error {
  readonly failure: FetchFailure;
  readonly url: string;

  constructor(url: string, failure: FetchFailure) {
    super(describe(url, failure));
    this.name = "FetchError";
    this.failure = failure;
    this.url = url;
  }

  /** Whether another attempt could plausibly settle this. */
  get retriable(): boolean {
    return match (this.failure.kind) {
      "network" => true,
      "timeout" => true,
      // 408 is a timeout the server noticed, 429 is "slow down", and 5xx is
      // the server's problem rather than the request's. Nothing else is worth
      // sending again: a 400 will be a 400 next time too.
      "http" =>
        this.failure.status === 408 || this.failure.status === 429 || this.failure.status >= 500,
      _ => false,
    };
  }
}

function describe(url: string, failure: FetchFailure): string {
  return match (failure.kind) {
    "http" => `${url} answered ${failure.status} ${failure.statusText}`,
    "network" => `${url} could not be reached: ${String(failure.cause)}`,
    "timeout" => `${url} did not answer within ${failure.millis}ms`,
    "parse" => `${url} did not return the body it said it would: ${String(failure.cause)}`,
    "invalid" => `${url} returned ${failure.issues.length} value(s) the schema rejected`,
  };
}

/**
 * A function that checks what arrived and narrows it.
 *
 * A function rather than a schema object, so this module depends on no
 * validator: `@uniflowed/validator`'s `parser(User)` is exactly this shape,
 * and so is a hand-written check.
 */
export type Parse<T> = (
  value: mixed,
) =>
  | {| readonly ok: true, readonly value: T |}
  | {| readonly ok: false, readonly issues: $ReadOnlyArray<mixed> |};

/** How a client behaves for every request it makes. */
export type FetchConfig = {|
  readonly baseURL?: string,
  readonly headers?: { readonly [string]: string },
  /** Abort a request that has not answered. Defaults to 30 seconds. */
  readonly timeout?: number,
  /** How many further attempts a retriable failure gets. Defaults to none. */
  readonly retries?: number,
  /** Milliseconds before the first retry; doubles each time. Defaults to 200. */
  readonly retryDelay?: number,
  /** Swap in a different `fetch`, which is how a test avoids the network. */
  readonly fetch?: typeof fetch,
|};

/** One request. */
export type RequestOptions<T> = {|
  readonly method?: "GET" | "HEAD" | "POST" | "PUT" | "PATCH" | "DELETE",
  /** Sent as JSON unless it is already a `BodyInit`. */
  readonly body?: mixed,
  readonly headers?: { readonly [string]: string },
  readonly searchParams?: { readonly [string]: string | number | boolean },
  readonly signal?: AbortSignal,
  readonly timeout?: number,
  readonly retries?: number,
  /** Checked against the parsed body; its failure is the request's failure. */
  readonly parse?: Parse<T>,
|};

/** A configured client. */
export type FetchClient = {|
  readonly request: <T>(path: string, options?: RequestOptions<T>) => Promise<T>,
  readonly raw: (path: string, options?: RequestOptions<mixed>) => Promise<Response>,
  /** A client with more defaults applied on top of this one's. */
  readonly extend: (config: FetchConfig) => FetchClient,
|};

const DEFAULTS = { timeout: 30_000, retries: 0, retryDelay: 200 };

/** Methods that may be retried without asking whether they were applied. */
const IDEMPOTENT = new Set(["GET", "HEAD", "PUT", "DELETE", "OPTIONS"]);

/**
 * A client with these defaults.
 *
 * Creating a client rather than exporting a function per verb, because the
 * base URL, the headers and the retry policy belong to a *service* — and an
 * application talks to more than one.
 */
export function createFetch(config?: FetchConfig): FetchClient {
  const settings = { ...DEFAULTS, ...(config ?? {}) };

  const raw = async (path: string, options?: RequestOptions<mixed>): Promise<Response> =>
    send(settings, path, options ?? {});

  return {
    raw,
    request: async <T>(path: string, options?: RequestOptions<T>): Promise<T> => {
      const given = options ?? {};
      const response = await send(settings, path, given);
      return parse(response, given, resolveUrl(settings, path, given)) as $FlowFixMe;
    },
    extend: (extra: FetchConfig) =>
      createFetch({
        ...settings,
        ...extra,
        headers: { ...(settings.headers ?? {}), ...(extra.headers ?? {}) },
      }),
  };
}

/** Send, with the timeout and the retry policy applied. */
async function send(
  settings: $FlowFixMe,
  path: string,
  options: RequestOptions<mixed>,
): Promise<Response> {
  const url = resolveUrl(settings, path, options);
  const method = (options.method ?? "GET").toUpperCase();
  const attempts = 1 + Math.max(0, options.retries ?? settings.retries);
  const timeout = options.timeout ?? settings.timeout;
  const doFetch = settings.fetch ?? globalThis.fetch;

  let failure: FetchError | null = null;
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    if (attempt > 0) {
      await pause(settings.retryDelay * 2 ** (attempt - 1));
    }
    try {
      const response = await withTimeout(
        (signal) => doFetch(url, requestInit(settings, options, method, signal)),
        timeout,
        options.signal,
        url,
      );
      if (response.ok) {
        return response;
      }
      failure = new FetchError(url, {
        kind: "http",
        status: response.status,
        statusText: response.statusText,
        response,
      });
    } catch (error) {
      failure =
        error instanceof FetchError
          ? error
          : new FetchError(url, { kind: "network", cause: error });
    }

    // A method that may have been applied is not retried, however retriable
    // the failure looks: a POST that timed out may have succeeded, and sending
    // it again is how an order is placed twice.
    if (!failure.retriable || !IDEMPOTENT.has(method)) {
      throw failure;
    }
  }
  throw failure ?? new FetchError(url, { kind: "network", cause: "no attempt was made" });
}

function requestInit(
  settings: $FlowFixMe,
  options: RequestOptions<mixed>,
  method: string,
  signal: AbortSignal,
): RequestOptions<mixed> {
  const headers: { [string]: string } = {
    ...(settings.headers ?? {}),
    ...(options.headers ?? {}),
  };

  let body = options.body;
  if (body != null && !isBodyInit(body)) {
    // JSON unless the caller already made it something the platform accepts,
    // and the header set only if they did not choose one.
    body = JSON.stringify(body);
    if (headers["content-type"] == null && headers["Content-Type"] == null) {
      headers["content-type"] = "application/json";
    }
  }

  return { method, headers, body, signal } as $FlowFixMe;
}

/** Whether the platform can send this as-is. */
function isBodyInit(body: mixed): boolean {
  return (
    typeof body === "string" ||
    body instanceof URLSearchParams ||
    body instanceof FormData ||
    body instanceof Blob ||
    body instanceof ArrayBuffer ||
    (globalThis.ReadableStream != null && body instanceof globalThis.ReadableStream)
  );
}

/**
 * Race the request against the clock, and against the caller's own signal.
 *
 * A timeout has to abort the request rather than merely stop waiting for it:
 * leaving it in flight holds a connection and, worse, lets its result arrive
 * after the caller has moved on.
 */
async function withTimeout(
  run: (signal: AbortSignal) => Promise<Response>,
  millis: number,
  external: AbortSignal | void,
  url: string,
): Promise<Response> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), millis);
  const forward = () => controller.abort();
  external?.addEventListener("abort", forward);

  try {
    return await run(controller.signal);
  } catch (error) {
    if (controller.signal.aborted && external?.aborted !== true) {
      throw new FetchError(url, { kind: "timeout", millis });
    }
    throw error;
  } finally {
    clearTimeout(timer);
    external?.removeEventListener("abort", forward);
  }
}

/** The body, parsed by content type, then checked against the schema. */
async function parse<T>(
  response: Response,
  options: RequestOptions<T>,
  url: string,
): Promise<mixed> {
  const type = response.headers.get("content-type") ?? "";
  let value: mixed;
  try {
    if (response.status === 204 || response.headers.get("content-length") === "0") {
      value = undefined;
    } else if (type.includes("json")) {
      value = await response.json();
    } else if (type.startsWith("text/") || type === "") {
      value = await response.text();
    } else {
      value = await response.arrayBuffer();
    }
  } catch (cause) {
    throw new FetchError(url, { kind: "parse", cause });
  }

  const check = options.parse;
  if (check == null) {
    return value;
  }
  const checked = check(value);
  if (!checked.ok) {
    // At the boundary, where the shape came from outside — not three frames
    // later as a `TypeError` about a property of undefined.
    throw new FetchError(url, { kind: "invalid", issues: checked.issues });
  }
  return checked.value;
}

function resolveUrl(settings: $FlowFixMe, path: string, options: RequestOptions<mixed>): string {
  const base = settings.baseURL;
  const joined =
    base == null || /^[a-z][a-z0-9+.-]*:/i.test(path)
      ? path
      : `${base.replace(/\/+$/, "")}/${path.replace(/^\/+/, "")}`;

  const search = options.searchParams;
  if (search == null) {
    return joined;
  }
  const query = new URLSearchParams();
  for (const key of Object.keys(search)) {
    query.set(key, String(search[key]));
  }
  const text = query.toString();
  if (text === "") {
    return joined;
  }
  return joined.includes("?") ? `${joined}&${text}` : `${joined}?${text}`;
}

function pause(millis: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, millis));
}
