// @flow
//
// `uf dev` renders inside a cache scope, the way `uf start` does.
//
// The middleware in `index.js` cannot import `@uniflowed/server/cache` — Vite
// loads that file before any Flow transform — so the scope lives in
// `internal/dev-cache.js`. This is the regression for a component that called
// `cacheLife` and threw under `uf dev` while the same call rendered under
// `uf start`. ubugeeei-prod/uf#1703

import { describe, expect, it } from "@uniflowed/test";
import { cacheLife, OutsideCacheScopeError } from "@uniflowed/server/cache";

import { inDevCacheScope } from "./internal/dev-cache.js";

describe("a render under uf dev", () => {
  it("may call cacheLife, and a call outside one still may not", async () => {
    const value = await inDevCacheScope(async () => {
      cacheLife({ revalidate: 60 });
      return "rendered";
    });
    expect(value).toBe("rendered");

    let raised: mixed = null;
    try {
      cacheLife({ revalidate: 60 });
    } catch (error) {
      raised = error;
    }
    expect(raised instanceof OutsideCacheScopeError).toBe(true);
  });
});
