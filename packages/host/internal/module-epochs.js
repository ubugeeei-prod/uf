// @noflow
//
// Plain JavaScript: this is part of the loader, so it cannot be Flow.
//
// Loading an edited module again in a process that already loaded it.
//
// `uf test --watch` keeps its workers between runs, so a rerun costs the file
// and whatever the edit touched, not a Node start and the whole dependency
// graph again. A process keeps every module it evaluated for as long as it
// lives, though, and no hook can reach into an instance that is already
// linked. The one thing a hook can do is hand out a *different URL* for a
// module, and a different URL is a different module: evaluated from the file
// as it is now, linked to whatever its own imports resolve to now.
//
// So this module records, as the resolve hook sees them, which loaded module
// imported which. When `uf` says which files changed, every module that
// reaches one of them — the changed file and each importer up to the test
// file — is given a new epoch, and from then on the resolve hook answers an
// import of it with `?uf-epoch=<n>`. Everything else keeps the instance it
// has, which is the same sharing a worker already does between two files.
//
// The graph is the one the process actually loaded, not one read off the
// source: a bare `@uniflowed/ui` a scan would not follow, a dynamic `import()`
// that only happened at run time and a package's own internal imports are all
// edges here, because the hook saw each of them resolve.
//
// # What it refuses
//
// A module reached through `require()` lives in the CommonJS cache under its
// path, whatever URL anyone asks for, so it cannot be loaded again this way.
// When an edit reaches one, `invalidate` answers `false` and changes nothing,
// and `uf` replaces the worker with a fresh one instead: slower, never stale.

import { fileURLToPath } from "node:url";

/** Where the state lives, so the worker can reach it without importing this. */
export const MODULE_EPOCHS = Symbol.for("@uniflowed/host/module-epochs");

/** The query parameter an epoch is carried in. */
export const EPOCH_PARAM = "uf-epoch";

/**
 * The state for this process, made on first use.
 *
 * On the global object under a registered symbol, because two copies of this
 * module — one reached as a file, one through the package — must still be one
 * graph, and because the worker, which asks for the invalidation, is not the
 * code that installed the hooks.
 */
export function moduleEpochs() {
  const existing = globalThis[MODULE_EPOCHS];
  if (existing != null) return existing;
  const state = createModuleEpochs();
  Object.defineProperty(globalThis, MODULE_EPOCHS, {
    value: state,
    configurable: true,
    enumerable: false,
    writable: false,
  });
  return state;
}

/** A fresh graph and epoch table, for one process or one test. */
export function createModuleEpochs() {
  /** Module path -> the paths of the modules that imported it. */
  const importers = new Map();
  /** Module path -> the epoch it was last invalidated in. */
  const epochs = new Map();
  /** Paths some `require()` reached: those cannot be loaded twice. */
  const required = new Set();
  let counter = 0;

  return {
    /**
     * Remember that `parentURL` reached `url`, and answer the URL to load.
     *
     * `isImport` is false for a `require()`, which is recorded and never
     * rewritten. A URL that already carries a query is left alone: the test
     * file's own `?uf-run=`, a module mock's revision, and an epoch this
     * function added on an earlier call all name the instance they want.
     */
    resolved(parentURL, url, isImport) {
      if (typeof url !== "string" || !url.startsWith("file:")) return url;
      const path = pathOf(url);
      if (typeof parentURL === "string" && parentURL.startsWith("file:")) {
        const parent = pathOf(parentURL);
        let known = importers.get(path);
        if (known == null) {
          known = new Set();
          importers.set(path, known);
        }
        known.add(parent);
      }
      if (!isImport) {
        required.add(path);
        return url;
      }
      if (epochs.size === 0 || url.includes("?")) return url;
      const epoch = epochs.get(path);
      return epoch == null ? url : `${url}?${EPOCH_PARAM}=${epoch}`;
    },

    /**
     * Load `paths` and everything that imports them afresh from now on.
     *
     * Answers whether that could be promised. `false` means a module in the
     * way was reached by `require()`, and nothing was changed: the caller has
     * to use a process that never loaded it.
     */
    invalidate(paths) {
      const reached = new Set();
      const queue = [];
      for (const path of paths) {
        if (typeof path === "string" && !reached.has(path)) {
          reached.add(path);
          queue.push(path);
        }
      }
      while (queue.length > 0) {
        const path = queue.pop();
        for (const importer of importers.get(path) ?? []) {
          if (!reached.has(importer)) {
            reached.add(importer);
            queue.push(importer);
          }
        }
      }
      for (const path of reached) {
        if (required.has(path)) return false;
      }
      counter += 1;
      for (const path of reached) epochs.set(path, counter);
      return true;
    },

    /** The epoch `path` is loaded under now, or `0` when it never changed. */
    epochOf(path) {
      return epochs.get(path) ?? 0;
    },
  };
}

/** A `file:` URL's path, without the query or fragment that name an instance. */
function pathOf(url) {
  const end = url.search(/[?#]/);
  return fileURLToPath(end === -1 ? url : url.slice(0, end));
}
