// @flow
//
// `ssr`: a page rendered for each request.
//
// A parameter and no `generateStaticParams` keeps it out of the prerender, and
// it reads a request header, which only a render at request time can see — so
// an answer carrying the probe is an answer the target rendered, not a file.
// `none` throws `notFound()`, which is the project's 404 reached from inside a
// render rather than from a path no route matches.

import * as React from "@uniflowed/react";
import { notFound } from "@uniflowed/router";
import { headers } from "@uniflowed/server";

export default component Rendered(params: { readonly id: string }) {
  if (params.id === "none") {
    notFound();
  }
  const probe = headers().get("x-matrix-probe") ?? "absent";
  return <h1>{`ssr: ${params.id} probe: ${probe}`}</h1>;
}
