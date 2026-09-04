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
    environment!("node.js"),
];
