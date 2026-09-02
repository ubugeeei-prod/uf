// @flow
//
// `@uniflowed/relay`.

import type { NativeHandle } from "@uniflowed/core/native";
import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/relay";

export opaque type GraphQLTaggedNode =
  NativeHandle<"@uniflowed/core/relay#GraphQLTaggedNode">;

export function graphql(source: string): GraphQLTaggedNode {
  return nativeRuntimeRequired(MODULE, "graphql");
}

export function useFragment<T>(
  fragment: GraphQLTaggedNode,
  key: mixed,
): T {
  return nativeRuntimeRequired(MODULE, "useFragment");
}

export function useLazyLoadQuery<T>(
  query: GraphQLTaggedNode,
  variables: { +[string]: mixed },
): T {
  return nativeRuntimeRequired(MODULE, "useLazyLoadQuery");
}

export function commitMutation<T>(config: {
  +mutation: GraphQLTaggedNode,
  +variables: { +[string]: mixed },
}): Promise<T> {
  return nativeRuntimeRequired(MODULE, "commitMutation");
}
