// @flow
//
// A route handler that says which path it was handed. Every front door takes
// the base path off before the application sees a request, so the answer is
// `/api/health` whichever door the request came through.

import { object, string } from "@uniflowed/validator";

export const schemas = {
  GET: {
    response: object({ status: string(), path: string() }),
  },
};

export function GET(request: Request): Response {
  return Response.json({ status: "ok", path: new URL(request.url).pathname });
}
