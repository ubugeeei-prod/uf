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
// So this file exists to be resolved and to make feature detection answer
// "not here". `registerHooks` is intentionally absent; the Bun stand-in helpers
// use `fs`, `os` and `path`, but they are a host-only escape hatch and should
// fail with a sentence if a page ever asks for them.
//
// Mapped in by `package.json`'s `browser` field, which Node ignores and every
// bundler honours, so the host's own behaviour is untouched.

function unavailable(what) {
  throw new Error(
    `${what} is not available in a browser: module mocking is implemented by the JavaScript host, not by the page running the test`,
  );
}

/** Node's synchronous module hooks, which a page does not have. */
export const registerHooks = undefined;

/** Process spawning, which only the host can perform. */
export function spawn(_command, _args, _options) {
  unavailable("process spawning");
}

/** A readline interface, which only the host transform service uses. */
export function createInterface(_options) {
  unavailable("readline");
}

/** Filesystem permission constants used by the host transform service. */
export const constants = { X_OK: 0 };

/** Filesystem access checks, which only the host can perform. */
export function accessSync(_path, _mode) {
  unavailable("the filesystem");
}

/** Filesystem stat checks, which only the host can perform. */
export function statSync(_path) {
  unavailable("the filesystem");
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

/** Convert a browser-resolved file URL the same way the host-side resolver does. */
export function fileURLToPath(url) {
  const parsed = url instanceof URL ? url : new URL(String(url));
  if (parsed.protocol !== "file:" || (parsed.host !== "" && parsed.host !== "localhost")) {
    unavailable("file URL conversion");
  }
  return decodeURIComponent(parsed.pathname);
}

/** @see fileURLToPath */
export function pathToFileURL(file) {
  const path = String(file);
  const encoded = path.split("/").map(encodeURIComponent).join("/");
  return new URL(`file://${encoded.startsWith("/") ? "" : "/"}${encoded}`);
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
  accessSync,
  constants,
  createInterface,
  join,
  fileURLToPath,
  mkdtempSync,
  pathToFileURL,
  registerHooks,
  sep,
  spawn,
  statSync,
  tmpdir,
  writeFileSync,
};
