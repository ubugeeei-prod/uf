// @flow
//
// `@uniflowed/fetch`.

import type { NativeHandle } from "@uniflowed/core/native";
import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/fetch";

export opaque type FetchClient = NativeHandle<"@uniflowed/core/fetch#FetchClient">;

export type FetchConfig = {
  +baseURL?: string,
  +headers?: { +[string]: string },
};

export type RequestOptions = {
  +method?: "GET" | "POST" | "PUT" | "PATCH" | "DELETE",
  +body?: mixed,
  +headers?: { +[string]: string },
};

export function ofetch(config?: FetchConfig): FetchClient {
  return nativeRuntimeRequired(MODULE, "ofetch");
}

export function createFetch(config?: FetchConfig): FetchClient {
  return nativeRuntimeRequired(MODULE, "createFetch");
}

export function request<T>(
  client: FetchClient,
  path: string,
  init?: RequestOptions,
): Promise<T> {
  return nativeRuntimeRequired(MODULE, "request");
}
