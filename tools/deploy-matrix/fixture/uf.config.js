// @flow
//
// The deploy-matrix fixture's configuration: the whole application, which is
// what every target but two is built from.
//
// `serverless` and `static` refuse part of it by design. `tools/deploy-matrix`
// builds this whole application for them first and asserts the refusal, then
// builds a copy with the refused part taken out and this file replaced by
// `../variants/<target>.uf.config.js`. Those are whole files rather than a
// switch in this one because `uf` reads a config statically: a value computed
// before the default export is not evaluated.

import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  app: {
    router: {
      entry: "app.js",
      root: "app",
      // `app.router`'s three lists. `/moved` and `/articles` have no route of
      // their own, so a redirect or a render there can only be the rule's.
      // `x-matrix` is on every answer and `cache-control` only on route
      // handlers, so a target that applies rules to documents and forgets
      // files, or the reverse, is caught.
      redirects: [{ source: "/moved/:slug", destination: "/posts/:slug", permanent: true }],
      rewrites: [{ source: "/articles/:slug", destination: "/posts/:slug" }],
      headers: [
        { source: "/:path*", headers: { "x-matrix": "deploy-matrix" } },
        { source: "/api/:rest*", headers: { "cache-control": "no-store" } },
      ],
    },
    rendering: {
      // What makes `app/isr` and `app/tagged` regenerate rather than stay the
      // build's documents, kept in each target's default store.
      cache: { route: true },
    },
  },
  build: {
    entries: ["app.js"],
    outDir: "dist",
  },
});
