// @flow

import { traceRequestPhase } from "@uniflowed/server/instrumentation";
import { NotFoundError, RedirectError } from "./routing.js";

export function traceLoader(body: () => mixed | Promise<mixed>): Promise<mixed> {
  return traceRequestPhase(
    "loader",
    async () => body(),
    (error) => error instanceof RedirectError || error instanceof NotFoundError,
  );
}
