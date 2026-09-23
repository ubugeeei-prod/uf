// @flow
//
// The on-demand half of `isr`: expire a tag or a path from a route handler.
//
// `revalidateTag` answers how many entries *this process* forgot; the durable
// store (a file, Workers KV, S3) loses every entry carrying the tag before the
// request finishes, which is what the check asserts by asking for the page
// again rather than by reading this number.

import { revalidatePath, revalidateTag } from "@uniflowed/server/cache";

/** `?tag=` or `?path=`; answers with what this process expired. */
export function POST(request: Request): Response {
  const url = new URL(request.url);
  const tag = url.searchParams.get("tag");
  const path = url.searchParams.get("path");
  if (tag != null) {
    return Response.json({ tag, expired: revalidateTag(tag) });
  }
  if (path != null) {
    return Response.json({ path, expired: revalidatePath(path) });
  }
  return Response.json({ problem: "name a tag or a path" }, { status: 400 });
}
