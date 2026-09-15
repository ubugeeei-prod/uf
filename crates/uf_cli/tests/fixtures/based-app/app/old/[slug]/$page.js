// @flow
//
// A route whose loader redirects.
//
// `redirect()` names an application path, the way a `Link`'s `to` does, and a
// browser follows an address — so the `Location` every server answers with has
// to carry the base path. A parameter and no `generateStaticParams` keeps the
// route out of the prerender, so each server renders it.

import * as React from "@uniflowed/react";
import { redirect, type LoaderArgs } from "@uniflowed/router";

export async function loader({ params }: LoaderArgs): Promise<empty> {
  redirect(`/posts/${params.slug}`);
}

// Never rendered: the loader throws first.
export default component Moved() {
  return <h1>based-app moved</h1>;
}
