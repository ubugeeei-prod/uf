// @noflow
//
// Plain JavaScript: this is part of the loader, so it cannot be Flow.
//
// Source maps read when a stack is, rather than when a module is.
//
// Every module the transform compiles carries its source map inline, and that
// comment is most of the module: four fifths of the bytes in
// `.uf/cache/transform` on this repository. Under `--enable-source-maps` Node
// reads it the moment it compiles the module — decodes the base64, parses the
// JSON, measures every line of the generated code — and V8 scans all of it
// first, whether or not anything the module does ever throws. `uf test` gives
// every test file its own copy of the project's modules, so a worker pays that
// once per module per file, and a suite of a few hundred files paid it tens of
// thousands of times to map the handful of stacks a failing run prints.
//
// So a `uf test` worker on Node hands V8 the code without the comment, keeps
// the map beside it, and maps a stack when one is formatted: V8 calls
// `Error.prepareStackTrace` the first time a stack is read, and only then is a
// map decoded — once per module, with Node's own `SourceMap`. A frame is
// written the way Node's source-mapped frames are, so a reader and
// `@uniflowed/test`'s frame parser see what they saw before.
//
// `uf` asks for this with `UF_TEST_LAZY_SOURCE_MAPS`, and only on a worker it
// started with `--enable-source-maps`: a `uf` that predates it still gets
// Node's own mapping from an `@uniflowed/host` that knows about it, and this
// `@uniflowed/host` under an older `uf` sees no variable and changes nothing.
// The variable is taken out of the environment as it is read, so a process a
// test starts is not put in this mode by having inherited it.

import { SourceMap } from "node:module";
import { fileURLToPath } from "node:url";
import path from "node:path";

const INLINE_MAP = "\n//# sourceMappingURL=data:application/json;base64,";

/**
 * Whether this process was asked to map lazily, and can: Node, with its own
 * source maps on, and `SourceMap` to decode with. Reading the answer takes the
 * request out of the environment.
 */
export function lazySourceMapsWanted() {
  const env = globalThis.process?.env;
  if (env == null || env.UF_TEST_LAZY_SOURCE_MAPS !== "1") return false;
  delete env.UF_TEST_LAZY_SOURCE_MAPS;
  return (
    globalThis.Deno == null &&
    globalThis.process.sourceMapsEnabled === true &&
    typeof globalThis.process.setSourceMapsEnabled === "function" &&
    typeof SourceMap === "function"
  );
}

/**
 * Take over mapping for this process, and return the call that detaches a
 * module's map from its code.
 */
export function installLazySourceMaps() {
  /** Each module's map by its path: the base64 until a stack asks, then the decoded map. */
  const maps = new Map();
  process.setSourceMapsEnabled(false);

  const mapFor = (fileName) => {
    if (typeof fileName !== "string") return null;
    let filename = fileName;
    if (fileName.startsWith("file:")) {
      try {
        filename = fileURLToPath(fileName);
      } catch {
        return null;
      }
    }
    const known = maps.get(filename);
    if (known == null || known instanceof SourceMap) return known ?? null;
    try {
      const decoded = new SourceMap(JSON.parse(Buffer.from(known, "base64").toString("utf8")));
      maps.set(filename, decoded);
      return decoded;
    } catch {
      maps.delete(filename);
      return null;
    }
  };

  /** One frame, mapped the way Node's `--enable-source-maps` writes it, or `null`. */
  const mapped = (site) => {
    const fileName = site.getFileName?.() ?? site.getScriptNameOrSourceURL?.();
    const map = mapFor(fileName);
    const line = site.getLineNumber();
    const column = site.getColumnNumber();
    if (map == null || line == null || column == null) return null;
    const entry = map.findEntry(line - 1, column - 1);
    if (entry?.originalSource == null || entry.originalLine == null) return null;
    const functionName = site.getFunctionName() ?? site.getMethodName();
    const typeName = site.getTypeName();
    const owner = typeName != null && typeName !== "global" ? `${typeName}.` : "";
    const prefix = site.isAsync?.() ? "async " : site.isConstructor?.() ? "new " : "";
    return (
      `${prefix}${owner}${functionName || "<anonymous>"} ` +
      `(${originalPath(entry.originalSource, fileName)}:${entry.originalLine + 1}:${entry.originalColumn + 1})`
    );
  };

  // `node:module`'s `findSourceMap`, for the maps Node no longer holds:
  // `@uniflowed/test` maps a registration's call site with it rather than by
  // printing a stack. Frozen, and on a symbol, like the file scope.
  Object.defineProperty(globalThis, Symbol.for("@uniflowed/host/source-maps"), {
    value: Object.freeze({ findSourceMap: mapFor }),
    configurable: true,
    enumerable: false,
    writable: false,
  });

  Error.prepareStackTrace = (error, trace) => {
    let header;
    try {
      // Node's own errors carry their code in the header, `TypeError
      // [ERR_INVALID_ARG_TYPE]: …`, and say so through a `toString` of their
      // own; everything else is headed the way V8 heads it.
      header =
        typeof error?.code === "string" &&
        error.code.startsWith("ERR_") &&
        typeof error.toString === "function" &&
        error.toString !== Error.prototype.toString
          ? String(error.toString())
          : Error.prototype.toString.call(error);
    } catch {
      header = "<error>";
    }
    if (trace.length === 0) return header;
    let out = header;
    for (const site of trace) {
      let frame = null;
      try {
        frame = mapped(site);
      } catch {
        // A map that cannot answer leaves the frame as V8 wrote it.
      }
      out += `\n    at ${frame ?? String(site)}`;
    }
    return out;
  };

  return (filename, output) => {
    const at = output.lastIndexOf(INLINE_MAP);
    if (at === -1) return output;
    const end = output.indexOf("\n", at + INLINE_MAP.length);
    maps.set(filename, output.slice(at + INLINE_MAP.length, end === -1 ? output.length : end));
    return output.slice(0, at + 1);
  };
}

/** A map's source as Node prints it: a path when it is a file, resolved against the module. */
function originalPath(source, fileName) {
  if (source.startsWith("file:")) {
    try {
      return fileURLToPath(source);
    } catch {
      return source;
    }
  }
  if (path.isAbsolute(source) || /^[a-z]+:/i.test(source)) return source;
  try {
    return path.resolve(path.dirname(fileURLToPath(fileName)), source);
  } catch {
    return source;
  }
}
