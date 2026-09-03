// Plain JavaScript: executed by the host that runs Vite, before any transform.
//
// React Fast Refresh wiring for `uf dev`. The runtime itself is
// `./refresh-runtime.js`, Meta's `react-refresh` runtime as simplified by
// `@vitejs/plugin-react` (MIT); this file is the glue that plugin-react adds
// around it, kept structurally identical so the two stay comparable:
//
// * the runtime is served at `/@react-refresh` as a virtual module;
// * a preamble installs it on `window` before any component module loads;
// * every module that registered a component gets a header that points
//   `$RefreshReg$`/`$RefreshSig$` at this module and a footer that validates
//   the boundary and enqueues the refresh on `import.meta.hot.accept`.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

/** Public URL the refresh runtime is served from. */
export const RUNTIME_PUBLIC_PATH = "/@react-refresh";

/** The resolved id Vite hands back for the runtime. */
export const RUNTIME_RESOLVED_ID = "\0uf:react-refresh";

const RUNTIME_SOURCE_PATH = fileURLToPath(new URL("./refresh-runtime.js", import.meta.url));

/** The runtime's source, read once. */
export function refreshRuntimeSource() {
  return readFileSync(RUNTIME_SOURCE_PATH, "utf8");
}

/**
 * The script every HTML document loads first in development.
 *
 * `base` is Vite's `config.base`, so the runtime resolves under a sub-path
 * deployment as well as at the root.
 */
export function preambleCode(base = "/") {
  return `import { injectIntoGlobalHook } from "${base}${RUNTIME_PUBLIC_PATH.slice(1)}";
injectIntoGlobalHook(window);
window.$RefreshReg$ = () => {};
window.$RefreshSig$ = () => (type) => type;`;
}

const REFRESH_CONTENT = /\$RefreshReg\$\(/;
const REACT_CLASS_COMPONENT = /extends\s+(?:React\.)?(?:Pure)?Component/;

/**
 * Wrap a transformed module with the Fast Refresh header and footer.
 *
 * A module that registered nothing and defines no class component comes back
 * untouched, so the wrapper never costs a module that has no component in it.
 * The source map is shifted by the number of lines prepended, which is what
 * keeps a stack trace pointing at the author's line.
 */
export function addRefreshWrapper(code, map, id) {
  const hasRefresh = REFRESH_CONTENT.test(code);
  const onlyReactComponent = !hasRefresh && REACT_CLASS_COMPONENT.test(code);
  if (!hasRefresh && !onlyReactComponent) {
    return { code, map };
  }

  const nextMap = typeof map === "string" ? JSON.parse(map) : map;
  let nextCode = code;

  if (hasRefresh) {
    nextCode = `let prevRefreshReg;
let prevRefreshSig;

if (import.meta.hot && !inWebWorker) {
  if (!window.$RefreshReg$) {
    throw new Error(
      "@uniflowed/vite can't detect the Fast Refresh preamble. Something is wrong."
    );
  }

  prevRefreshReg = window.$RefreshReg$;
  prevRefreshSig = window.$RefreshSig$;
  window.$RefreshReg$ = RefreshRuntime.getRefreshReg(${JSON.stringify(id)});
  window.$RefreshSig$ = RefreshRuntime.createSignatureFunctionForTransform;
}

${nextCode}

if (import.meta.hot && !inWebWorker) {
  window.$RefreshReg$ = prevRefreshReg;
  window.$RefreshSig$ = prevRefreshSig;
}
`;
    if (nextMap) nextMap.mappings = ";".repeat(16) + nextMap.mappings;
  }

  nextCode = `import * as RefreshRuntime from "${RUNTIME_PUBLIC_PATH}";
const inWebWorker = typeof WorkerGlobalScope !== 'undefined' && self instanceof WorkerGlobalScope;

${nextCode}

if (import.meta.hot && !inWebWorker) {
  RefreshRuntime.__hmr_import(import.meta.url).then((currentExports) => {
    RefreshRuntime.registerExportsForReactRefresh(${JSON.stringify(id)}, currentExports);
    import.meta.hot.accept((nextExports) => {
      if (!nextExports) return;
      const invalidateMessage = RefreshRuntime.validateRefreshBoundaryAndEnqueueUpdate(${JSON.stringify(id)}, currentExports, nextExports);
      if (invalidateMessage) import.meta.hot.invalidate(invalidateMessage);
    });
  });
}
`;
  if (nextMap) nextMap.mappings = ";;;" + nextMap.mappings;

  return { code: nextCode, map: nextMap };
}
