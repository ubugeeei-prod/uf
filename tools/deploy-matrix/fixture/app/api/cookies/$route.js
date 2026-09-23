// @flow
//
// Two `Set-Cookie` headers on one answer — the adapter contract's rule 5 ("keep
// every Set-Cookie separate"), which a platform that joins headers with a comma
// breaks for every cookie with an `Expires` attribute — and the request's own
// `Cookie` header read back, so both directions are asked.

/** Sets `a` and `b`; answers with the `cookie` header it was sent. */
export function GET(request: Request): Response {
  const response = Response.json({ received: request.headers.get("cookie") ?? "" });
  response.headers.append("set-cookie", "a=1; Path=/; HttpOnly");
  response.headers.append("set-cookie", "b=2; Path=/; Expires=Wed, 21 Oct 2037 07:28:00 GMT");
  return response;
}
