//! The modules a set of files reaches, by the batch's own resolution rules.
//!
//! # Why a batch needs this at all
//!
//! [`super::check_sources`] is a function of the sources it is handed and
//! nothing else: an import resolves to a file in the batch or it resolves to
//! nothing typed. That is the right contract — it is what makes a check
//! reproducible and what keeps the file system out of the checker — but it
//! pushes a question onto the caller that the caller has no way to answer:
//! *which* sources does checking these files require?
//!
//! Answering it by hand is how `uf check <one file>` came to check that file
//! against nothing. `import type { Control } from "@uniflowed/form"` in a
//! batch of one file is an `any`-typed value, so every annotation written
//! against it is neither right nor wrong — it is unread. Answering it with
//! "the whole project" is correct and costs the whole project's inference for
//! a question about one file. This module answers it with the closure: the
//! seeds, plus every module they reach, transitively.
//!
//! # It resolves with the checker's rules, not a second set of them
//!
//! Every edge here goes through [`super::resolve`] and [`super::packages`] —
//! the same [`ModuleIndex`] and [`WorkspacePackages`] that
//! [`super::project::ProjectModules`] resolves through while it types a file.
//! A closure that resolved by its own rules would be a second answer to *what
//! does this specifier mean*, and the batch it assembled would be missing
//! exactly the files the checker then went looking for.
//!
//! Two consequences of using the checker's rules are worth naming, because
//! both look like bugs from outside:
//!
//! * A file that does not parse, or that says `@noflow`, contributes no
//!   imports. That is not this module being careful — it is
//!   [`super::project::ProjectModules::facts`]'s own rule, and it has to be:
//!   the checker resolves nothing through such a file, so a batch holding what
//!   it imports would hold files nothing can reach.
//! * A package's **manifest** is pulled in beside the file it publishes. The
//!   manifest is where the name comes from, so a batch with
//!   `packages/form/index.js` in it and no `packages/form/package.json`
//!   resolves `@uniflowed/form` to nothing, having been handed the answer.
//!
//! # What is left over
//!
//! [`Closure::unresolved`] is every specifier no source answered *and* Flow's
//! own library definitions do not describe. The second half of that matters as
//! much as the first, because it is what the caller is expected to do about it:
//! `uf check` reads `node_modules` for a bare specifier left here and adds what
//! it finds to the batch. A package Flow describes is Flow's to describe: the
//! walk must not add `react`, or a package a project's `flow-typed/` declares,
//! because doing so would replace the declaration with whatever JavaScript
//! happens to be installed and make the answer worse rather than better.

use std::collections::BTreeSet;

use compact_str::{CompactString, ToCompactString};
use flow_common::flow_import_specifier::FlowImportSpecifier;
use flow_common::options::Options;
use flow_parser::file_key::{FileKey, FileKeyInner};

use super::packages::{PackageFile, WorkspacePackages, is_manifest};
use super::parse;
use super::resolve::{self, ModuleIndex};
use crate::{Source, UnresolvedImport};

/// What a set of seeds reaches.
pub(super) struct Closure {
    /// The sources the seeds reach, as indices into what was searched, in
    /// ascending order.
    ///
    /// Ascending rather than in discovery order so that the batch is a
    /// function of the sources and the seeds alone: discovery order depends on
    /// which seed was walked first, and two orders would be two cache keys for
    /// the same check.
    pub(super) reached: Vec<usize>,
    /// Specifiers nothing searched answered and no libdef declares, each
    /// beside the file that imported it, sorted and de-duplicated.
    ///
    /// The importer is kept because a bare specifier means whatever Node's
    /// climb from *that file* finds, and a caller that goes looking for the
    /// package has to climb from the same place. Two files importing one name
    /// are two entries for that reason, and not a redundancy.
    pub(super) unresolved: Vec<UnresolvedImport>,
}

