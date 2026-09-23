// @flow
//
// `ssg` with parameters: `generateStaticParams` enumerates two posts, so both
// are prerendered — documents and RSC payloads — and a target answers them
// without rendering. The client navigation in the browser check goes between
// these two, so their payloads are what a `<Link>` fetches.

import * as React from "@uniflowed/react";

/** The two posts the build prerenders. */
export function generateStaticParams(): $ReadOnlyArray<{ readonly slug: string }> {
  return [{ slug: "first" }, { slug: "second" }];
}

export default component Post(params: { readonly slug: string }) {
  // One text node: React separates two children with `<!-- -->`, which would
  // be in the way of an assertion that the slug arrived.
  return <h1>{`post: ${params.slug}`}</h1>;
}
