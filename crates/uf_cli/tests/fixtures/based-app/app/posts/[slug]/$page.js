// @flow
//
// A route with a parameter and no `generateStaticParams`, so nothing is
// prerendered for it and every server has to render it — handed the
// application path, with the base already taken off.

import * as React from "@uniflowed/react";

export default component Post(params: { readonly slug: string }) {
  // One text node, for the reason `served-app`'s copy of this page gives.
  return <h1>{`post: ${params.slug}`}</h1>;
}
