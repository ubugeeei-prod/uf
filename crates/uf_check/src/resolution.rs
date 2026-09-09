//! Which file a package specifier means, and under what conditions.
//!
//! A dependency's `package.json` does not name one file per subpath. It names
//! one *per condition* — `import` against `require`, `browser` against `node`,
//! `bun` and `deno` against neither — and the resolver picks by asking which
//! conditions are true of the thing doing the resolving. So a type checker has
//! to answer a question a runtime never has to: **which of those files is the
//! one whose types the project should be held to.**
//!
//! These are constants rather than configuration for the same reason the rest
//! of the checker's options are: a unified toolchain decides, and a project
//! that could set them would be a project whose `uf check` result nobody else
//! could reproduce. But they are constants with a cost, and the cost is
//! written down below rather than left for someone to discover from a wrong
//! answer.
//!
//! # Four kinds of condition, and only two of them are uf's to set
//!
//! **Module system.** `import` against `require`. uf sets `import`: every
//! package here is `"type": "module"`, and the transform, the dev server and
//! the bundler all reach a package through `import`. This one is a property of
//! how uf loads modules, so uf knows it.
//!
//! **Language.** `flow`. uf sets it, and for a while it did not — see below.
//!
//! **Host.** `node`, `bun`, `deno`, `browser`, `workerd`. uf sets **none of
//! them**, and that is a decision rather than an omission. See "The host
//! conditions are deliberately unset".
//!
//! **Graph.** `react-server`, which selects the build of React with no
//! `useState` in it. uf sets it nowhere, including here. `docs/architecture.md`
//! says why that is a second module graph and not a second import, and
//! ubugeeei-prod/uf#519 is the size of it; a checker that set the condition
//! without there being such a graph would type a module the build never
//! produces.
//!
//! # `flow` is set because uf is the consumer it was written for
//!
//! `uf create lib` scaffolds a library whose `exports` is exactly this:
//!
//! ```json
//! "exports": { ".": { "flow": "./index.js", "default": "./dist/index.js" } }
//! ```
//!
//! `uf_cli::commands::build::library` argues that split at length, and the
//! sentence that matters here is its own: `"flow"` names the language of the
//! file behind it, "so a consumer who has a Flow transform and is not uf can
//! select the same condition".
//!
//! uf is a consumer that has a Flow transform, and it did not select the
//! condition. With `import` alone `flow` matched nothing, `default` won, and
//! `uf check` resolved a dependency on a uf-built library to
//! `./dist/index.js` — the *compiled* build, every type erased, frequently not
//! even in the batch. The one consumer that whole design says it cares most
//! about, "another uf project", got precisely the outcome it was written to
//! prevent.
//!
//! Which of the two a given manifest yields is still the manifest's own
//! choice, because the order that decides is the order the *manifest* writes
//! its conditions in, not the order of this list. A package that writes
//! `import` above `flow` gets its `import` target here, exactly as it would
//! from Node. This list says which keys uf answers to; it does not rank them.
//!
//! # The host conditions are deliberately unset
//!
//! `docs/hosts.md` is the table this turns on: a uf application runs on Node,
//! on Bun, and — experimentally — on Deno, and `CapabilityJsHostConfig`
//! defaults to accepting all three. One `uf check` produces one answer. So the
//! conditions it resolves under have to be true of *every* host uf claims, or
//! the check is a check of one host wearing the name of the toolchain.
//!
//! Neither alternative survives that.
//!
//! Setting the *default* host's condition — `node`, since that is
//! `CapabilityJsHost`'s default — makes `uf check` Node's checker and says so
//! nowhere. Red line 6 is that core must not depend on a specific JavaScript
//! runtime, and a type checker that resolves `node` is core depending on one.
//!
//! Setting *all three* is worse, and it is worse in a way that is easy to miss.
//! Conditions are matched in the order the manifest writes them, so a
//! dependency that lists `{ "bun": …, "node": … }` would hand uf its Bun
//! branch and one that lists them the other way round would hand uf its Node
//! branch — the *dependency's* key order deciding which host uf typed the
//! project against. That is not any host's answer. It is a fourth answer, and
//! it belongs to whoever wrote the dependency.
//!
//! # What that costs, measured rather than asserted
//!
//! Three shapes resolve to something other than what the project's host loads,
//! and naming them is the point of writing this down. Each was confirmed
//! against this resolver, and the runtime column against Node 24 and Bun 1.3 on
//! a real install rather than against the specification:
//!
//! | The dependency's `exports` for `.` | uf types | Node loads | Bun loads |
//! | --- | --- | --- | --- |
//! | `{ "bun": "./b.js", "import": "./i.js" }` | `./i.js` | `./i.js` | `./b.js` |
//! | `{ "deno": …, "bun": …, "node": … }` | nothing | its `node` | its `bun` |
//! | `{ "node": …, "browser": … }` | nothing | its `node` | its `node` |
//!
//! Row one is the Node lean stated exactly: uf does not resolve `node`, but the
//! branch it does resolve is the one a package writes *for* Node, because
//! `bun` and `deno` are published as overrides on top of a portable default.
//! Rows two and three are the quieter half — a package that publishes only
//! host branches resolves to nothing at all here, and lands in
//! [`crate::CheckReport::untyped_modules`] beside packages that are merely not
//! installed. Two different problems, one report.
//!
//! Closing that is a decision about what `uf check` *is* — one check of a
//! portable graph, or one check per host — and it is not this module's to take
//! alone. ubugeeei-prod/uf#735.

