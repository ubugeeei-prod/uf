// @flow
//
// `@uniflowed/graphql`: the Relay environment, without the boilerplate.
//
// Relay itself is `@uniflowed/relay`, which is the real `react-relay`. This is
// the twenty lines every Relay application writes before it can use it: a
// network layer that posts an operation to an endpoint, a store, and an
// environment holding the two together.
//
// It exists because that boilerplate is not a design decision — every project
// writes the same thing, gets the response shape slightly wrong the first time,
// and discovers it when a GraphQL error is silently rendered as `null`. uf has
// an opinion about it, and the opinion is small enough to read.
//
// It is deliberately not a wrapper. `createEnvironment` hands back Relay's own
// `Environment`, so everything Relay can do a uf project can do, and a project
// that outgrows this function replaces the call rather than escaping a
// framework. That is red line 8, and this is what it looks like when it holds.

import type { FetchClient } from "@uniflowed/fetch";
import { Environment, Network, Observable, RecordSource, Store } from "relay-runtime";

export type { Environment } from "relay-runtime";

/** A GraphQL error as a server reports it. */
export type GraphQlError = {
  readonly message: string,
  readonly path?: $ReadOnlyArray<string | number>,
};

/** Raised when a response carries `errors`. */
export class GraphQlResponseError extends Error {
  /** Every error the response reported, in the order it reported them. */
  errors: $ReadOnlyArray<GraphQlError>;

  constructor(errors: $ReadOnlyArray<GraphQlError>) {
    const first = errors[0]?.message ?? "the server reported an error";
    super(errors.length === 1 ? first : `${first} (and ${errors.length - 1} more)`);
    this.name = "GraphQlResponseError";
    this.errors = errors;
  }
}

/** How to reach the GraphQL endpoint. */
export type EnvironmentOptions = {
  /** Where operations are posted. */
  readonly endpoint: string,
  /**
   * The fetch client to post with.
   *
   * Explicit rather than reaching for a global: uf does not override
   * `globalThis.fetch`, and a server rendering a request often needs to forward
   * that request's credentials, which a global cannot know about.
   */
  readonly fetch: FetchClient,
  /** Headers sent with every operation, e.g. an authorization token. */
  readonly headers?: { readonly [string]: string },
};

/**
 * A Relay environment that posts operations to `endpoint`.
 *
 * The store is fresh, so two calls are two independent caches — which is what a
 * server needs, where one environment per request is the only way two users do
 * not see each other's data.
 */
export function createEnvironment(options: EnvironmentOptions): Environment {
  return new Environment({
    network: Network.create(fetchOperation(options)),
    store: new Store(new RecordSource()),
  });
}

/**
 * The network function Relay calls to run one operation.
 *
 * Returns a promise rather than an `Observable`: Relay accepts either, and a
 * promise is the whole of what a plain HTTP endpoint can do. Subscriptions need
 * the observable form and a transport that supports them, which is a different
 * function and not one uf guesses at.
 */
function fetchOperation(options: EnvironmentOptions) {
  return async function run(operation: mixed, variables: mixed): Promise<mixed> {
    const request = operation as $FlowFixMe;
    // `raw` rather than `request`: a GraphQL endpoint answers 200 with an
    // `errors` array, so the status is not the outcome and the body has to be
    // read either way.
    const response = await options.fetch.raw(options.endpoint, {
      method: "POST",
      headers: {
        accept: "application/graphql-response+json, application/json",
        ...(options.headers ?? {}),
      },
      body: {
        query: request.text,
        variables,
        // Relay names every operation, and sending the name makes a server's
        // logs and traces readable without any extra work.
        operationName: request.name,
      },
    });

    const payload = (await response.json()) as $FlowFixMe;
    // A GraphQL response can carry `errors` and still be HTTP 200, and a
    // client that only checks the status renders `null` and says nothing.
    if (Array.isArray(payload?.errors) && payload.errors.length > 0) {
      throw new GraphQlResponseError(payload.errors);
    }
    return payload;
  };
}

export { Observable };
