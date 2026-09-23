// @flow
//
// The deploy-matrix fixture as `--adapter serverless` can take it.
//
// The same application as `../fixture/uf.config.js` with two differences, each
// the answer to a refusal the matrix asserts on the whole application first:
//
// * `app/ppr` is removed from the copy (a Lambda buffers its response, so a
//   static shell would arrive with its hole rather than before it);
// * the route cache is kept by `./s3-cache.js`, a project provider over an S3
//   bucket (Kumo's in the matrix), because a Lambda has no store of its own for
//   a regenerated page.

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
