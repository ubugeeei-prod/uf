/**
 * @fileoverview The files Vite resolves that are not JavaScript, typed for Flow.
 *
 * A component that imports a stylesheet and an icon is the ordinary case, and
 * before this file it was two `any`s:
 *
 *     › these imports are typed as any; uf resolved no module for them
 *       - ./a.module.css
 *       - ./logo.svg?url
 *
 * A TypeScript project gets these from `vite/client`'s ambient module
 * declarations. Flow has no `/// <reference types>` and no wildcard module
 * name, so a project cannot declare `*.css` for itself even if it wanted to —
 * the checker has to be told, and uf is the checker.
 *
 * # Why the names look like this
 *
 * Flow's `declare module` takes a string literal, so there is no
 * `declare module "*.css"`. Instead each *shape* gets a name of its own here,
 * and `ProjectModules::resolve` maps a specifier onto one: `./a.module.css`
 * becomes `uf$asset$css_module`, `./logo.svg?url` becomes `uf$asset$url`. The
 * mapping is in Rust because it is a decision about a path; what the answer
 * *is* stays here, as declarations somebody can read and edit.
 *
 * The `uf$` prefix is not decoration. These are not importable names — nothing
 * may write `import x from "uf$asset$css"` — and a name with a `$` in it is
 * one nobody types by accident. `Import$Meta` in Flow's own `core.js` is the
 * same convention.
 *
 * # What Vite decides, and what this follows
 *
 * The query wins over the extension: `./a.css?raw` is a string of CSS source,
 * not a stylesheet. That is Vite's rule and it is why `?raw`, `?url`,
 * `?inline`, `?worker` and `?sharedworker` are separate shapes rather than
 * options on one.
 *
 * Everything else — an image, a font, a video, a `.pdf` — is a URL, because
 * that is what the build turns it into: a hashed file in `dist/assets` and a
 * string pointing at it. `import logo from "./logo.svg"` is a `string`, and a
 * project that renders it as anything but a `src` is wrong in a way Flow can
 * now say.
 */

/**
 * A stylesheet imported for its effect: `import "./a.css"`.
 *
 * The default export is the CSS source, which Vite provides and almost nobody
 * reads — the import exists so the build collects the file. It is typed
 * anyway, because a default export that exists and is `any` is the thing this
 * file is here to stop.
 */
declare module "uf$asset$css" {
  declare export default string;
}

/**
 * A CSS module: `import styles from "./a.module.css"`.
 *
 * The class map, not the source. `styles.button` is a `string` — the generated
 * class name — and a class the stylesheet does not define is `void` rather
 * than an error, because Flow cannot see inside the stylesheet and pretending
 * otherwise would be a lie with a caret on it.
 */
declare module "uf$asset$css_module" {
  declare export default { readonly [className: string]: string | void };
}

/**
 * An asset imported for its URL: an image, a font, a video, `?url`, `?inline`.
 *
 * `import logo from "./logo.svg"` is the hashed path the build wrote, so it is
 * a `string`. `?inline` is a `data:` URI and `?url` is a path, and they are
 * one shape here because a caller uses both the same way.
 */
declare module "uf$asset$url" {
  declare export default string;
}

/**
 * A file imported as text: `import source from "./readme.md?raw"`.
 *
 * The bytes of the file, decoded. The same file without `?raw` would be a URL,
 * which is the reason the query decides the shape.
 */
declare module "uf$asset$raw" {
  declare export default string;
}

/**
 * What `?worker` gives back.
 *
 * A constructor taking no URL: Vite compiled the module separately and baked
 * the URL and the bundling options in, so `new Worker()` is the whole call.
 * `typeof Worker` would promise a constructor that takes a URL, which this one
 * does not, and `Class<Worker>` would promise the same.
 *
 * An `interface` with a `new` signature, which is how `core.js` spells a
 * constructor type — see `Int8ArrayConstructor`. The `$` in the name is the
 * convention for something declared here that nobody writes: it is a type, not
 * a value, and no module exports it.
 */
interface Uf$WorkerConstructor {
  new (options?: { readonly name?: string }): Worker;
}

/** The same, for `?sharedworker`. */
interface Uf$SharedWorkerConstructor {
  new (options?: { readonly name?: string }): SharedWorker;
}

/** A module imported as a worker: `import Worker from "./w.js?worker"`. */
declare module "uf$asset$worker" {
  declare export default Uf$WorkerConstructor;
}

/** A module imported as a shared worker. */
declare module "uf$asset$shared_worker" {
  declare export default Uf$SharedWorkerConstructor;
}
