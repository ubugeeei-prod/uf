// @flow
import {
  ACTION_CONTENT_TYPE,
  ACTION_HEADER,
  decodeActionResult,
  encodeActionArguments,
  isActionId,
  type ActionValue,
} from "./internal/action-wire.js";

export type RouteClientOptions = {|
  readonly origin: string,
  readonly getToken?: () => Promise<string>,
  readonly fetch?: (url: string, options: RequestOptions) => Promise<Response>,
  readonly allowInsecureDevelopment?: boolean,
|};

export type RouteRequest = {|
  readonly method?: string,
  readonly headers?: { readonly [string]: string },
  readonly body?: string,
  readonly signal?: AbortSignal,
|};

type RequestOptions = {|
  ...RouteRequest,
  readonly credentials: "omit",
  readonly redirect: "error",
|};

/** Fetch transport shared by generated typed route clients and native actions. */
export function createRouteClient(
  options: RouteClientOptions,
): (path: string, request?: RouteRequest) => Promise<Response> {
  const origin = new URL(options.origin);
  if (
    origin.username ||
    origin.password ||
    origin.pathname !== "/" ||
    origin.search ||
    origin.hash
  ) {
    throw new TypeError("createRouteClient origin must contain only a scheme and host");
  }
  const loopback = ["localhost", "127.0.0.1", "[::1]"].includes(origin.hostname);
  if (
    origin.protocol !== "https:" &&
    !(origin.protocol === "http:" && (loopback || options.allowInsecureDevelopment === true))
  ) {
    throw new TypeError(
      "createRouteClient requires HTTPS outside explicit development connections",
    );
  }
  const send = options.fetch ?? globalThis.fetch;
  return async (path, request = {}) => {
    if (!path.startsWith("/") || path.startsWith("//") || path.includes("\\")) {
      throw new TypeError("route client needs a same-origin absolute path");
    }
    const url = new URL(path, origin);
    if (url.origin !== origin.origin) throw new TypeError("route client cannot change origin");
    const headers: { [string]: string } = {};
    for (const [name, value] of Object.entries(request.headers ?? {})) {
      const lower = name.toLowerCase();
      if (
        ["cookie", "authorization", "origin", "host"].includes(lower) ||
        lower.startsWith("sec-fetch-")
      ) {
        throw new TypeError(`route client does not accept the ${name} header`);
      }
      if (typeof value !== "string") throw new TypeError(`route header ${name} must be a string`);
      headers[name] = value;
    }
    if (options.getToken != null) {
      const token = await options.getToken();
      if (!/^[A-Za-z0-9\-._~+/]+=*$/.test(token) || token.length > 8192) {
        throw new TypeError("route client received an invalid bearer credential");
      }
      headers.authorization = `Bearer ${token}`;
    }
    return send(url.href, { ...request, headers, credentials: "omit", redirect: "error" });
  };
}

/** Explicit action channel for clients holding application-issued bearer tokens. */
export function createNativeActionClient(options: {|
  ...RouteClientOptions,
  readonly getToken: () => Promise<string>,
|}): (id: string, args: $ReadOnlyArray<mixed>, path?: string) => Promise<ActionValue | void> {
  const send = createRouteClient(options);
  return async (id, args, path = "/") => {
    if (!isActionId(id)) throw new TypeError("native action needs an ID from the current build");
    const response = await send(path, {
      method: "POST",
      headers: {
        [ACTION_HEADER]: id,
        "uf-native-action": "bearer-v1",
        "content-type": ACTION_CONTENT_TYPE,
      },
      body: encodeActionArguments(args),
    });
    if (!response.ok) throw new Error(`Native action failed with status ${response.status}`);
    return decodeActionResult(await response.text());
  };
}
