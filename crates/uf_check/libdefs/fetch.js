/**
 * @fileoverview The Fetch classes where the vendored declarations fall short:
 * `Response` without its static `json`, `AbortSignal` with its factories on the
 * wrong side, `Headers` without `getSetCookie`, a `Request` that will not take
 * another `Request` as its init, `RequestOptions` without `duplex`, and
 * `HeadersInit`/`URLSearchParams` that insist on a writable table. Each
 * declaration below says what it changes and why.
 *
 * Flow's `evals/flow-typed/environment/bom.js` declares `Response.error()` and
 * `Response.redirect()` and stops there. `Response.json(data, init)`, from the
 * Fetch standard, is implemented by every runtime uf targets (Node 18+, Deno,
 * Bun, Cloudflare Workers and current browsers), and it is how a route handler
 * answers with JSON. Without it, `Response.json(...)` was "property json is
 * missing in statics of Response" in every handler that used it: 32 errors in
 * this repository (ubugeeei-prod/uf#1451).
 *
 * # How this replaces the vendored one
 *
 * Library definitions are merged in reverse declaration order, so a later
 * file's `Response` shadows an earlier one's. That is the same mechanism
 * `web-crypto.js` uses for `Crypto`, and the reason `ENVIRONMENTS` lists uf's
 * own libdefs last. The class is therefore redeclared **whole**: every member
 * below is `bom.js`'s, unchanged, because a shadow that dropped one would trade
 * one error for another. The only addition is `json`.
 *
 * `data` is `mixed` because the standard serializes whatever it is given with
 * `JSON.stringify`. A value that does not serialize throws at run time, which
 * is not something a type can say.
 */

declare class Response {
  constructor(input?: ?BodyInit, init?: ResponseOptions): void;
  clone(): Response;
  static error(): Response;
  static redirect(url: string, status?: number): Response;
  /**
   * A response whose body is `JSON.stringify(data)`, with `content-type:
   * application/json` unless `init.headers` names one.
   */
  static json(data: mixed, init?: ResponseOptions): Response;

  redirected: boolean;
  type: ResponseType;
  url: string;
  ok: boolean;
  status: number;
  statusText: string;
  headers: Headers;
  trailer: Promise<Headers>;

  // Body methods and attributes
  bodyUsed: boolean;
  body: ?ReadableStream;

  arrayBuffer(): Promise<ArrayBuffer>;
  blob(): Promise<Blob>;
  formData(): Promise<FormData>;
  json(): Promise<any>;
  text(): Promise<string>;
}

/**
 * `AbortSignal`, with its factories where the standard puts them.
 *
 * The vendored `dom.js` declares `abort(reason)` and `timeout(time)` as
 * *instance* methods, which no runtime has, and leaves out `any(signals)`
 * entirely. So `AbortSignal.timeout(3000)`, the usual way to give a `fetch` a
 * deadline, was "property timeout is missing in statics of AbortSignal". Here
 * all three are statics, as the DOM standard and every runtime uf targets
 * have them. The instance members are `dom.js`'s, unchanged, except that the
 * two misplaced factories are gone: calling `signal.timeout(…)` fails at run
 * time, so declaring it only hid a bug.
 */
declare class AbortSignal extends EventTarget {
  readonly aborted: boolean;
  readonly reason: any;
  onabort: (event: Event) => mixed;
  throwIfAborted(): void;
  /** A signal already aborted with `reason`. */
  static abort(reason?: mixed): AbortSignal;
  /** A signal that aborts with a `TimeoutError` after `milliseconds`. */
  static timeout(milliseconds: number): AbortSignal;
  /** A signal that aborts when any of `signals` does, with that one's reason. */
  static any(signals: Iterable<AbortSignal>): AbortSignal;
}

/**
 * What a `Headers` or a request's `headers` may be built from.
 *
 * The vendored alias asks for a writable `{ [key: string]: string }` and a
 * mutable `Array`. The constructor only reads its argument, so a readonly map
 * or a `$ReadOnlyArray` of pairs, which is what a module that keeps its
 * defaults in a frozen table has, was refused. Both are accepted here.
 */
