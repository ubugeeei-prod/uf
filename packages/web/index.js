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
// Routing is not here, and not re-exported either. `Link`, `useRoute` and
// `useRouter` are `@uniflowed/router`'s, and passing them through this package
// would give each of them two import paths, two places to document, and two
// places to be wrong. It would also put this package's `Page` — a `<main>`
// landmark — into the same import as the router's `PageProps`, which is a
// route module's props and an entirely different idea.
//
// One name, one home. Import routing from the router.

export type { Loading, Source } from "./internal/media.js";
export { Font, Image, Picture } from "./internal/media.js";

export type { TimeFormat } from "./internal/time.js";
export { Time, relative } from "./internal/time.js";

export { Announcer, Layout, Page, SkipLink } from "./internal/regions.js";

export type { Head, Link as HeadLink, Meta } from "./internal/head.js";
export { useHead } from "./internal/head.js";

export type { CookieOptions } from "./internal/cookie.js";
export { useCookie } from "./internal/cookie.js";
