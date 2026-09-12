// @noflow
//
// Plain JavaScript: a stand-in, and there is nothing in it to type.
//
// The `node:` modules `@uniflowed/host/module-mocks` can import in a browser
// bundle.
//
// `../module-mocks.js` is reachable from `@uniflowed/test`, which a module
// with an in-source test block imports. The block itself is compiled away by
// `uf build` (`import.meta.uf.test` becomes `void 0`) and the import is shaken
// out after it — but the bundler resolves the graph before it shakes. Every
// `node:` import this module carries therefore needs a browser answer, or every
// project that writes an in-source test gets a warning about a module that was
// never going to run.
//
// So this file exists to be resolved and never called. `registerHooks` is the
// only browser-reachable binding the public API can touch; the Bun stand-in
// helpers use `fs`, `os` and `path`, but they are a host-only escape hatch and
// should fail with a sentence if a page ever asks for them.
//
// Mapped in by `package.json`'s `browser` field, which Node ignores and every
// bundler honours, so the host's own behaviour is untouched.

function unavailable(what) {
  throw new Error(
    `${what} is not available in a browser: module mocking is implemented by the JavaScript host, not by the page running the test`,
  );
}

/** Node's synchronous module hooks, which a page does not have. */
export function registerHooks() {
  unavailable("module interception");
}

/** A filesystem write, which only the host can perform. */
export function mkdtempSync(_prefix) {
  unavailable("the filesystem");
}

/** @see mkdtempSync */
export function writeFileSync(_path, _contents) {
  unavailable("the filesystem");
}

/** A process temp directory, which only the host owns. */
export function tmpdir() {
  unavailable("the operating system temp directory");
}

export const sep = "/";

export function join(...parts) {
  return normalize(parts.filter((part) => part !== "").join("/"));
}

function normalize(input) {
  const absolute = input.startsWith("/");
  const out = [];
  for (const part of input.split("/")) {
    if (part === "" || part === ".") continue;
    if (part === ".." && out.length > 0 && out[out.length - 1] !== "..") out.pop();
    else out.push(part);
  }
  const joined = out.join("/");
  return absolute ? `/${joined}` : joined === "" ? "." : joined;
}

export default {
  join,
  mkdtempSync,
  registerHooks,
  sep,
  tmpdir,
  writeFileSync,
};
