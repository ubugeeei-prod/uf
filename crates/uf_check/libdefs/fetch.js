/**
 * @fileoverview The Fetch `Response` class, with the static `json` the vendored
 * one is missing.
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
