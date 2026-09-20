// @noflow

import { existsSync } from "node:fs";
import path from "node:path";

export function instrumentationFile(root, client = false) {
  const stem = client ? "$instrumentation.client" : "$instrumentation";
  const files = [".js", ".jsx"]
    .map((extension) => path.join(root, stem + extension))
    .filter(existsSync);
  if (files.length > 1)
    throw new Error(`uf: choose one instrumentation module: ${files.join(", ")}`);
  return files[0] ?? null;
}

export function clientInstrumentationSource(file) {
  if (file == null) return "";
  return `import * as instrumentation from ${JSON.stringify(file)};
import { installClientInstrumentation } from "@uniflowed/router/instrumentation";
const disposeInstrumentation = await installClientInstrumentation(instrumentation);
if (import.meta.hot) import.meta.hot.dispose(disposeInstrumentation);
`;
}
