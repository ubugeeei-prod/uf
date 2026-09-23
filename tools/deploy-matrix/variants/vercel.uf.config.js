// @flow
//
// The deploy-matrix fixture as `--adapter vercel` can take it.
//
// The whole application — a Vercel function streams, so `app/ppr` stays — with
// one difference, the answer to the refusal the matrix asserts on the whole
// application first: the route cache is kept by `./s3-cache.js`, a project
// provider over an S3 bucket (Kumo's in the matrix), because a function
// instance's memory and `/tmp` go with the instance and the adapter has no
// store of its own for a regenerated page.

import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  app: {
    router: {
      entry: "app.js",
      root: "app",
      redirects: [{ source: "/moved/:slug", destination: "/posts/:slug", permanent: true }],
      rewrites: [{ source: "/articles/:slug", destination: "/posts/:slug" }],
      headers: [
        { source: "/:path*", headers: { "x-matrix": "deploy-matrix" } },
        { source: "/api/:rest*", headers: { "cache-control": "no-store" } },
      ],
    },
    rendering: {
      cache: { route: true, store: "./s3-cache.js" },
    },
  },
  build: {
    entries: ["app.js"],
    outDir: "dist",
  },
});
