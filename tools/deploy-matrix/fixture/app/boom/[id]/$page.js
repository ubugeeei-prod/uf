// @flow
//
// A page that throws at request time. A parameter and no
// `generateStaticParams` keep it out of the build — a page that throws during
// the prerender fails `uf build` instead — so the throw happens on the target.

import * as React from "@uniflowed/react";

export default component Exploding(params: { readonly id: string }) {
  if (params.id !== "") {
    throw new Error(`deploy-matrix: /boom/${params.id} throws on purpose`);
  }
  // Unreachable for any URL (a route parameter is never empty); here so the
  // component has a render type without an annotation to explain.
  return <h1>matrix boom</h1>;
}
