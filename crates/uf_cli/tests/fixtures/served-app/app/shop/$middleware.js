// @flow
//
// A middleware that rewrites: `/shop/<slug>` is answered by `/posts/<slug>`,
// and the address stays `/shop/<slug>`.
//
// It guards a subtree with no page of its own, so no prerendered document sits
// under it and the sitemap assertions are unchanged by it. What it adds to the
// questions every front door is asked is the rewrite's own: the destination
// renders, and a navigating browser's payload request for `/shop/<slug>` is the
// destination's payload.

import type { Rewrite } from "@uniflowed/router/middleware";
import { rewrite } from "@uniflowed/router/middleware";

export default function middleware(request: Request): Rewrite | void {
  const slug = new URL(request.url).pathname.split("/")[2];
  if (slug != null && slug !== "") {
    return rewrite(`/posts/${slug}`);
  }
}
