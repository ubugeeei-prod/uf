// @flow
//
// `@uniflowed/mock`.

import type { NativeHandle } from "./internal/native-runtime.js";
import { nativeRuntimeRequired } from "./internal/native-runtime.js";

const MODULE = "@uniflowed/core/mock";

export opaque type MockRegistry = NativeHandle<"@uniflowed/core/mock#MockRegistry">;
export opaque type MockHandler = NativeHandle<"@uniflowed/core/mock#MockHandler">;

export function mock(): MockRegistry {
  return nativeRuntimeRequired(MODULE, "mock");
}

export function get(path: string, response: mixed): MockHandler {
  return nativeRuntimeRequired(MODULE, "get");
}

export function post(path: string, response: mixed): MockHandler {
  return nativeRuntimeRequired(MODULE, "post");
}

export function use(handler: MockHandler): MockRegistry {
  return nativeRuntimeRequired(MODULE, "use");
}