/// The closure of `seeds` over `available`.
///
/// A seed naming nothing in `available` is skipped rather than reported: the
/// caller chose both lists, and a seed it did not also supply is its own
/// mistake to notice, not a property of the module graph.
///
/// `declared` answers whether Flow's library definitions describe a specifier.
/// It is a parameter rather than something this module asks for itself so that
/// the walk stays a function of its inputs — and so that a test of the graph
/// does not have to merge the builtins to run.
pub(super) fn closure(
    seeds: &[&str],
    available: &[Source<'_>],
    options: &Options,
    declared: &dyn Fn(&str) -> bool,
) -> Closure {
    let index = ModuleIndex::new(available.iter().map(|source| source.path));
    let packages = WorkspacePackages::new(available, options);

    let mut seen = vec![false; available.len()];
    let mut frontier: Vec<usize> = Vec::new();
    let mut unresolved: BTreeSet<UnresolvedImport> = BTreeSet::new();

    for seed in seeds {
        if let Some(target) = index.index_of(seed) {
            reach(target, &mut seen, &mut frontier);
        }
    }

    while let Some(module) = frontier.pop() {
        let source = available[module];
        for specifier in requires(&source, options) {
            let declared = declared(&specifier);
            let relative = resolve::is_relative(&specifier);
            let resolved = if relative {
                index.resolve(source.path, &specifier)
            } else if declared {
                None
            } else {
                // The manifest first, and whether or not the file it names is
                // in the batch: a package that publishes a subpath this batch
                // does not hold still has to be able to answer for the ones it
                // does.
                if let Some(manifest) = packages
                    .manifest_of(source.path, &specifier)
                    .and_then(|path| index.index_of(path))
                {
                    reach(manifest, &mut seen, &mut frontier);
                }
                match packages.resolve(source.path, &specifier) {
                    Some(PackageFile::Exact(path)) => index.lookup(&path),
                    Some(PackageFile::Implied(base)) => index.resolve_file(&base),
                    None => None,
                }
            };
            match resolved {
                Some(target) => reach(target, &mut seen, &mut frontier),
                None if !declared => {
                    unresolved.insert(UnresolvedImport {
                        specifier,
                        importer: source.path.to_compact_string(),
                    });
                }
                None => {}
            }
        }
    }

    Closure {
        reached: (0..available.len()).filter(|index| seen[*index]).collect(),
        unresolved: unresolved.into_iter().collect(),
    }
}

/// Mark a module reached, and queue it if this is the first time.
fn reach(target: usize, seen: &mut [bool], frontier: &mut Vec<usize>) {
    if !seen[target] {
        seen[target] = true;
        frontier.push(target);
    }
}

/// Every module specifier `source` imports.
///
/// Read out of the file signature rather than the packed module, and dropped
/// entirely for a file that cannot contribute one, for the reasons
/// [`super::project::ProjectModules::facts`] gives — this is that function
/// with the signature packing left out, because a closure needs to know what a
/// file imports and never needs to know what it exports.
fn requires(source: &Source<'_>, options: &Options) -> Vec<CompactString> {
    // A manifest is JSON. Handing it to the Flow parser produces a syntax
    // error and no imports, which is the right answer by a slow route.
    if is_manifest(source.path) {
        return Vec::new();
    }
    let file_key = FileKey::new(FileKeyInner::SourceFile(source.path.to_owned()));
    let parsed = parse::parse_file(file_key, source.source, options, false);
    if !parsed.is_parseable() || !parsed.is_checked() {
        return Vec::new();
    }
    parsed
        .file_sig
        .require_loc_map()
        .keys()
        .map(|specifier| {
            let FlowImportSpecifier::Userland(userland) = specifier;
            userland.as_str().to_compact_string()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CheckLimits;
    use crate::upstream::options;

    /// Nothing is declared: these tests are about the graph, not the libdefs.
    fn undeclared(_specifier: &str) -> bool {
        false
    }

    fn reached<'a>(seeds: &[&str], available: &[Source<'a>]) -> Vec<&'a str> {
        let found = closure(
            seeds,
            available,
            &options::options(&CheckLimits::default()),
            &undeclared,
        );
        found
            .reached
            .into_iter()
            .map(|index| available[index].path)
            .collect()
    }

    fn unresolved(seeds: &[&str], available: &[Source<'_>]) -> Vec<String> {
        closure(
            seeds,
            available,
            &options::options(&CheckLimits::default()),
            &undeclared,
        )
        .unresolved
        .into_iter()
        .map(|left_over| format!("{} in {}", left_over.specifier, left_over.importer))
        .collect()
    }

    const FORM_MANIFEST: &str = r#"{
      "name": "@uniflowed/form",
      "exports": { ".": "./index.js", "./watch": "./watch.js" }
    }"#;

    #[test]
    fn a_seed_with_no_imports_reaches_only_itself() {
        let available = [
            Source::new("a.js", "export const a = 1;\n"),
            Source::new("b.js", "export const b = 2;\n"),
        ];

        assert_eq!(reached(&["a.js"], &available), ["a.js"]);
    }

    #[test]
    fn a_relative_import_is_followed_transitively() {
        let available = [
            Source::new("app.js", "import { b } from './b.js';\nexport { b };\n"),
            Source::new("b.js", "import { c } from './c.js';\nexport const b = c;\n"),
            Source::new("c.js", "export const c = 1;\n"),
            Source::new("unrelated.js", "export const d = 1;\n"),
        ];

        assert_eq!(reached(&["app.js"], &available), ["app.js", "b.js", "c.js"]);
    }

    #[test]
    fn a_package_import_reaches_the_file_and_the_manifest_that_named_it() {
        let available = [
            Source::new("app.js", "import { useForm } from '@uniflowed/form';\n"),
            Source::new("packages/form/index.js", "export const useForm = 1;\n"),
            Source::new("packages/form/package.json", FORM_MANIFEST),
            Source::new("packages/form/watch.js", "export const watch = 1;\n"),
        ];

        assert_eq!(
            reached(&["app.js"], &available),
            [
                "app.js",
                "packages/form/index.js",
                "packages/form/package.json"
            ]
        );
    }

    #[test]
    fn a_subpath_export_resolves_to_the_file_the_map_names() {
        let available = [
            Source::new("app.js", "import { watch } from '@uniflowed/form/watch';\n"),
            Source::new("packages/form/index.js", "export const useForm = 1;\n"),
            Source::new("packages/form/package.json", FORM_MANIFEST),
            Source::new("packages/form/watch.js", "export const watch = 1;\n"),
        ];

        assert_eq!(
            reached(&["app.js"], &available),
            [
                "app.js",
                "packages/form/package.json",
                "packages/form/watch.js"
            ]
        );
    }

    #[test]
    fn a_type_only_import_is_an_edge_like_any_other() {
        let available = [
            Source::new(
                "app.js",
                "// @flow\nimport type { Control } from '@uniflowed/form';\nexport type C = Control;\n",
            ),
            Source::new(
                "packages/form/index.js",
                "// @flow\nexport type Control = string;\n",
            ),
            Source::new("packages/form/package.json", FORM_MANIFEST),
        ];

        assert_eq!(
            reached(&["app.js"], &available),
            [
                "app.js",
                "packages/form/index.js",
                "packages/form/package.json"
            ]
        );
    }

    #[test]
    fn a_cycle_terminates() {
        let available = [
            Source::new("a.js", "import { b } from './b.js';\nexport const a = b;\n"),
            Source::new("b.js", "import { a } from './a.js';\nexport const b = a;\n"),
        ];

        assert_eq!(reached(&["a.js"], &available), ["a.js", "b.js"]);
    }

    #[test]
    fn a_specifier_a_libdef_declares_is_not_left_for_the_caller() {
        // A caller that went and found `react` on disk would replace Flow's
        // description of it with whatever is installed.
        let available = [Source::new("app.js", "import 'react';\nimport 'nope';\n")];
        let found = closure(
            &["app.js"],
            &available,
            &options::options(&CheckLimits::default()),
            &|specifier| specifier == "react",
        );

        assert_eq!(
            found.unresolved,
            [UnresolvedImport {
                specifier: "nope".into(),
                importer: "app.js".into(),
            }]
        );
    }

    #[test]
    fn a_declared_package_is_not_shadowed_by_a_manifest_in_the_batch() {
        // `flow-typed` is how a project describes an untyped package. A
        // vendored copy or workspace wrapper with the same package name must
        // not change what a bare import means once the declaration exists.
        let available = [
            Source::new("app.js", "import 'editor-pkg';\n"),
            Source::new("vendor/editor-pkg/index.js", "export const anything = 1;\n"),
            Source::new(
                "vendor/editor-pkg/package.json",
                r#"{ "name": "editor-pkg", "main": "index.js" }"#,
            ),
        ];
        let found = closure(
            &["app.js"],
            &available,
            &options::options(&CheckLimits::default()),
            &|specifier| specifier == "editor-pkg",
        );

        assert_eq!(found.reached, [0]);
        assert!(found.unresolved.is_empty());
    }

    #[test]
    fn a_specifier_nothing_answers_is_reported() {
        let available = [Source::new(
            "app.js",
            "import 'react';\nimport './missing.js';\nimport '@uniflowed/nope';\n",
        )];

        assert_eq!(
            unresolved(&["app.js"], &available),
            [
                "./missing.js in app.js",
                "@uniflowed/nope in app.js",
                "react in app.js"
            ]
        );
    }

    #[test]
    fn a_noflow_dependency_is_in_the_batch_and_its_own_imports_are_not() {
        // The checker resolves nothing through a `@noflow` file, so a batch
        // holding what it imports would hold files nothing can reach.
        let available = [
            Source::new("app.js", "import './plain.js';\n"),
            Source::new("plain.js", "// @noflow\nimport './deep.js';\n"),
            Source::new("deep.js", "export const deep = 1;\n"),
        ];

        assert_eq!(reached(&["app.js"], &available), ["app.js", "plain.js"]);
    }

    #[test]
    fn one_specifier_written_in_two_packages_is_two_entries() {
        // ubugeeei-prod/uf#486: the two may mean two different copies of one
        // package, so a caller that deduplicated them to a name could only
        // ever go looking for one.
        let available = [
            Source::new("app.js", "import 'bar';\nimport './lib.js';\n"),
            Source::new("node_modules/foo/index.js", "import 'bar';\n"),
            Source::new("node_modules/foo/package.json", r#"{ "name": "foo" }"#),
            Source::new("lib.js", "import 'foo';\n"),
        ];

        assert_eq!(
            unresolved(&["app.js"], &available),
            ["bar in app.js", "bar in node_modules/foo/index.js"]
        );
    }

    #[test]
    fn a_nested_copy_is_reached_by_the_package_that_holds_it() {
        // The hoisted copy answers the application and the nested one answers
        // `foo`, so the closure holds both files and both manifests.
        let available = [
            Source::new("app.js", "import 'foo';\nimport 'bar';\n"),
            Source::new(
                "node_modules/bar/package.json",
                r#"{ "exports": { ".": "./v2.js" } }"#,
            ),
            Source::new("node_modules/bar/v2.js", "export const bar = 2;\n"),
            Source::new(
                "node_modules/foo/index.js",
                "import { bar } from 'bar';\nexport { bar };\n",
            ),
            Source::new(
                "node_modules/foo/node_modules/bar/package.json",
                r#"{ "exports": { ".": "./v1.js" } }"#,
            ),
            Source::new(
                "node_modules/foo/node_modules/bar/v1.js",
                "export const bar = 1;\n",
            ),
            Source::new(
                "node_modules/foo/package.json",
                r#"{ "exports": { ".": "./index.js" } }"#,
            ),
        ];

        assert_eq!(
            reached(&["app.js"], &available),
            [
                "app.js",
                "node_modules/bar/package.json",
                "node_modules/bar/v2.js",
                "node_modules/foo/index.js",
                "node_modules/foo/node_modules/bar/package.json",
                "node_modules/foo/node_modules/bar/v1.js",
                "node_modules/foo/package.json",
            ]
        );
        assert!(unresolved(&["app.js"], &available).is_empty());
    }

    #[test]
    fn a_seed_that_is_not_available_is_skipped_rather_than_reported() {
        let available = [Source::new("a.js", "export const a = 1;\n")];

        assert_eq!(reached(&["a.js", "gone.js"], &available), ["a.js"]);
        assert!(unresolved(&["gone.js"], &available).is_empty());
    }

    #[test]
    fn every_seed_is_a_root() {
        let available = [
            Source::new("a.js", "import './shared.js';\n"),
            Source::new("b.js", "import './other.js';\n"),
            Source::new("shared.js", "export const s = 1;\n"),
            Source::new("other.js", "export const o = 1;\n"),
        ];

        // In the order `available` gave them, not in discovery order: the
        // batch has to be a function of the sources and the seeds alone.
        assert_eq!(
            reached(&["a.js", "b.js"], &available),
            ["a.js", "b.js", "shared.js", "other.js"]
        );
    }
}
