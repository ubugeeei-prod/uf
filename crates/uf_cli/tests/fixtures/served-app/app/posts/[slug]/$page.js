// @flow
//
// A route with a parameter and no `generateStaticParams`.
//
// That combination is the one `driver.js`'s `staticPaths` skips entirely: it
// is not prerendered, there is no file for it in `dist/`, and before `uf
// start` existed a built application had no way to answer it at all. The slug
// is echoed back so a test can tell a render from a cached document.

import * as React from "@uniflowed/react";

export default component Post(params: { readonly slug: string }) {
  // One text node rather than `post: {params.slug}`, which React renders with
  // a `<!-- -->` separator between the two children — invisible on the page and
  // in the way of an assertion that the slug arrived.
  return <h1>{`post: ${params.slug}`}</h1>;
}