/// The `exports` conditions a package subpath is resolved under.
///
/// Upstream honours `default` on top of whatever is listed, so a package with
/// no conditions at all still resolves; and it matches in the order the
/// *manifest* writes its keys, so this is a set uf answers to rather than a
/// ranking uf imposes.
///
/// Listing `require` here would be a bug, not a kindness: a dual package that
/// happened to write `require` above `import` would hand uf its CommonJS
/// entry, and the checker would be typing a file the runtime never loads.
///
/// See this module's header for why `node`, `bun`, `deno`, `browser` and
/// `react-server` are absent.
pub const EXPORT_CONDITIONS: &[&str] = &["flow", "import"];

/// The `package.json` fields a package with no `exports` map is entered
/// through.
///
/// Flow's own default, and Node's. Stated rather than left to Flow's
/// `Options::default()`, whose list is empty — with an empty list `main` is
/// never read, and a package that has no `exports` map would resolve to
/// nothing at all.
pub const MAIN_FIELDS: &[&str] = &["main"];

#[cfg(test)]
mod tests {
    use super::*;

    /// The language condition is answered, so a uf library's consumer gets its
    /// source.
    ///
    /// The behaviour test is
    /// `upstream::packages::tests::a_uf_library_resolves_to_its_flow_source_and_not_its_build`;
    /// this one is here so that the constant cannot be emptied without a
    /// failure in the module that documents why it is not empty.
    #[test]
    fn the_language_condition_is_answered() {
        assert!(EXPORT_CONDITIONS.contains(&"flow"));
        assert!(EXPORT_CONDITIONS.contains(&"import"));
    }

    /// No condition here names a JavaScript host.
    ///
    /// Red line 6, as a test rather than as a paragraph: adding one is a
    /// decision about what `uf check` checks (ubugeeei-prod/uf#735), and it
    /// should not be reachable by editing a list.
    #[test]
    fn no_condition_names_a_host_or_a_graph() {
        for named in ["node", "bun", "deno", "browser", "workerd", "react-server"] {
            assert!(
                !EXPORT_CONDITIONS.contains(&named),
                "`{named}` names a host or a graph; see this module's header and \
                 ubugeeei-prod/uf#735"
            );
        }
    }

    /// `require` would resolve the module a uf project never loads.
    #[test]
    fn the_commonjs_condition_is_not_answered() {
        assert!(!EXPORT_CONDITIONS.contains(&"require"));
    }

    /// Without `main`, a package that has no `exports` map resolves to nothing.
    #[test]
    fn a_package_with_no_exports_map_is_still_enterable() {
        assert_eq!(MAIN_FIELDS, ["main"]);
    }
}
