//! The DOM, BOM and Node globals, which Flow keeps outside `lib/`.
//!
//! Flow's `lib/` holds `core.js` and `react.js` and nothing else. Everything a
//! browser or a Node process provides — `document`, `HTMLElement`, `Response`,
//! `process`, `URL` — lives in `evals/flow-typed/environment/`, which Flow
//! itself loads through a `.flowconfig`'s `[libs]` section.
//!
//! uf has no `.flowconfig` and does not want one, so the environments it
//! targets are merged unconditionally. That is not a shortcut around
//! configuration: a uf project targets the web and a Capability JS Host, and
//! there is no uf project for which `document` should be an unresolvable name.
//! `uf check` reported 193 `cannot-resolve-name` errors against this
//! repository's own packages before this existed, 41 of them for `Response`.
//!
//! # Which ones
//!
//! Everything that describes a platform uf targets, and nothing that describes
//! one it does not. `fbt` is Meta's internationalisation library and `jsx` is
//! the intrinsic element table, which `core.js` and `react.js` already cover
//! between them; both are left out.

/// One environment library definition, as Flow names it.
pub(super) type Environment = (&'static str, &'static str);

macro_rules! environment {
    ($file:literal) => {
        (
            $file,
            include_str!(concat!(
                "../../../../upstream/flow/evals/flow-typed/environment/",
                $file
            )),
        )
    };
}

/// The environments merged into every `uf check`.
///
/// Order matters the way it matters for Flow's own libraries: a later
/// definition shadows an earlier one, and the caller folds from the front after
/// reversing. `dom` before `html` before `bom` is the order Flow's own
/// `.flowconfig` files list them in.
pub(super) static ENVIRONMENTS: &[Environment] = &[
    environment!("dom.js"),
    environment!("html.js"),
    environment!("svg.js"),
    environment!("cssom.js"),
    environment!("geometry.js"),
    environment!("web-animations.js"),
    environment!("bom.js"),
    environment!("streams.js"),
    environment!("indexeddb.js"),
    environment!("serviceworkers.js"),
    environment!("webassembly.js"),
    environment!("intl.js"),
    // `Intl` with `RelativeTimeFormat`. Before `uf-intl.js`, whose namespace
    // types are looked up on the last `Intl` declared. See the file's header.
    (
        "intl-relative-time.js",
        include_str!("../../libdefs/intl-relative-time.js"),
    ),
    environment!("node.js"),
    // Node's `url`, `fs` and `async_hooks`, after `node.js` so that each
    // replaces the module of the same name, and whole for the same reason as
    // `web-crypto.js`. See each file's header and #1451.
    ("node-url.js", include_str!("../../libdefs/node-url.js")),
    ("node-fs.js", include_str!("../../libdefs/node-fs.js")),
    (
        "node-async-hooks.js",
        include_str!("../../libdefs/node-async-hooks.js"),
    ),
    // TypeScript declarations name Intl's option dictionaries through the
    // `Intl` namespace, while Flow's libdefs expose the older `$` aliases.
    // This bridges those spellings for packages translated from `.d.ts`. See
    // ubugeeei-prod/uf#1089.
    ("uf-intl.js", include_str!("../../libdefs/intl.js")),
    // uf's own, and the only one not from the submodule. Vite ships its client
    // types as TypeScript and Flow's `core.js` types `import.meta` as
    // `{ [string]: unknown, url?: string, ... }`, so `import.meta.env`,
    // `.hot` and `.glob` were type errors in every uf project. Last in the
    // list because a later definition shadows an earlier one, which is how it
    // replaces `core.js`'s `Import$Meta`.
    (
        "vite-client.js",
        include_str!("../../libdefs/vite-client.js"),
    ),
    // The other half of what Vite resolves: a stylesheet, an icon, a `?raw`.
    // Flow has no wildcard module name, so each *shape* is declared under a
    // name of its own and `ProjectModules::resolve` maps a specifier onto one.
    (
        "vite-assets.js",
        include_str!("../../libdefs/vite-assets.js"),
    ),
    // WHATWG Streams, of which the vendored `streams.js` has the shapes and
    // not the generics: `ReadableStream` there takes no type argument, so
    // `ReadableStream<Uint8Array>` — what a module that streams a response
    // writes, and what every runtime uf targets implements — was "not a
    // polymorphic type". After `streams.js`, `bom.js` and `dom.js` so that it
    // shadows all three, and with a default on every parameter so that their
    // own unparameterised uses keep meaning what they mean.
    (
        "web-streams.js",
        include_str!("../../libdefs/web-streams.js"),
    ),
    // Web Crypto, of which the vendored `bom.js` declares `digest` and no
    // more — no `sign`, no `importKey`, and no `CryptoKey` to annotate a key
    // with. A module that signs reported errors for calls every runtime uf
    // targets implements. After `bom.js` so that its `Crypto` is the one that
    // shadows, and whole rather than partial for the same reason. See #619.
    ("web-crypto.js", include_str!("../../libdefs/web-crypto.js")),
    // Fetch's `Response`, `Request`, `Headers` and `URLSearchParams`, and
    // `AbortSignal`, each redeclared with what the vendored `bom.js` and
    // `dom.js` leave out or put on the wrong side. After `bom.js` so that it shadows, and
    // whole for the same reason as `web-crypto.js`. See #1451.
    ("fetch.js", include_str!("../../libdefs/fetch.js")),
];
