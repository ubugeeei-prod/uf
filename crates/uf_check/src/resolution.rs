//! Which file a package specifier means, and under what conditions.
//!
//! A dependency's `package.json` does not name one file per subpath. It names
//! one per condition -- `import` against `require`, `browser` against `node`,
//! `bun` and `deno` against neither -- and the resolver picks by asking which
//! conditions are true of the thing doing the resolving.
//!
//! `uf check` answers two condition kinds:
//!
//! * `flow`, because uf is a Flow toolchain and `uf build --lib` publishes
//!   Flow sources behind that condition.
//! * `import`, because uf projects load packages as ES modules, not CommonJS.
//!
//! It deliberately answers no JavaScript host condition. One type check
//! produces one graph, while uf claims Node, Bun, and experimental Deno hosts;
//! choosing `node`, `bun`, `deno`, or `browser` here would make the checker a
//! checker for that host rather than for the portable graph. The host-specific
//! miss is reported separately so the cost is visible.

/// The `exports` conditions a package subpath is resolved under.
///
/// Upstream honours `default` in addition to this list, and it matches in the
/// order the manifest writes its keys. This is a set of keys uf answers to, not
/// a priority order uf imposes.
pub const EXPORT_CONDITIONS: &[&str] = &["flow", "import"];

/// The `package.json` fields a package with no `exports` map is entered
/// through.
///
/// Prefer `module` to `main` because uf checks the ESM graph. A legacy dual
/// package with no `exports` map commonly leaves its CommonJS entry in `main`
/// and its ESM entry in `module`; checking the CommonJS file is the same kind
/// of wrong graph as choosing the `require` condition above.
pub const MAIN_FIELDS: &[&str] = &["module", "main"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_language_condition_is_answered() {
        assert!(EXPORT_CONDITIONS.contains(&"flow"));
        assert!(EXPORT_CONDITIONS.contains(&"import"));
    }

    #[test]
    fn no_condition_names_a_host_or_graph() {
        for named in ["node", "bun", "deno", "browser", "workerd", "react-server"] {
            assert!(
                !EXPORT_CONDITIONS.contains(&named),
                "`{named}` names a host or graph; changing that is a decision \
                 about what one `uf check` checks"
            );
        }
    }

    #[test]
    fn the_commonjs_condition_is_not_answered() {
        assert!(!EXPORT_CONDITIONS.contains(&"require"));
    }

    #[test]
    fn a_package_with_no_exports_map_is_still_enterable_as_esm() {
        assert_eq!(MAIN_FIELDS, ["module", "main"]);
    }
}
