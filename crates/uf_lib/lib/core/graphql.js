// @flow
//
// `@uniflowed/graphql`.

import type { FetchClient } from "./fetch.js";
import { nativeRuntimeRequired } from "./internal/native-runtime.js";

const MODULE = "@uniflowed/core/graphql";

/** A compiled operation, keyed by both its data and its variables. */
export opaque type GraphQlOperation<TData, TVariables> = {
  +__ufNative: "@uniflowed/core/graphql#GraphQlOperation",
  __ufData: TData,
  __ufVariables: TVariables,
};

export type GraphQlClient = {
  +fetch: FetchClient,
  +relayBase: true,
};

export function graphql<TData, TVariables: {...}>(
  text: string,
): GraphQlOperation<TData, TVariables> {
  return nativeRuntimeRequired(MODULE, "graphql");
}

export function createGraphQlClient(config: {
  +fetch: FetchClient,
}): GraphQlClient {
  return nativeRuntimeRequired(MODULE, "createGraphQlClient");
}

export function useLazyLoadQuery<TData, TVariables: {...}>(
  operation: GraphQlOperation<TData, TVariables>,
  variables: TVariables,
): Promise<TData> {
  return nativeRuntimeRequired(MODULE, "useLazyLoadQuery");
}