type HeadersInit =
  | Headers
  | $ReadOnlyArray<[string, string]>
  | { readonly [key: string]: string, ... };

/**
 * The `Headers` class, with `getSetCookie`.
 *
 * `getSetCookie()` returns each `Set-Cookie` header on its own, where `get`
 * joins them with a comma that is also legal inside a cookie's expiry. It is
 * in the Fetch standard and in every runtime uf targets. The rest is
 * `bom.js`'s, unchanged, apart from taking the readonly `HeadersInit` above.
 */
declare class Headers {
  @@iterator(): Iterator<[string, string]>;
  constructor(init?: HeadersInit): void;
  append(name: string, value: string): void;
  delete(name: string): void;
  entries(): Iterator<[string, string]>;
  forEach<This>(
    callback: (this: This, value: string, name: string, headers: Headers) => mixed,
    thisArg: This,
  ): void;
  get(name: string): null | string;
  /** Every `Set-Cookie` header, one string each. */
  getSetCookie(): Array<string>;
  has(name: string): boolean;
  keys(): Iterator<string>;
  set(name: string, value: string): void;
  values(): Iterator<string>;
}

/**
 * `URLSearchParams`, taking a readonly map or list of pairs for the same
 * reason as `HeadersInit`: the constructor only reads it. The rest is
 * `bom.js`'s, unchanged.
 */
declare class URLSearchParams {
  @@iterator(): Iterator<[string, string]>;

  size: number;

  constructor(
    init?:
      | string
      | URLSearchParams
      | $ReadOnlyArray<[string, string]>
      | { readonly [string]: string, ... },
  ): void;
  append(name: string, value: string): void;
  delete(name: string, value?: string): void;
  entries(): Iterator<[string, string]>;
  forEach<This>(
    callback: (this: This, value: string, name: string, params: URLSearchParams) => mixed,
    thisArg: This,
  ): void;
  get(name: string): null | string;
  getAll(name: string): Array<string>;
  has(name: string, value?: string): boolean;
  keys(): Iterator<string>;
  set(name: string, value: string): void;
  sort(): void;
  values(): Iterator<string>;
  toString(): string;
}

/**
 * The options a `Request` or a `fetch` takes, with `duplex`.
 *
 * `duplex: "half"` is what the Fetch standard requires beside a streaming
 * request body, and Node refuses a stream body without it. The rest is
 * `bom.js`'s, unchanged. A Node `Readable` is still not a `BodyInit`: only
 * Node takes one, and uf targets Deno and Bun as well.
 */
type RequestOptions = {
  body?: ?BodyInit,
  cache?: CacheType,
  credentials?: CredentialsType,
  duplex?: "half",
  headers?: HeadersInit,
  integrity?: string,
  keepalive?: boolean,
  method?: string,
  mode?: ModeType,
  redirect?: RedirectType,
  referrer?: string,
  referrerPolicy?: ReferrerPolicyType,
  signal?: ?AbortSignal,
  window?: any,
  ...
};

/**
 * The `Request` class, taking another `Request` as its init.
 *
 * `new Request(url, request)` copies `request`'s method, headers, body and
 * the rest onto a new URL. The standard reads the init as a dictionary, so
 * any object with those members works, and a `Request` is one. The vendored
 * declaration took only the `RequestOptions` object type, which a class
 * instance is never a subtype of. The rest is `bom.js`'s, unchanged.
 */
declare class Request {
  constructor(input: RequestInfo, init?: RequestOptions | Request): void;
  clone(): Request;

  url: string;

  cache: CacheType;
  credentials: CredentialsType;
  headers: Headers;
  integrity: string;
  method: string;
  mode: ModeType;
  redirect: RedirectType;
  referrer: string;
  referrerPolicy: ReferrerPolicyType;
  readonly signal: AbortSignal;

  // Body methods and attributes
  bodyUsed: boolean;

  arrayBuffer(): Promise<ArrayBuffer>;
  blob(): Promise<Blob>;
  formData(): Promise<FormData>;
  json(): Promise<any>;
  text(): Promise<string>;
}
