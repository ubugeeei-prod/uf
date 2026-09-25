// @flow

import { createFetch } from "@uniflowed/fetch";
import { createEnvironment } from "@uniflowed/graphql";
import type { Environment } from "@uniflowed/graphql";

/** A fresh normalized store for one request or mounted browser application. */

export function environment(endpoint: string, cookie?: string): Environment {
  return createEnvironment({
    endpoint,
    fetch: createFetch(),
    headers: cookie == null ? {} : { cookie },
  });
}
