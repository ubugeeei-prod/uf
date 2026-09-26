// @flow
//
// `@uniflowed/std/path`: Go's `path/filepath`, for runtime-agnostic slash paths.
//
// JavaScript has URL paths, package subpaths and route paths everywhere, but no
// host-independent path helpers for them. Node's `path` module is tied to Node
// and has platform modes; this module deliberately is not. It treats `/` as the
// separator on every runtime and performs lexical cleanup only: it never asks a
// file system whether a segment exists or whether a symlink changes the answer.
//
// That makes it the right primitive for package exports, generated routes,
// manifest entries and virtual file systems. Host file-system adapters can put
// their own OS semantics at the boundary where the host is already known.

/** Whether `path` starts at the slash-path root. */
export function isAbsolute(path: string): boolean {
  return path.startsWith("/");
}

/**
 * Clean a slash path.
 *
 * Duplicate separators and `.` segments are removed. `..` removes the previous
 * segment when there is one; leading `..` is preserved on relative paths and
 * clamped at the root on absolute paths. Empty relative paths clean to `"."`.
 */
export function normalize(path: string): string {
  const absolute = isAbsolute(path);
  const parts = path.split("/");
  const stack = [];

  for (const part of parts) {
    if (part === "" || part === ".") {
      continue;
    }
    if (part === "..") {
      if (stack.length > 0 && stack[stack.length - 1] !== "..") {
        stack.pop();
      } else if (!absolute) {
        stack.push("..");
      }
      continue;
    }
    stack.push(part);
  }

  if (absolute) {
    return stack.length === 0 ? "/" : `/${stack.join("/")}`;
  }
  return stack.length === 0 ? "." : stack.join("/");
}

/** Join path fragments with `/` and clean the result. */
export function join(...parts: Array<string>): string {
  if (parts.length === 0) {
    return ".";
  }
  return normalize(parts.join("/"));
}

/** Return the directory containing `path`, after lexical cleanup. */
export function dirname(path: string): string {
  const cleaned = normalize(path);
  if (cleaned === "." || cleaned === "/") {
    return cleaned;
  }
  const slash = cleaned.lastIndexOf("/");
  if (slash < 0) {
    return ".";
  }
  if (slash === 0) {
    return "/";
  }
  return cleaned.slice(0, slash);
}

/** Return the final path segment, after lexical cleanup. */
export function basename(path: string): string {
  const cleaned = normalize(path);
  if (cleaned === "/" || cleaned === ".") {
    return cleaned;
  }
  const slash = cleaned.lastIndexOf("/");
  return slash < 0 ? cleaned : cleaned.slice(slash + 1);
}

/**
 * Return the final extension of the final segment.
 *
 * This follows Go's `path.Ext`: the extension begins at the final dot in the
 * final segment, so `.env` has extension `.env`.
 */
export function extname(path: string): string {
  const base = basename(path);
  if (base === "." || base === "/" || base === "..") {
    return "";
  }
  const dot = base.lastIndexOf(".");
  return dot < 0 ? "" : base.slice(dot);
}

/**
 * Return a relative slash path from `from` to `to`.
 *
 * Both inputs must either be absolute or relative. The result is `"."` when
 * the cleaned paths name the same location.
 */
export function relative(from: string, to: string): string {
  const fromAbsolute = isAbsolute(from);
  const toAbsolute = isAbsolute(to);
  if (fromAbsolute !== toAbsolute) {
    throw new RangeError("@uniflowed/std/path: cannot make a path relative across roots");
  }

  const fromParts = components(normalize(from));
  const toParts = components(normalize(to));
  let common = 0;
  while (
    common < fromParts.length &&
    common < toParts.length &&
    fromParts[common] === toParts[common]
  ) {
    common += 1;
  }
  if (fromParts[common] === "..") {
    throw new RangeError(
      "@uniflowed/std/path: cannot make a path relative from an unanchored parent",
    );
  }

  const upward = Array.from({ length: fromParts.length - common }, () => "..");
  const downward = toParts.slice(common);
  const result = upward.concat(downward).join("/");
  return result === "" ? "." : result;
}

/** Path components without root or current-directory sentinels. */
function components(path: string): Array<string> {
  if (path === "." || path === "/") {
    return [];
  }
  const trimmed = path.startsWith("/") ? path.slice(1) : path;
  return trimmed === "" ? [] : trimmed.split("/");
}
