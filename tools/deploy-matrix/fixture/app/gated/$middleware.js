// @flow
//
// Middleware that rewrites: `/gated/<id>` is answered by `/ssr/<id>` and the
// address stays `/gated/<id>`. It guards a subtree with no page of its own, so
// only the middleware can make that path answer at all.

import type { Rewrite } from "@uniflowed/router/middleware";
import { rewrite } from "@uniflowed/router/middleware";

export default function middleware(request: Request): Rewrite | void {
  const id = new URL(request.url).pathname.split("/")[2];
  if (id != null && id !== "") {
    return rewrite(`/ssr/${id}`);
  }
}
