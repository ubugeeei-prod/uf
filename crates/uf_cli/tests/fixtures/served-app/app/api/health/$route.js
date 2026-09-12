// @flow
//
// A route handler, which is the clearest thing a build could not serve:
// `createDispatcher` is complete and `uf build` never called it, so this
// module answered under `uf dev` and did not exist in `dist/`.

import { object, string } from "@uniflowed/validator";

export const schemas = {
  GET: {
    response: object({ status: string() }),
  },
  POST: {
    body: object({ name: string() }),
    response: object({ echoed: string() }),
  },
};

export function GET(): Response {
  return Response.json({ status: "ok" });
}

export async function POST(request: Request): Promise<Response> {
  const body = await request.json();
  // Echoed rather than acknowledged, so the assertion is that the request body
  // reached the handler — a 200 alone would also be what a static host's
  // fallback page returns.
  return Response.json({ echoed: body.name });
}
