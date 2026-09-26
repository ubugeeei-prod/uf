// @flow
//
// `@uniflowed/fetch`: typed HTTP over the platform's own `fetch`.
//
// The platform's `fetch` is the right primitive and this does not replace it:
// `raw` hands the `Response` straight back. What it adds is the three things
// every application writes around it and gets subtly wrong — a failed response
// being a resolved promise, no timeout at all, and retrying things that must
// not be retried.

export type {
  FetchClient,
  FetchConfig,
  FetchFailure,
  Parse,
  RequestOptions,
} from "./internal/client.js";

export { FetchError, createFetch } from "./internal/client.js";
