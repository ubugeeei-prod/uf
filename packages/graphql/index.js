// @flow
//
// `@uniflowed/graphql`.

import type { FetchClient } from "@uniflowed/fetch";
import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/graphql";

/** A compiled operation, keyed by both its data and its variables. */
export opaque type GraphQlOperation<TData, TVariables> = {
  readonly __ufNative: "@uniflowed/core/graphql#GraphQlOperation",
  __ufData: TData,
  __ufVariables: TVariables,
};

export type GraphQlClient = {
  readonly fetch: FetchClient,
  readonly relayBase: true,
};

export function graphql<TData, TVariables extends { ... }>(
  text: string,
): GraphQlOperation<TData, TVariables> {
  return nativeRuntimeRequired(MODULE, "graphql");
}

export function createGraphQlClient(config: { readonly fetch: FetchClient }): GraphQlClient {
  return nativeRuntimeRequired(MODULE, "createGraphQlClient");
}

export function useLazyLoadQuery<TData, TVariables extends { ... }>(
  operation: GraphQlOperation<TData, TVariables>,
  variables: TVariables,
): Promise<TData> {
  return nativeRuntimeRequired(MODULE, "useLazyLoadQuery");
}
