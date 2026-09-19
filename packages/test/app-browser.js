// @flow
export function createTestApp(_options?: mixed): empty {
  throw new Error(
    "createTestApp runs in a Node test worker. Run this test with uf test, and use createBrowser() to check its RSC hydration in Chromium.",
  );
}
