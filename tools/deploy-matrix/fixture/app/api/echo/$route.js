// @flow
//
// A route handler: the clearest thing only a server answers. Both methods echo
// what they were sent, so a `200` from a host's fallback page cannot pass for
// the handler's answer.

/** The method and query string, as JSON. */
export function GET(request: Request): Response {
  const url = new URL(request.url);
  return Response.json({ method: "GET", query: url.searchParams.get("q") ?? "" });
}

/** The JSON body's `name`, echoed. */
export async function POST(request: Request): Promise<Response> {
  const body = await request.json();
  return Response.json({ method: "POST", echoed: String(body?.name ?? "") });
}
