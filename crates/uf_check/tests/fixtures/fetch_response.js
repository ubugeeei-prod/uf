// @flow
//
// `Response.json`, which the vendored `bom.js` leaves out of `Response`'s
// statics, and the rest of the class, which `libdefs/fetch.js` repeats.

export function ok(): Response {
  return Response.json({ ok: true });
}

export function created(id: string): Response {
  return Response.json({ id }, { status: 201, headers: { "cache-control": "no-store" } });
}

export async function roundTrip(): Promise<mixed> {
  const response = Response.json([1, 2, 3]);
  const copy: Response = response.clone();
  const status: number = copy.status;
  const text: string = await response.text();
  return { status, text, gone: Response.error(), moved: Response.redirect("/", 308) };
}

export async function withDeadline(): Promise<Response> {
  const either = AbortSignal.any([AbortSignal.timeout(3000), AbortSignal.abort("stop")]);
  return fetch("https://example.com", { signal: either });
}
