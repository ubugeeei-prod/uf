// @flow
import { observations } from "../_shared/observations.js";

export function GET() {
  return Response.json({ starts: observations.starts, errors: observations.errors, spans: observations.spans() });
}
