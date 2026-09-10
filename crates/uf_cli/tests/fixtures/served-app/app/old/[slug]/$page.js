// @flow
//
// A route whose loader redirects.
//
// It is here because a redirect is the one answer whose whole content is a
// *header*. `createRenderer` answers a `RedirectError` with a 307, a `Location`
// and a meta-refresh document, and the document is the fallback for a static
// host that can only serve a file — so a server that writes the status and the
// body and drops the headers produces a 307 pointing nowhere, which a browser
// papers over by obeying the meta refresh one paint late and which `curl -I`
// and every other client cannot follow at all. `uf dev` did exactly that. See
// ubugeeei-prod/uf#338.
//
// A parameter and no `generateStaticParams`, for the reason `app/posts/[slug]`
// gives: it keeps the route out of the prerender, so there is no file in
// `dist/` for the static half of `uf preview` and `uf start` to answer with,
// and all three servers have to render it. That is what makes it the same
// question asked of each of them.

import * as React from "@uniflowed/react";
import { redirect, type LoaderArgs } from "@uniflowed/router";

export async function loader({ params }: LoaderArgs): Promise<empty> {
  redirect(`/posts/${params.slug}`);
}

// Never rendered — the loader throws before it can be — and still required: a
// page is a module with a component, and a route table entry without one is
// not a route. It is what would be served if the redirect above ever stopped
// happening, which is the failure this file exists to notice.
export default component Moved() {
  return <h1>served-app moved</h1>;
}
