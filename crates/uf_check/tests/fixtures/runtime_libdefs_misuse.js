// @flow
//
// Every line after a comment here must be a type error: the declarations are
// narrower than `any`, not merely present.

import { AsyncLocalStorage } from "node:async_hooks";
import fs from "node:fs";
import { pathToFileURL } from "node:url";

export function misuses(): mixed {
  // A `URL` is not a string.
  const notAString: string = pathToFileURL("/tmp/entry.js");

  // Asking for buffers gets buffers.
  const notStrings: Array<string> = fs
    .readdirSync("/tmp", { encoding: "buffer", withFileTypes: true })
    .map((entry) => entry.name);

  // `duplex` is "half" and nothing else.
  const full = new Request("https://example.com", { duplex: "full" });

  // A unit ECMA-402 does not define.
  const fortnight = new Intl.RelativeTimeFormat("en").format(1, "fortnight");

  // The store is what it was declared as, and may be absent.
  const storage: AsyncLocalStorage<number> = new AsyncLocalStorage();
  const notANumber: number = storage.getStore();

  return { notAString, notStrings, full, fortnight, notANumber };
}
