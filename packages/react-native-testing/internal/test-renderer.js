// @flow
//
// Node-only loader for the optional React Test Renderer peer.

import { createRequire } from "node:module";

export function loadTestRenderer(): mixed {
  const load = createRequire(import.meta.url);
  return load("react-test-renderer");
}
