//! What uf is about to spawn, without spawning it.
//!
//! [`run_operation`] is a process spawn and cannot be asserted on in a unit
//! test. [`invocation_for`] is the whole of its argument handling, which is the
//! part with the decisions in it: which flag uf adds, where it goes, and which
//! operands never reach a command line at all.

use super::*;
use crate::command::{DependencyKind, PROGRAMS};
use crate::detect::YarnEdition;

fn operands(list: &[&str]) -> Vec<String> {
    list.iter().map(ToString::to_string).collect()
}

fn args(
    manager: PackageManager,
    operation: Operation<'_>,
    list: &[&str],
    allow_scripts: bool,
) -> Vec<String> {
    let invocation = invocation_for(manager, operation, &operands(list), allow_scripts)
        .expect("these operands are passable");
    std::iter::once(invocation.program.to_owned())
        .chain(invocation.args.iter().map(ToString::to_string))
        .collect()
}

/// A range is part of the specifier, not a second argument.
///
/// `react@^18.2.0` split on the `@` would be two packages, one of which does
/// not exist; `"react@>=18 <20"` split on the space would be two more. Neither
/// ever goes through a shell, so neither is ever split.
#[test]
fn a_version_range_stays_inside_one_specifier() {
    for spec in [
        "react",
        "react@^18.2.0",
        "react@>=18 <20",
        "@uniflowed/vite@0.0.0-alpha.7",
        "react@npm:preact@^10",
        "./packages/ui",
        "file:../shared",
    ] {
        let invocation = invocation_for(
            PackageManager::Npm,
            Operation::Add {
                kind: DependencyKind::Prod,
            },
            &operands(&[spec]),
            true,
        )
        .expect("a specifier is passable");

        assert_eq!(
            invocation.args.last().map(std::borrow::Cow::as_ref),
            Some(spec),
            "npm was handed something other than {spec:?}"
        );
    }
}

/// Every specifier is its own argument, in the order they were typed.
#[test]
fn several_specifiers_stay_several_arguments() {
    assert_eq!(
        args(
            PackageManager::Pnpm,
            Operation::Add {
                kind: DependencyKind::Dev,
            },
            &["eslint", "prettier@^3"],
            true,
        ),
        ["pnpm", "add", "--save-dev", "eslint", "prettier@^3"]
    );
}

/// uf's own flag comes before the specifiers, so the `command` row reads as
/// "what uf added" then "what you asked for".
#[test]
fn the_scripts_refusal_precedes_the_specifiers() {
    assert_eq!(
        args(
            PackageManager::Npm,
            Operation::Add {
                kind: DependencyKind::Dev,
            },
            &["eslint"],
            false,
        ),
        ["npm", "install", "--save-dev", "--ignore-scripts", "eslint"]
    );
}

/// A project that allows lifecycle scripts is not told to ignore them.
#[test]
fn allowing_lifecycle_scripts_leaves_the_flag_off() {
    assert_eq!(
        args(PackageManager::Bun, Operation::Install, &[], true),
        ["bun", "install"]
    );
    assert_eq!(
        args(PackageManager::Bun, Operation::Install, &[], false),
        ["bun", "install", "--ignore-scripts"]
    );
}

/// `--ignore-scripts` is an install flag. `pnpm why --ignore-scripts` is an
/// unknown option, and a query that fails on a flag uf added is uf's bug.
#[test]
fn a_read_only_query_is_never_told_to_ignore_scripts() {
    for manager in PackageManager::ALL {
        let invocation = invocation_for(manager, Operation::Why, &operands(&["react"]), false)
            .expect("a package name is passable");
        assert!(
            !invocation.args.iter().any(|arg| arg == "--ignore-scripts"),
            "{manager} why carried an install flag: {invocation}"
        );
        assert_eq!(
            invocation.args.last().map(std::borrow::Cow::as_ref),
            Some("react")
        );
    }
}

/// Every operation, every manager, with an operand: the operand is last and
/// the program is still one uf is allowed to spawn.
#[test]
fn an_operand_is_always_the_last_argument() {
    for manager in PackageManager::ALL {
        for operation in Operation::ALL {
            let invocation = match invocation_for(manager, operation, &operands(&["lodash"]), false)
            {
                Ok(invocation) => invocation,
                // The manager has no such command, which is what
                // `search_is_unsupported_where_the_manager_has_none` asserts.
                Err(ManagerRunError::Unsupported { .. }) => continue,
                Err(error) => panic!("{manager} {operation:?}: {error}"),
            };
            assert!(PROGRAMS.contains(&invocation.program));
            assert_eq!(
                invocation.args.last().map(std::borrow::Cow::as_ref),
                Some("lodash"),
                "{manager} {operation:?} dropped or reordered the operand"
            );
        }
    }
}

/// An operand starting with `-` is a flag to every manager there is, so it is
/// refused here rather than passed on to mean something.
#[test]
fn an_operand_that_would_be_read_as_a_flag_is_refused() {
    for operand in ["--global", "-g", "--save-dev", "-"] {
        let error = invocation_for(
            PackageManager::Npm,
            Operation::Remove,
            &operands(&[operand]),
            false,
        )
        .expect_err("uf does not pass this on");

        let message = error.to_string();
        assert!(message.contains(operand), "{message}");
        assert!(
            message.contains("would read as a flag"),
            "the message has to say why: {message}"
        );
        assert!(
            message.contains("./"),
            "and what to write instead: {message}"
        );
    }
}

/// An empty operand is a package with no name.
#[test]
fn an_empty_operand_is_refused() {
    let error = invocation_for(
        PackageManager::Bun,
        Operation::Add {
            kind: DependencyKind::Prod,
        },
        &operands(&[""]),
        true,
    )
    .expect_err("uf does not pass this on");

    assert!(error.to_string().contains("it is empty"), "{error}");
}

/// A refusal is a refusal for the whole command: one bad operand does not get
/// a half-run that installed the others.
#[test]
fn one_refused_operand_refuses_the_whole_invocation() {
    assert!(
        invocation_for(
            PackageManager::Yarn(YarnEdition::Berry),
            Operation::Add {
                kind: DependencyKind::Peer,
            },
            &operands(&["react", "--force", "react-dom"]),
            true,
        )
        .is_err()
    );
}

/// A scoped name starts with `@`, not with `-`, and is ordinary.
#[test]
fn a_scoped_name_is_an_ordinary_operand() {
    assert_eq!(
        args(
            PackageManager::Yarn(YarnEdition::Classic),
            Operation::Remove,
            &["@uniflowed/vite"],
            false,
        ),
        ["yarn", "remove", "--ignore-scripts", "@uniflowed/vite"]
    );
}

/// The frozen install is the one CI runs, and it takes no operands.
#[test]
fn the_frozen_install_is_the_managers_own_immutable_install() {
    assert_eq!(
        args(PackageManager::Npm, Operation::InstallFrozen, &[], false),
        ["npm", "ci", "--ignore-scripts"]
    );
    assert_eq!(
        args(
            PackageManager::Yarn(YarnEdition::Berry),
            Operation::InstallFrozen,
            &[],
            true,
        ),
        ["yarn", "install", "--immutable"]
    );
}
