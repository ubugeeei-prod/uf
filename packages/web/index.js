// @flow
//
// `@uniflowed/web`: the primitives a page is built from.
//
// The small number of things every web page needs and nearly every project
// reimplements: an image that does not shift the layout, a date that survives
// hydration, a live region a screen reader actually announces, a cookie that
// reads the same on both sides.
//
// Each is here because the naive version is wrong in a way that only shows up
// later — a font preload without `crossOrigin` that silently downloads twice, a
// `toLocaleString` that makes the server and the browser disagree, a live
// region rendered at the same moment as its text and therefore never read. The
// per-component documentation says which mistake it exists to prevent.
//
// # How the package is laid out
//
// Five modules beside this one, each named after the mistake it prevents
// rather than after its place in a build:
//
// - `media.js` — the elements that load bytes, and the layout shifts and
//   double downloads they cause.
// - `time.js` — an instant, rendered the same way on a server and in a
//   browser.
// - `regions.js` — the elements that exist for a screen reader: landmarks,
//   live regions, and the link that reaches them.
// - `cookie.js` — one value with two readers, and no `node:async_hooks` in a
//   browser bundle.
// - `head.js` — what a component decides to put in `<head>` while it runs.
//
// They sit here, not under an `internal/`, and each has its own subpath. The
// directory is the table of contents: a reader who opens the package sees five
// subjects and can guess which file holds the thing they came for. Nothing in
// them needs hiding — they contain the same components this file re-exports —
// and calling them internal while exporting every one of their names would
// have been a claim the package does not keep. `internal/` is for a module
// consumers must *not* reach, and this package has none.
//
// This file stays a barrel because five modules genuinely need one. That is
// aggregation, not indirection: `import { Image, Time } from "@uniflowed/web"`
// is one import for a page that wants both, and the subpaths are there for a
// bundle that wants exactly one.
//
// Routing is not here, and not re-exported either. `Link`, `useRoute` and
// `useRouter` are `@uniflowed/router`'s, and passing them through this package
// would give each of them two import paths, two places to document, and two
// places to be wrong. It would also put this package's `Page` — a `<main>`
// landmark — into the same import as the router's `PageProps`, which is a
// route module's props and an entirely different idea.
//
// One name, one home. Import routing from the router.

export type { Loading, Source } from "./media.js";
export { Font, Image, Picture } from "./media.js";

export type { TimeFormat } from "./time.js";
export { Time, relative } from "./time.js";

export { Announcer, Layout, Page, SkipLink } from "./regions.js";

export type { Head, Link as HeadLink, Meta } from "./head.js";
export { useHead } from "./head.js";

export type { CookieOptions } from "./cookie.js";
export { useCookie } from "./cookie.js";
