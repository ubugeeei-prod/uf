// @flow
import { AsyncLocalStorage } from "node:async_hooks";
import { processWide } from "./process-state.js";

const scopes: AsyncLocalStorage<string> = processWide(
  "data-cache-scope@1",
  () => new AsyncLocalStorage(),
);

export function inDataScope<T>(name: string, produce: () => Promise<T>): Promise<T> {
  return scopes.run(name, produce);
}

/** Reject before returning private data, including through indirect helpers. */
export function refuseRequestInDataScope(binding: string): void {
  const name = scopes.getStore();
  if (name != null) {
    throw new Error(
      `@uniflowed/server: cached function ${JSON.stringify(name)} cannot read ${binding}(). ` +
        "Read request data outside the cached function and pass its public inputs explicitly.",
    );
  }
}
