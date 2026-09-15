// @flow
//
// What an application does when the data behind a page changes: it invalidates
// the page's tag. The regeneration tests call this on a running server and then
// restart it, because a restart after an invalidation is when a server used to
// start the page from the build's copy again.

import { revalidateTag } from "@uniflowed/server/cache";

export function POST(): Response {
  return Response.json({ expired: revalidateTag("clock") });
}
