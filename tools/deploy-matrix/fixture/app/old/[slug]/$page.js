// @flow
//
// A loader redirect: the answer's whole content is its `Location` header, so a
// target that writes the status and the body and drops the headers is caught.
// A parameter and no `generateStaticParams`, so every target has to render it.

import * as React from "@uniflowed/react";
import { notFound, redirect, type LoaderArgs } from "@uniflowed/router";

export async function loader({ params }: LoaderArgs): Promise<empty> {
  const { slug } = params;
  // `[slug]` is one segment; only a catch-all segment is handed an array.
  if (typeof slug !== "string") return notFound();
  return redirect(`/posts/${slug}`);
}

// Never rendered — the loader throws first — and still required, because a
// route is a module with a component.
export default component Old() {
  return <h1>matrix old</h1>;
}
